pub type ConnectionId = String;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ConnectionSummary {
    pub id: ConnectionId,
    pub name: String,
    pub database: String,
    pub host: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ConnectionStatus {
    Disconnected,
    Connecting,
    Connected,
    Error(String),
}
