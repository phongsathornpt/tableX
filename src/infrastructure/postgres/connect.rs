use std::time::Duration;
use std::{
    future::Future,
    sync::{Arc, OnceLock},
};

use rustls::client::danger::{HandshakeSignatureValid, ServerCertVerified, ServerCertVerifier};
use rustls::pki_types::{CertificateDer, ServerName, UnixTime, pem::PemObject};
use rustls::{ClientConfig, DigitallySignedStruct, SignatureScheme};
use tokio_postgres::{Client, Config, config::SslMode};
use tokio_postgres_rustls::MakeRustlsConnect;

use super::model::{PostgresConnectionProfile, PostgresSslMode};
use crate::infrastructure::error::DatabaseError;

pub(super) const CONNECTION_TIMEOUT: Duration = Duration::from_secs(10);

static NATIVE_ROOT_CERTIFICATES: OnceLock<(rustls::RootCertStore, usize)> = OnceLock::new();
static RING_PROVIDER: OnceLock<()> = OnceLock::new();

pub(super) fn rustls_connector(
    profile: &PostgresConnectionProfile,
) -> Result<MakeRustlsConnect, DatabaseError> {
    if profile.ssl == PostgresSslMode::Require && !profile.reject_unauthorized {
        return Err(DatabaseError::new(
            "SSL mode 'require' must verify the server certificate",
        ));
    }
    RING_PROVIDER.get_or_init(|| {
        let _ = rustls::crypto::ring::default_provider().install_default();
    });
    let (mut roots, added_native) = NATIVE_ROOT_CERTIFICATES
        .get_or_init(|| {
            let native = rustls_native_certs::load_native_certs();
            if !native.errors.is_empty() {
                eprintln!(
                    "PostgreSQL TLS loaded with {} native certificate warning(s)",
                    native.errors.len()
                );
            }
            let mut roots = rustls::RootCertStore::empty();
            let (added, _) = roots.add_parsable_certificates(native.certs);
            (roots, added)
        })
        .clone();
    if added_native == 0 && profile.ca_certificate_path.is_none() && profile.reject_unauthorized {
        return Err(DatabaseError::new(
            "could not load any trusted TLS certificates from the operating system",
        ));
    }
    if let Some(path) = profile.ca_certificate_path.as_deref() {
        load_ca_certificate(&mut roots, path)?;
    }

    let config = if profile.reject_unauthorized {
        ClientConfig::builder()
            .with_root_certificates(roots)
            .with_no_client_auth()
    } else {
        ClientConfig::builder()
            .dangerous()
            .with_custom_certificate_verifier(Arc::new(NoCertificateVerification::new()))
            .with_no_client_auth()
    };
    Ok(MakeRustlsConnect::new(config))
}

fn load_ca_certificate(roots: &mut rustls::RootCertStore, path: &str) -> Result<(), DatabaseError> {
    let certificates = CertificateDer::pem_file_iter(path)
        .map_err(|error| {
            DatabaseError::new(format!("could not read configured CA certificate: {error}"))
        })?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| {
            DatabaseError::new(format!(
                "could not parse configured CA certificate: {error}"
            ))
        })?;
    if certificates.is_empty() {
        return Err(DatabaseError::new(
            "configured CA certificate file contains no PEM certificates",
        ));
    }
    let (added, _) = roots.add_parsable_certificates(certificates);
    if added == 0 {
        return Err(DatabaseError::new(
            "configured CA certificate file contains no usable certificates",
        ));
    }
    Ok(())
}

#[derive(Debug)]
struct NoCertificateVerification {
    supported: rustls::crypto::WebPkiSupportedAlgorithms,
}

impl NoCertificateVerification {
    fn new() -> Self {
        Self {
            supported: rustls::crypto::ring::default_provider().signature_verification_algorithms,
        }
    }
}

impl ServerCertVerifier for NoCertificateVerification {
    fn verify_server_cert(
        &self,
        _end_entity: &CertificateDer<'_>,
        _intermediates: &[CertificateDer<'_>],
        _server_name: &ServerName<'_>,
        _ocsp_response: &[u8],
        _now: UnixTime,
    ) -> Result<ServerCertVerified, rustls::Error> {
        Ok(ServerCertVerified::assertion())
    }

    fn verify_tls12_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, rustls::Error> {
        rustls::crypto::verify_tls12_signature(message, cert, dss, &self.supported)
    }

    fn verify_tls13_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, rustls::Error> {
        rustls::crypto::verify_tls13_signature(message, cert, dss, &self.supported)
    }

    fn supported_verify_schemes(&self) -> Vec<SignatureScheme> {
        self.supported.supported_schemes()
    }
}

pub(super) fn connection_config(profile: &PostgresConnectionProfile) -> Config {
    let mut config = Config::new();
    config
        .host(&profile.host)
        .port(profile.port)
        .user(&profile.user)
        .dbname(&profile.database)
        .connect_timeout(CONNECTION_TIMEOUT)
        .ssl_mode(match profile.ssl {
            PostgresSslMode::Disable => SslMode::Disable,
            PostgresSslMode::Prefer => SslMode::Prefer,
            PostgresSslMode::Require => SslMode::Require,
        });
    if let Some(password) = &profile.password {
        config.password(password);
    }
    config
}

pub(super) fn read_only_connection_config(profile: &PostgresConnectionProfile) -> Config {
    let mut config = connection_config(profile);
    config.options("-c default_transaction_read_only=on -c statement_timeout=30000");
    config
}

pub(super) fn read_only_catalog_connection_config(profile: &PostgresConnectionProfile) -> Config {
    let mut config = connection_config(profile);
    config.options("-c default_transaction_read_only=on -c statement_timeout=30000 -c jit=off");
    config
}

pub(super) fn mutation_connection_config(profile: &PostgresConnectionProfile) -> Config {
    let mut config = connection_config(profile);
    config.options("-c statement_timeout=30000");
    config
}

pub(super) async fn connect_with_timeout<T, F, Fut>(
    connect: Fut,
    profile: &PostgresConnectionProfile,
    format_error: F,
) -> Result<(Client, T), DatabaseError>
where
    Fut: Future<Output = Result<(Client, T), tokio_postgres::Error>>,
    F: FnOnce(tokio_postgres::Error) -> DatabaseError,
{
    tokio::time::timeout(CONNECTION_TIMEOUT, connect)
        .await
        .map_err(|_| {
            DatabaseError::new(format!(
                "PostgreSQL connection to {}:{} timed out after {} seconds",
                profile.host,
                profile.port,
                CONNECTION_TIMEOUT.as_secs()
            ))
        })?
        .map_err(format_error)
}
