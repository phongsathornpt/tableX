use keyring::Entry;

use super::error::DatabaseError;

/// OS-backed password storage.
///
/// The keyring crate selects the native credential store for each desktop
/// platform: Secret Service on Linux, Keychain Services on macOS, and Windows
/// Credential Manager on Windows. Passwords never enter the JSON metadata file.
#[derive(Clone, Copy, Debug, Default)]
pub struct CredentialStore;

impl CredentialStore {
    pub fn new() -> Self {
        Self
    }

    pub fn load(&self, connection_id: &str) -> Result<Option<String>, DatabaseError> {
        let entry = entry(connection_id)?;
        match entry.get_password() {
            Ok(password) => Ok(Some(password)),
            Err(keyring::Error::NoEntry) => Ok(None),
            Err(error) => Err(keyring_error("read", error)),
        }
    }

    pub fn save(&self, connection_id: &str, password: &str) -> Result<(), DatabaseError> {
        entry(connection_id)?
            .set_password(password)
            .map_err(|error| keyring_error("save", error))
    }

    pub fn delete(&self, connection_id: &str) -> Result<(), DatabaseError> {
        match entry(connection_id)?.delete_credential() {
            Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
            Err(error) => Err(keyring_error("delete", error)),
        }
    }
}

fn entry(connection_id: &str) -> Result<Entry, DatabaseError> {
    Entry::new("tablex", connection_id).map_err(|error| keyring_error("initialize", error))
}

fn keyring_error(operation: &str, error: keyring::Error) -> DatabaseError {
    DatabaseError::new(format!(
        "could not {operation} PostgreSQL credential: {error}"
    ))
}
