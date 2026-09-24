use super::provider::{DatabaseError, DatabaseProvider};
use crate::domain::{
    connection::{ConnectionId, ConnectionSummary},
    database_object::{DatabaseObject, DatabaseObjectKind},
};

#[derive(Clone, Default)]
pub struct MockDatabaseProvider;

impl MockDatabaseProvider {
    pub fn new() -> Self {
        Self
    }
}

impl DatabaseProvider for MockDatabaseProvider {
    fn connections(&self) -> Vec<ConnectionSummary> {
        vec![ConnectionSummary {
            id: "local-postgres".into(),
            name: "Local PostgreSQL".into(),
            database: "postgres".into(),
            host: "localhost".into(),
        }]
    }

    fn database_objects(
        &self,
        connection_id: &ConnectionId,
    ) -> Result<Vec<DatabaseObject>, DatabaseError> {
        if connection_id != "local-postgres" {
            return Err(DatabaseError {
                message: format!("unknown mock connection: {connection_id}"),
            });
        }
        Ok(vec![DatabaseObject::branch(
            "database",
            "PostgreSQL 16",
            DatabaseObjectKind::Database,
            vec![
                DatabaseObject::branch(
                    "public",
                    "public",
                    DatabaseObjectKind::Schema,
                    vec![
                        DatabaseObject::leaf("customers", "customers", DatabaseObjectKind::Table),
                        DatabaseObject::leaf("orders", "orders", DatabaseObjectKind::Table),
                        DatabaseObject::leaf("products", "products", DatabaseObjectKind::Table),
                    ],
                ),
                DatabaseObject::leaf("analytics", "analytics", DatabaseObjectKind::Schema),
            ],
        )])
    }
}
