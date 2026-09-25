use std::fmt::Write as _;

use crate::infrastructure::error::DatabaseError;
use tokio_postgres::{
    Row,
    types::{FromSql, Type},
};

#[cfg(test)]
pub(super) fn cell_to_string(
    row: &Row,
    index: usize,
    max_bytes: usize,
) -> Result<String, DatabaseError> {
    cell_to_string_with_null(row, index, max_bytes).map(|(value, _)| value)
}

pub(super) fn cell_to_string_with_null(
    row: &Row,
    index: usize,
    max_bytes: usize,
) -> Result<(String, bool), DatabaseError> {
    let column_type = row.columns()[index].type_();
    if <&str as FromSql>::accepts(column_type) {
        let Ok(value) = row.try_get::<_, Option<&str>>(index) else {
            return fallback_value(column_type.name(), max_bytes).map(|value| (value, false));
        };
        return match value {
            Some(value) => copy_bounded(value, max_bytes).map(|value| (value, false)),
            None => copy_bounded("NULL", max_bytes).map(|value| (value, true)),
        };
    }

    macro_rules! decode_typed {
        ($pg_type:expr, $rust_type:ty, $formatter:expr) => {
            if column_type == &$pg_type {
                if let Ok(value) = row.try_get::<_, Option<$rust_type>>(index) {
                    return format_optional_with_null(value, max_bytes, $formatter);
                }
                return fallback_value(column_type.name(), max_bytes).map(|value| (value, false));
            }
        };
    }

    decode_typed!(
        Type::TIMESTAMPTZ,
        chrono::DateTime<chrono::Utc>,
        |value: chrono::DateTime<chrono::Utc>| value.format("%Y-%m-%d %H:%M:%S%:z").to_string()
    );
    decode_typed!(
        Type::TIMESTAMP,
        chrono::NaiveDateTime,
        |value: chrono::NaiveDateTime| value.to_string()
    );
    decode_typed!(Type::DATE, chrono::NaiveDate, |value: chrono::NaiveDate| {
        value.to_string()
    });
    decode_typed!(Type::TIME, chrono::NaiveTime, |value: chrono::NaiveTime| {
        value.to_string()
    });
    decode_typed!(Type::BOOL, bool, |value: bool| value.to_string());
    decode_typed!(Type::INT2, i16, |value: i16| value.to_string());
    decode_typed!(Type::INT4, i32, |value: i32| value.to_string());
    decode_typed!(Type::INT8, i64, |value: i64| value.to_string());
    decode_typed!(Type::FLOAT4, f32, |value: f32| value.to_string());
    decode_typed!(Type::FLOAT8, f64, |value: f64| value.to_string());

    if column_type == &Type::BYTEA {
        if let Ok(value) = row.try_get::<_, Option<&[u8]>>(index) {
            return match value {
                None => copy_bounded("NULL", max_bytes).map(|value| (value, true)),
                Some(bytes) => {
                    let encoded_len = bytes.len().saturating_mul(2).saturating_add(2);
                    check_size(encoded_len, max_bytes)?;
                    Ok((bytea_to_hex(bytes), false))
                }
            };
        }
        return fallback_value(column_type.name(), max_bytes).map(|value| (value, false));
    }

    fallback_value(column_type.name(), max_bytes).map(|value| (value, false))
}

fn fallback_value(type_name: &str, max_bytes: usize) -> Result<String, DatabaseError> {
    copy_bounded(&format!("<{type_name}>"), max_bytes)
}

fn format_optional_with_null<T>(
    value: Option<T>,
    max_bytes: usize,
    format: impl FnOnce(T) -> String,
) -> Result<(String, bool), DatabaseError> {
    match value {
        Some(value) => copy_bounded(&format(value), max_bytes).map(|value| (value, false)),
        None => copy_bounded("NULL", max_bytes).map(|value| (value, true)),
    }
}

#[cfg(test)]
fn format_optional<T>(
    value: Option<T>,
    max_bytes: usize,
    format: impl FnOnce(T) -> String,
) -> Result<String, DatabaseError> {
    format_optional_with_null(value, max_bytes, format).map(|(value, _)| value)
}

fn copy_bounded(value: &str, max_bytes: usize) -> Result<String, DatabaseError> {
    check_size(value.len(), max_bytes)?;
    Ok(value.to_owned())
}

fn check_size(size: usize, max_bytes: usize) -> Result<(), DatabaseError> {
    if size > max_bytes {
        return Err(DatabaseError::new(
            "Query result exceeds display limits (1 MiB per cell, 32 MiB total). Select fewer or shorter values.",
        ));
    }
    Ok(())
}

fn bytea_to_hex(bytes: &[u8]) -> String {
    let mut hex = String::with_capacity(bytes.len().saturating_mul(2).saturating_add(2));
    hex.push_str("\\x");
    for byte in bytes {
        write!(hex, "{byte:02x}").expect("writing to a String cannot fail");
    }
    hex
}

#[cfg(test)]
pub(super) fn cell_to_string_probe_chain(
    row: &Row,
    index: usize,
    max_bytes: usize,
) -> Result<String, DatabaseError> {
    if let Ok(value) = row.try_get::<_, Option<&str>>(index) {
        return match value {
            Some(value) => copy_bounded(value, max_bytes),
            None => copy_bounded("NULL", max_bytes),
        };
    }
    if let Ok(value) = row.try_get::<_, Option<chrono::DateTime<chrono::Utc>>>(index) {
        return format_optional(value, max_bytes, |value| {
            value.format("%Y-%m-%d %H:%M:%S%:z").to_string()
        });
    }
    if let Ok(value) = row.try_get::<_, Option<chrono::NaiveDateTime>>(index) {
        return format_optional(value, max_bytes, |value| value.to_string());
    }
    if let Ok(value) = row.try_get::<_, Option<chrono::NaiveDate>>(index) {
        return format_optional(value, max_bytes, |value| value.to_string());
    }
    if let Ok(value) = row.try_get::<_, Option<chrono::NaiveTime>>(index) {
        return format_optional(value, max_bytes, |value| value.to_string());
    }
    macro_rules! try_value {
        ($ty:ty) => {
            if let Ok(value) = row.try_get::<_, Option<$ty>>(index) {
                return format_optional(value, max_bytes, |value| value.to_string());
            }
        };
    }
    try_value!(String);
    try_value!(bool);
    try_value!(i16);
    try_value!(i32);
    try_value!(i64);
    try_value!(f32);
    try_value!(f64);
    if let Ok(value) = row.try_get::<_, Option<&[u8]>>(index) {
        return match value {
            None => copy_bounded("NULL", max_bytes),
            Some(bytes) => {
                let encoded_len = bytes.len().saturating_mul(2).saturating_add(2);
                check_size(encoded_len, max_bytes)?;
                Ok(bytea_to_hex(bytes))
            }
        };
    }
    fallback_value(row.columns()[index].type_().name(), max_bytes)
}

#[cfg(test)]
mod tests {
    use super::{bytea_to_hex, check_size};

    #[test]
    fn hex_encodes_bytes_without_changing_lowercase_output() {
        assert_eq!(bytea_to_hex(&[0x00, 0x0a, 0x7f, 0xff]), "\\x000a7fff");
    }

    #[test]
    fn rejects_values_over_the_remaining_result_budget() {
        assert!(check_size(3, 4).is_ok());
        assert!(check_size(5, 4).is_err());
    }
}
