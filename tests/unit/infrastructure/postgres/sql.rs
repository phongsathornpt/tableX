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
