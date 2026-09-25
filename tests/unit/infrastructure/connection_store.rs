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
