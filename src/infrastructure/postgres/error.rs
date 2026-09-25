use std::error::Error as StdError;

use super::model::{PostgresConnectionProfile, PostgresSslMode};
use crate::infrastructure::error::DatabaseError;

pub(crate) fn format_connection_error(
    profile: &PostgresConnectionProfile,
    error: tokio_postgres::Error,
) -> String {
    format!(
        "could not connect to {}:{} ({}): {}",
        profile.host,
        profile.port,
        profile.database,
        postgres_error_detail(&error)
    )
}

pub(crate) fn format_tls_connection_error(
    profile: &PostgresConnectionProfile,
    error: tokio_postgres::Error,
    mode: PostgresSslMode,
) -> DatabaseError {
    let mode_guidance = match mode {
        PostgresSslMode::Prefer => {
            "SSL mode 'prefer' only falls back when the PostgreSQL server declines TLS"
        }
        PostgresSslMode::Require => "SSL mode 'require' does not permit a plaintext fallback",
        PostgresSslMode::Disable => "SSL mode 'disable' does not use TLS",
    };
    DatabaseError::new(format!(
        "TLS connection to {}:{} ({}) failed: {}. {mode_guidance}; certificate validation remains enabled. Install the server CA, use a hostname covered by the certificate, or choose 'disable' only when plaintext is intentional.",
        profile.host,
        profile.port,
        profile.database,
        postgres_error_detail(&error)
    ))
}

fn postgres_error_detail(error: &tokio_postgres::Error) -> String {
    let mut detail = error.to_string();
    let mut source = error.source();
    while let Some(error) = source {
        let message = error.to_string();
        if !message.is_empty() && !detail.contains(&message) {
            detail.push_str("; ");
            detail.push_str(&message);
        }
        source = error.source();
    }
    normalize_error_terms(detail)
}

pub(crate) fn normalize_error_terms(detail: String) -> String {
    detail.replace("UnknownIssuer", "unknown issuer")
}

#[cfg(test)]
#[path = "../../../tests/unit/infrastructure/postgres/error.rs"]
mod tests;
