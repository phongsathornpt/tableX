mod connect;
mod error;
mod metadata;
pub(crate) mod model;
mod query;
mod runtime;
pub(crate) mod sql;
mod value;

pub(crate) use metadata::TableListRequest;

pub use crate::domain::query::QueryResult;

use std::fmt;
use std::sync::Arc;

use self::model::{PostgresConnectionProfile, PostgresServerInfo};
use crate::domain::database_object::TablePage;
use crate::domain::query::{CellUpdateRequest, MutationResult, QueryResult as DomainQueryResult};
use crate::infrastructure::error::DatabaseError;

/// Stable PostgreSQL provider facade. Transport, TLS, runtime, metadata, and
/// value conversion details remain private to this adapter module.
#[derive(Clone, Default)]
pub struct PostgresProvider {
    metadata_sessions: Arc<metadata::MetadataSessionCache>,
}

#[derive(Debug)]
pub struct PostgresInspection {
    pub server: PostgresServerInfo,
    pub schemas: Vec<String>,
    pub table_page: Option<Result<TablePage, DatabaseError>>,
}

impl PostgresProvider {
    pub fn new() -> Self {
        Self::default()
    }

    /// Drop the cached read-only session before explicitly reconnecting a profile.
    /// This makes credential edits and explicit reconnects take effect immediately.
    pub(crate) fn invalidate_metadata_session(&self) {
        self.metadata_sessions.invalidate_generation();
    }

    pub fn inspect(
        &self,
        profile: PostgresConnectionProfile,
        request: metadata::TableListRequest,
    ) -> Result<PostgresInspection, DatabaseError> {
        metadata::inspect(self, profile, request)
    }

    pub fn list_tables(
        &self,
        profile: PostgresConnectionProfile,
        request: metadata::TableListRequest,
    ) -> Result<TablePage, DatabaseError> {
        metadata::list_tables(self, profile, request)
    }

    pub fn test_connection(
        &self,
        profile: PostgresConnectionProfile,
    ) -> Result<PostgresServerInfo, DatabaseError> {
        metadata::test_connection(self, profile)
    }

    pub fn execute_read_query(
        &self,
        profile: PostgresConnectionProfile,
        sql: String,
    ) -> Result<DomainQueryResult, DatabaseError> {
        query::execute_read(self, profile, sql)
    }

    pub fn is_mutating_query(sql: &str) -> bool {
        query::is_mutating_query(sql)
    }

    pub fn execute_mutation(
        &self,
        profile: PostgresConnectionProfile,
        sql: String,
    ) -> Result<MutationResult, DatabaseError> {
        query::execute_mutation(self, profile, sql)
    }

    pub fn update_table_cell(
        &self,
        profile: PostgresConnectionProfile,
        request: CellUpdateRequest,
    ) -> Result<MutationResult, DatabaseError> {
        query::update_table_cell(self, profile, request)
    }

    pub fn preview_table_page(
        &self,
        profile: PostgresConnectionProfile,
        schema: String,
        table: String,
        request: crate::domain::query::TablePreviewPageRequest,
    ) -> Result<DomainQueryResult, DatabaseError> {
        query::preview_table_page(self, profile, schema, table, request)
    }

    #[cfg(test)]
    fn preview_table(
        &self,
        profile: PostgresConnectionProfile,
        schema: String,
        table: String,
    ) -> Result<DomainQueryResult, DatabaseError> {
        self.preview_table_page(
            profile,
            schema,
            table,
            crate::domain::query::TablePreviewPageRequest {
                limit: 100,
                offset: 0,
                sort: None,
                primary_key_columns: Vec::new(),
                cursor: None,
                filters: Vec::new(),
            },
        )
    }
}

impl fmt::Debug for PostgresProvider {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.debug_struct("PostgresProvider").finish()
    }
}

#[cfg(test)]
#[path = "../../tests/unit/infrastructure/postgres.rs"]
mod tests;
