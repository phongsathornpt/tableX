pub type ConnectionId = String;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, serde::Deserialize, serde::Serialize)]
#[serde(rename_all = "lowercase")]
pub enum ConnectionSslMode {
    Disable,
    #[default]
    Prefer,
    Require,
}

#[derive(Clone, Debug, PartialEq, Eq, serde::Deserialize, serde::Serialize)]
pub struct ConnectionSummary {
    pub id: ConnectionId,
    pub name: String,
    pub database: String,
    pub host: String,
    #[serde(default = "default_port")]
    pub port: u16,
    #[serde(default = "default_user")]
    pub user: String,
    #[serde(default)]
    pub ssl: ConnectionSslMode,
    #[serde(default = "default_reject_unauthorized")]
    pub reject_unauthorized: bool,
    #[serde(default)]
    pub ca_certificate_path: Option<String>,
}

fn default_port() -> u16 {
    5432
}

fn default_user() -> String {
    "postgres".into()
}

fn default_reject_unauthorized() -> bool {
    true
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ConnectionStatus {
    Disconnected,
    Connecting,
    Connected,
    Error(String),
}
