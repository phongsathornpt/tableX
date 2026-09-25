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
