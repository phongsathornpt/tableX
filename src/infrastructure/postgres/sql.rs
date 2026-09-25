use crate::domain::query::EditableTable;

pub(crate) fn quote_identifier(identifier: &str) -> String {
    format!("\"{}\"", identifier.replace('"', "\"\""))
}

pub(crate) fn quote_literal(value: &str) -> String {
    format!("'{}'", value.replace('\'', "''"))
}

pub(crate) fn build_update_sql(
    table: &EditableTable,
    columns: &[String],
    row: &[String],
) -> Option<String> {
    let key_indices = table
        .primary_key_columns
        .iter()
        .map(|key| columns.iter().position(|column| column == key))
        .collect::<Option<Vec<_>>>()?;
    let value_index = columns
        .iter()
        .enumerate()
        .find(|(index, _)| !key_indices.contains(index))
        .map(|(index, _)| index)?;
    let key_filter = build_key_filter(&key_indices, columns, row)?;
    Some(format!(
        "UPDATE {}.{} SET {} = {} WHERE {};",
        quote_identifier(&table.schema),
        quote_identifier(&table.table),
        quote_identifier(&columns[value_index]),
        quote_sql_value(row.get(value_index)?),
        key_filter
    ))
}

pub(crate) fn build_delete_sql(
    table: &EditableTable,
    columns: &[String],
    row: &[String],
) -> Option<String> {
    let key_indices = table
        .primary_key_columns
        .iter()
        .map(|key| columns.iter().position(|column| column == key))
        .collect::<Option<Vec<_>>>()?;
    let key_filter = build_key_filter(&key_indices, columns, row)?;
    Some(format!(
        "DELETE FROM {}.{} WHERE {};",
        quote_identifier(&table.schema),
        quote_identifier(&table.table),
        key_filter
    ))
}

fn build_key_filter(indices: &[usize], columns: &[String], row: &[String]) -> Option<String> {
    let mut key_filters = Vec::with_capacity(indices.len());
    for index in indices {
        let value = row.get(*index)?;
        let column = quote_identifier(&columns[*index]);
        if value == "NULL" {
            key_filters.push(format!("{column} IS NULL"));
        } else {
            key_filters.push(format!("{column} = {}", quote_sql_value(value)));
        }
    }
    Some(key_filters.join(" AND "))
}

fn quote_sql_value(value: &str) -> String {
    if value == "NULL" {
        "NULL".into()
    } else {
        quote_literal(value)
    }
}

#[cfg(test)]
#[path = "../../../tests/unit/infrastructure/postgres/sql.rs"]
mod tests;
