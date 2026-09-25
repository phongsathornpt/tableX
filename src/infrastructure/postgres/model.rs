use crate::domain::connection::{ConnectionId, ConnectionSslMode};

/// Credentials and connection settings for a PostgreSQL profile.
///
/// Passwords are kept out of `Debug` output intentionally. The profile is a
/// transport object; credentials are persisted separately in the OS keychain.
#[derive(Clone)]
pub struct PostgresConnectionProfile {
    pub id: ConnectionId,
    pub name: String,
    pub host: String,
    pub port: u16,
    pub database: String,
    pub user: String,
    pub password: Option<String>,
    pub ssl: PostgresSslMode,
    pub reject_unauthorized: bool,
    pub ca_certificate_path: Option<String>,
}

impl PostgresConnectionProfile {
    pub fn new(
        id: impl Into<String>,
        name: impl Into<String>,
        host: impl Into<String>,
        database: impl Into<String>,
        user: impl Into<String>,
    ) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
            host: host.into(),
            port: 5432,
            database: database.into(),
            user: user.into(),
            password: None,
            ssl: PostgresSslMode::Prefer,
            reject_unauthorized: true,
            ca_certificate_path: None,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, serde::Deserialize, serde::Serialize)]
#[serde(rename_all = "lowercase")]
pub enum PostgresSslMode {
    Disable,
    #[default]
    Prefer,
    Require,
}

impl From<ConnectionSslMode> for PostgresSslMode {
    fn from(mode: ConnectionSslMode) -> Self {
        match mode {
            ConnectionSslMode::Disable => Self::Disable,
            ConnectionSslMode::Prefer => Self::Prefer,
            ConnectionSslMode::Require => Self::Require,
        }
    }
}

impl From<PostgresSslMode> for ConnectionSslMode {
    fn from(mode: PostgresSslMode) -> Self {
        match mode {
            PostgresSslMode::Disable => Self::Disable,
            PostgresSslMode::Prefer => Self::Prefer,
            PostgresSslMode::Require => Self::Require,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PostgresServerInfo {
    pub server_version: String,
    pub version: PostgresVersion,
    pub database: String,
    pub user: String,
    pub host: String,
    pub port: u16,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct PostgresVersion {
    pub major: u16,
    pub minor: u16,
    pub patch: u16,
}

impl PostgresVersion {
    pub fn from_server_version_num(value: i32) -> Option<Self> {
        if value <= 0 {
            return None;
        }

        if value < 100_000 {
            let major = (value / 10_000) as u16;
            let minor = ((value / 100) % 100) as u16;
            let patch = (value % 100) as u16;
            return (major > 0).then_some(Self {
                major,
                minor,
                patch,
            });
        }

        Some(Self {
            major: (value / 10_000) as u16,
            minor: ((value / 100) % 100) as u16,
            patch: (value % 100) as u16,
        })
    }
}

impl std::fmt::Display for PostgresVersion {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if self.major >= 10 {
            write!(formatter, "{}.{}", self.major, self.patch)
        } else {
            write!(formatter, "{}.{}.{}", self.major, self.minor, self.patch)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::PostgresVersion;

    #[test]
    fn parses_postgres_18_server_version_num() {
        let version = PostgresVersion::from_server_version_num(180_006).unwrap();
        assert_eq!(version.to_string(), "18.6");
    }

    #[test]
    fn parses_pre_v10_server_version_num() {
        let version = PostgresVersion::from_server_version_num(90624).unwrap();
        assert_eq!(version.to_string(), "9.6.24");
    }
}
