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
#[path = "../../tests/unit/infrastructure/connection_store.rs"]
mod tests;
