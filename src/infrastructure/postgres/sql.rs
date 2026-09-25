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
mod tests {
    use super::{build_delete_sql, build_update_sql, quote_identifier, quote_literal};
    use crate::domain::query::EditableTable;

    #[test]
    fn quotes_identifiers_and_literals() {
        assert_eq!(quote_identifier("user\"data"), "\"user\"\"data\"");
        assert_eq!(quote_literal("O'Reilly"), "'O''Reilly'");
    }

    #[test]
    fn builds_review_sql_for_composite_and_null_keys() {
        let table = EditableTable {
            schema: "public".into(),
            table: "items".into(),
            primary_key_columns: vec!["tenant\"id".into(), "item_id".into()],
        };
        let columns = vec!["tenant\"id".into(), "item_id".into(), "label".into()];
        let row = vec!["NULL".into(), "7".into(), "new".into()];

        assert_eq!(
            build_update_sql(&table, &columns, &row),
            Some("UPDATE \"public\".\"items\" SET \"label\" = 'new' WHERE \"tenant\"\"id\" IS NULL AND \"item_id\" = '7';".into())
        );
        assert_eq!(
            build_delete_sql(&table, &columns, &row),
            Some("DELETE FROM \"public\".\"items\" WHERE \"tenant\"\"id\" IS NULL AND \"item_id\" = '7';".into())
        );
    }
}
