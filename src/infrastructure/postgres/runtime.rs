use std::sync::OnceLock;

use crate::infrastructure::error::DatabaseError;

static POSTGRES_RUNTIME: OnceLock<Result<tokio::runtime::Runtime, String>> = OnceLock::new();

pub(crate) fn handle() -> Result<tokio::runtime::Handle, DatabaseError> {
    let runtime = POSTGRES_RUNTIME.get_or_init(|| {
        tokio::runtime::Builder::new_multi_thread()
            .worker_threads(2)
            .enable_all()
            .thread_name("tablex-postgres-io")
            .build()
            .map_err(|error| format!("failed to create PostgreSQL runtime: {error}"))
    });
    runtime
        .as_ref()
        .map(|runtime| runtime.handle().clone())
        .map_err(|error| DatabaseError::new(error.clone()))
}

/// Runs the synchronous provider operation on its caller. UI call sites must
/// invoke provider APIs from the GPUI background executor, never from render.
pub(super) fn run<T, F>(operation: F) -> Result<T, DatabaseError>
where
    F: FnOnce() -> Result<T, DatabaseError>,
{
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(operation))
        .unwrap_or_else(|_| Err(DatabaseError::new("PostgreSQL worker operation panicked")))
}
