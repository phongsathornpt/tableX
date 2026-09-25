use std::fmt::Write as _;

use crate::infrastructure::error::DatabaseError;
use tokio_postgres::{
    Row,
    types::{FromSql, Kind, Type},
};

pub(super) fn enum_values(column_type: &Type) -> Option<&[String]> {
    match column_type.kind() {
        Kind::Enum(values) => Some(values),
        Kind::Domain(base_type) => enum_values(base_type),
        _ => None,
    }
}

fn is_numeric_type(column_type: &Type) -> bool {
    column_type == &Type::NUMERIC
        || matches!(column_type.kind(), Kind::Domain(base_type) if is_numeric_type(base_type))
}

fn is_jsonb_type(column_type: &Type) -> bool {
    column_type == &Type::JSONB
        || matches!(column_type.kind(), Kind::Domain(base_type) if is_jsonb_type(base_type))
}

struct EnumText(String);

impl<'a> FromSql<'a> for EnumText {
    fn from_sql(
        _column_type: &Type,
        raw: &'a [u8],
    ) -> Result<Self, Box<dyn std::error::Error + Sync + Send>> {
        Ok(Self(std::str::from_utf8(raw)?.to_owned()))
    }

    fn accepts(column_type: &Type) -> bool {
        enum_values(column_type).is_some()
    }
}

struct NumericText(String);

impl<'a> FromSql<'a> for NumericText {
    fn from_sql(
        _column_type: &Type,
        raw: &'a [u8],
    ) -> Result<Self, Box<dyn std::error::Error + Sync + Send>> {
        decode_numeric(raw)
            .map(Self)
            .map_err(|message| Box::new(std::io::Error::other(message)) as _)
    }

    fn accepts(column_type: &Type) -> bool {
        is_numeric_type(column_type)
    }
}

struct JsonbText<'a>(&'a str);

impl<'a> FromSql<'a> for JsonbText<'a> {
    fn from_sql(
        _column_type: &Type,
        raw: &'a [u8],
    ) -> Result<Self, Box<dyn std::error::Error + Sync + Send>> {
        let Some((&1, json)) = raw.split_first() else {
            return Err(Box::new(std::io::Error::other(
                "unsupported PostgreSQL jsonb binary version",
            )));
        };
        Ok(Self(std::str::from_utf8(json)?))
    }

    fn accepts(column_type: &Type) -> bool {
        is_jsonb_type(column_type)
    }
}

#[cfg(test)]
pub(super) fn cell_to_string(
    row: &Row,
    index: usize,
    max_bytes: usize,
) -> Result<String, DatabaseError> {
    let mut truncated = false;
    cell_to_string_with_null(row, index, max_bytes, &mut truncated).map(|(value, _)| value)
}

pub(super) fn cell_to_string_with_null(
    row: &Row,
    index: usize,
    max_bytes: usize,
    truncated: &mut bool,
) -> Result<(String, bool), DatabaseError> {
    *truncated = false;
    let column_type = row.columns()[index].type_();
    if enum_values(column_type).is_some() {
        return match row.try_get::<_, Option<EnumText>>(index) {
            Ok(Some(value)) => copy_bounded(&value.0, max_bytes).map(|value| (value, false)),
            Ok(None) => copy_bounded("NULL", max_bytes).map(|value| (value, true)),
            Err(_) => fallback_value(column_type.name(), max_bytes).map(|value| (value, false)),
        };
    }
    if is_numeric_type(column_type) {
        return match row.try_get::<_, Option<NumericText>>(index) {
            Ok(Some(value)) => copy_bounded(&value.0, max_bytes).map(|value| (value, false)),
            Ok(None) => copy_bounded("NULL", max_bytes).map(|value| (value, true)),
            Err(_) => fallback_value(column_type.name(), max_bytes).map(|value| (value, false)),
        };
    }
    if is_jsonb_type(column_type) {
        return match row.try_get::<_, Option<JsonbText>>(index) {
            Ok(Some(value)) => {
                let (value, was_truncated) = copy_preview(value.0, max_bytes);
                *truncated = was_truncated;
                Ok((value, false))
            }
            Ok(None) => copy_bounded("NULL", max_bytes).map(|value| (value, true)),
            Err(_) => fallback_value(column_type.name(), max_bytes).map(|value| (value, false)),
        };
    }
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

fn decode_numeric(raw: &[u8]) -> Result<String, &'static str> {
    if raw.len() < 8 || !(raw.len() - 8).is_multiple_of(2) {
        return Err("invalid PostgreSQL numeric binary value");
    }

    let read_u16 = |offset: usize| u16::from_be_bytes([raw[offset], raw[offset + 1]]);
    let digit_count = usize::from(read_u16(0));
    if raw.len() != 8 + digit_count * 2 {
        return Err("invalid PostgreSQL numeric digit count");
    }
    let weight = i16::from_be_bytes([raw[2], raw[3]]);
    let sign = read_u16(4);
    let scale = usize::from(read_u16(6));

    match sign {
        0xC000 => return Ok("NaN".to_owned()),
        0xD000 => return Ok("Infinity".to_owned()),
        0xF000 => return Ok("-Infinity".to_owned()),
        0x0000 | 0x4000 => {}
        _ => return Err("invalid PostgreSQL numeric sign"),
    }

    let mut digits = Vec::with_capacity(digit_count);
    for offset in (8..raw.len()).step_by(2) {
        let digit = read_u16(offset);
        if digit >= 10_000 {
            return Err("invalid PostgreSQL numeric digit");
        }
        digits.push(digit);
    }

    let group_at = |exponent: i32| -> u16 {
        let digit_index = i32::from(weight) - exponent;
        usize::try_from(digit_index)
            .ok()
            .and_then(|index| digits.get(index).copied())
            .unwrap_or(0)
    };

    let mut value = String::new();
    if sign == 0x4000 {
        value.push('-');
    }
    if weight < 0 {
        value.push('0');
    } else {
        for exponent in (0..=i32::from(weight)).rev() {
            let digit = group_at(exponent);
            if exponent == i32::from(weight) {
                write!(value, "{digit}").expect("writing to a String cannot fail");
            } else {
                write!(value, "{digit:04}").expect("writing to a String cannot fail");
            }
        }
    }

    if scale > 0 {
        value.push('.');
        let fraction_start = value.len();
        let fractional_groups = scale.div_ceil(4);
        for exponent in (1..=fractional_groups as i32).map(|group| -group) {
            write!(value, "{:04}", group_at(exponent)).expect("writing to a String cannot fail");
        }
        value.truncate(fraction_start + scale);
    }
    Ok(value)
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

fn copy_preview(value: &str, max_bytes: usize) -> (String, bool) {
    if value.len() <= max_bytes {
        return (value.to_owned(), false);
    }
    let mut end = max_bytes.min(value.len());
    while !value.is_char_boundary(end) {
        end -= 1;
    }
    (value[..end].to_owned(), true)
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
#[path = "../../../tests/unit/infrastructure/postgres/value.rs"]
mod tests;
