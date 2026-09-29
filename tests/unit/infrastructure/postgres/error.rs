use super::{format_tls_connection_error, normalize_error_terms};
use crate::infrastructure::postgres::model::{PostgresConnectionProfile, PostgresSslMode};

#[test]
fn tls_errors_explain_certificate_remediation_for_require() {
    let profile = PostgresConnectionProfile::new(
        "ssl-test",
        "SSL test",
        "db.example.com",
        "postgres",
        "postgres",
    );
    let error = tokio_postgres::Error::__private_api_timeout();

    let detail = format_tls_connection_error(&profile, error, PostgresSslMode::Require);

    assert!(
        detail
            .message
            .contains("does not permit a plaintext fallback")
    );
    assert!(detail.message.contains("Install the server CA"));
    assert!(
        detail
            .message
            .contains("certificate validation remains enabled")
    );
}

#[test]
fn normalizes_rustls_unknown_issuer_text() {
    assert_eq!(
        normalize_error_terms("invalid peer certificate: UnknownIssuer".into()),
        "invalid peer certificate: unknown issuer"
    );
}

#[test]
fn formats_postgres_error_from_non_db_and_normalizes() {
    let error = tokio_postgres::Error::__private_api_timeout();
    let formatted = super::format_postgres_error(&error);
    assert_eq!(formatted, "timeout waiting for server");
}
