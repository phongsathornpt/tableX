use std::fs;
use std::path::PathBuf;

use crate::domain::connection::ConnectionSummary;

use super::error::DatabaseError;

/// Durable storage for non-secret connection metadata.
///
/// Passwords deliberately do not appear in `ConnectionSummary` and therefore
/// can never be written by this store. Credentials are kept in the platform
/// keychain by `CredentialStore`.
#[derive(Clone, Debug)]
pub struct ConnectionStore {
    path: PathBuf,
}

impl Default for ConnectionStore {
    fn default() -> Self {
        Self::new()
    }
}

impl ConnectionStore {
    pub fn new() -> Self {
        let path = dirs::config_dir()
            .or_else(dirs::data_local_dir)
            .unwrap_or_else(|| std::env::temp_dir().join("tablex"))
            .join("tablex")
            .join("connections.json");
        Self::from_path(path)
    }

    pub fn from_path(path: impl Into<PathBuf>) -> Self {
        Self { path: path.into() }
    }

    pub fn load(&self) -> Result<Vec<ConnectionSummary>, DatabaseError> {
        let contents = match fs::read_to_string(&self.path) {
            Ok(contents) => contents,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
            Err(error) => {
                return Err(DatabaseError::new(format!(
                    "could not read saved connections: {error}"
                )));
            }
        };

        serde_json::from_str(&contents).map_err(|error| {
            DatabaseError::new(format!("saved connections file is invalid JSON: {error}"))
        })
    }

    pub fn save(&self, connections: &[ConnectionSummary]) -> Result<(), DatabaseError> {
        let directory = self
            .path
            .parent()
            .ok_or_else(|| DatabaseError::new("could not determine config directory"))?;
        fs::create_dir_all(directory).map_err(|error| {
            DatabaseError::new(format!("could not create config directory: {error}"))
        })?;

        let json = serde_json::to_vec_pretty(connections).map_err(|error| {
            DatabaseError::new(format!("could not serialize saved connections: {error}"))
        })?;
        let temporary_path = self
            .path
            .with_extension(format!("json.{}.tmp", std::process::id()));
        fs::write(&temporary_path, json).map_err(|error| {
            DatabaseError::new(format!("could not write saved connections: {error}"))
        })?;
        fs::rename(&temporary_path, &self.path).map_err(|error| {
            DatabaseError::new(format!("could not commit saved connections: {error}"))
        })?;

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&self.path, fs::Permissions::from_mode(0o600)).map_err(
                |error| DatabaseError::new(format!("could not secure saved connections: {error}")),
            )?;
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::PathBuf;

    use super::ConnectionStore;
    use crate::domain::connection::ConnectionSummary;

    fn test_path(name: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "tablex-connection-store-{name}-{}.json",
            std::process::id()
        ))
    }

    fn summary() -> ConnectionSummary {
        ConnectionSummary {
            id: "test".into(),
            name: "Test".into(),
            database: "postgres".into(),
            host: "localhost".into(),
            port: 5432,
            user: "postgres".into(),
            ssl: crate::domain::connection::ConnectionSslMode::Prefer,
            reject_unauthorized: true,
            ca_certificate_path: None,
        }
    }

    #[test]
    fn missing_store_loads_as_empty() {
        let path = test_path("empty");
        let _ = fs::remove_file(&path);
        assert!(ConnectionStore::from_path(path).load().unwrap().is_empty());
    }

    #[test]
    fn metadata_round_trips_without_passwords() {
        let path = test_path("round-trip");
        let _ = fs::remove_file(&path);
        let store = ConnectionStore::from_path(&path);
        let expected = vec![summary()];

        store.save(&expected).unwrap();

        assert_eq!(store.load().unwrap(), expected);
        let json = fs::read_to_string(&path).unwrap();
        assert!(!json.contains("password"));
        let _ = fs::remove_file(path);
    }

    #[test]
    fn malformed_json_is_reported() {
        let path = test_path("malformed");
        fs::write(&path, "not-json").unwrap();

        let error = ConnectionStore::from_path(&path).load().unwrap_err();

        assert!(error.message.contains("invalid JSON"));
        let _ = fs::remove_file(path);
    }

    #[test]
    fn legacy_metadata_defaults_port_and_user() {
        let path = test_path("legacy");
        fs::write(
            &path,
            r#"[{"id":"legacy","name":"Legacy","database":"postgres","host":"localhost"}]"#,
        )
        .unwrap();

        let connections = ConnectionStore::from_path(&path).load().unwrap();

        assert_eq!(connections[0].port, 5432);
        assert_eq!(connections[0].user, "postgres");
        assert_eq!(
            connections[0].ssl,
            crate::domain::connection::ConnectionSslMode::Prefer
        );
        assert!(connections[0].reject_unauthorized);
        let _ = fs::remove_file(path);
    }

    #[test]
    fn serialized_metadata_does_not_have_a_password_field() {
        let summary = summary();
        let json = serde_json::to_string(&[summary]).unwrap();
        assert!(!json.contains("password"));
        assert!(format!("{:?}", ConnectionStore::new()).contains("connections.json"));
    }
}
