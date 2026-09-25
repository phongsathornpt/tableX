use crate::domain::query::EditableTable;
use crate::infrastructure::postgres::sql::{build_delete_sql, build_update_sql};

#[test]
fn generates_reviewable_update_with_escaped_values() {
    let table = EditableTable {
        schema: "public".into(),
        table: "people".into(),
        primary_key_columns: vec!["id".into()],
    };
    let sql = build_update_sql(
        &table,
        &["id".into(), "name".into()],
        &["7".into(), "O'Reilly".into()],
    )
    .unwrap();
    assert_eq!(
        sql,
        "UPDATE \"public\".\"people\" SET \"name\" = 'O''Reilly' WHERE \"id\" = '7';"
    );
}

#[test]
fn generates_delete_with_composite_key_and_null_literal() {
    let table = EditableTable {
        schema: "sales".into(),
        table: "line_items".into(),
        primary_key_columns: vec!["order_id".into(), "line_id".into()],
    };
    let sql = build_delete_sql(
        &table,
        &["order_id".into(), "line_id".into(), "label".into()],
        &["9".into(), "NULL".into(), "item".into()],
    )
    .unwrap();
    assert_eq!(
        sql,
        "DELETE FROM \"sales\".\"line_items\" WHERE \"order_id\" = '9' AND \"line_id\" IS NULL;"
    );
}
