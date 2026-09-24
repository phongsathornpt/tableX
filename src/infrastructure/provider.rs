use crate::domain::{
    connection::{ConnectionId, ConnectionSummary},
    database_object::DatabaseObject,
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DatabaseError {
    pub message: String,
}

pub trait DatabaseProvider: Clone + 'static {
    fn connections(&self) -> Vec<ConnectionSummary>;
    fn database_objects(
        &self,
        connection_id: &ConnectionId,
    ) -> Result<Vec<DatabaseObject>, DatabaseError>;
}
