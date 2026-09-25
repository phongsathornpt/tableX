use super::{
    EnumText, JsonbText, NumericText, bytea_to_hex, check_size, copy_preview, decode_numeric,
    enum_values, is_jsonb_type, is_numeric_type,
};
use tokio_postgres::types::{FromSql, Kind, Type};

#[test]
fn hex_encodes_bytes_without_changing_lowercase_output() {
    assert_eq!(bytea_to_hex(&[0x00, 0x0a, 0x7f, 0xff]), "\\x000a7fff");
}

#[test]
fn rejects_values_over_the_remaining_result_budget() {
    assert!(check_size(3, 4).is_ok());
    assert!(check_size(5, 4).is_err());
}

#[test]
fn truncates_cell_previews_on_utf8_boundaries() {
    assert_eq!(copy_preview("abécd", 3), ("ab".to_owned(), true));
    assert_eq!(copy_preview("short", 8), ("short".to_owned(), false));
    assert_eq!(copy_preview("large", 0), (String::new(), true));
}

#[test]
fn reads_enum_labels_and_enum_text_from_postgres_type_metadata() {
    let labels = vec![
        "draft".to_owned(),
        "in review".to_owned(),
        "ready".to_owned(),
    ];
    let enum_type = Type::new(
        "mood".into(),
        90_001,
        Kind::Enum(labels.clone()),
        "public".into(),
    );
    assert_eq!(enum_values(&enum_type), Some(labels.as_slice()));
    assert!(EnumText::accepts(&enum_type));
    assert_eq!(
        EnumText::from_sql(&enum_type, b"in review").unwrap().0,
        "in review"
    );

    let domain_type = Type::new(
        "mood_domain".into(),
        90_002,
        Kind::Domain(enum_type),
        "public".into(),
    );
    assert_eq!(enum_values(&domain_type), Some(labels.as_slice()));
    assert!(EnumText::accepts(&domain_type));
}

#[test]
fn decodes_numeric_binary_without_losing_precision_or_scale() {
    let numeric = |weight: i16, sign: u16, scale: u16, digits: &[u16]| {
        let mut raw = Vec::with_capacity(8 + digits.len() * 2);
        raw.extend_from_slice(&(digits.len() as u16).to_be_bytes());
        raw.extend_from_slice(&weight.to_be_bytes());
        raw.extend_from_slice(&sign.to_be_bytes());
        raw.extend_from_slice(&scale.to_be_bytes());
        for digit in digits {
            raw.extend_from_slice(&digit.to_be_bytes());
        }
        raw
    };

    assert_eq!(
        decode_numeric(&numeric(1, 0x0000, 4, &[12, 3456, 7800])).unwrap(),
        "123456.7800"
    );
    assert_eq!(
        decode_numeric(&numeric(
            7,
            0x0000,
            4,
            &[12, 3456, 7890, 1234, 5678, 9012, 3456, 7890, 1234]
        ))
        .unwrap(),
        "123456789012345678901234567890.1234"
    );
    assert_eq!(
        decode_numeric(&numeric(-2, 0x4000, 6, &[1200])).unwrap(),
        "-0.000012"
    );
    assert_eq!(
        decode_numeric(&numeric(0, 0x0000, 3, &[])).unwrap(),
        "0.000"
    );
    assert_eq!(decode_numeric(&numeric(0, 0xC000, 0, &[])).unwrap(), "NaN");
    assert_eq!(
        decode_numeric(&numeric(0, 0xD000, 0, &[])).unwrap(),
        "Infinity"
    );
    assert_eq!(
        decode_numeric(&numeric(0, 0xF000, 0, &[])).unwrap(),
        "-Infinity"
    );
    assert!(decode_numeric(&[0, 1]).is_err());
}

#[test]
fn supports_numeric_and_jsonb_types_and_domains() {
    let numeric_domain = Type::new(
        "money_amount".into(),
        90_003,
        Kind::Domain(Type::NUMERIC),
        "public".into(),
    );
    assert!(NumericText::accepts(&numeric_domain));
    assert!(is_numeric_type(&numeric_domain));

    let jsonb_domain = Type::new(
        "document".into(),
        90_004,
        Kind::Domain(Type::JSONB),
        "public".into(),
    );
    assert!(JsonbText::accepts(&jsonb_domain));
    assert!(is_jsonb_type(&jsonb_domain));
    assert_eq!(
        JsonbText::from_sql(&Type::JSONB, b"\x01{\"amount\":12345678901234567890}")
            .unwrap()
            .0,
        "{\"amount\":12345678901234567890}"
    );
    assert!(JsonbText::from_sql(&Type::JSONB, b"\x02{}").is_err());
}
