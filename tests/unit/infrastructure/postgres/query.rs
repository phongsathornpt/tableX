use super::{
    MAX_TABLE_FILTER_VALUE_BYTES, build_cell_update_sql, build_filter_predicates,
    build_filtered_offset_sql, build_keyset_sql, build_keyset_sql_with_filters, preview_select_sql,
};
use crate::domain::query::{
    CellUpdateRequest, TableColumnFilter, TableDataCursorDirection, TableFilterOperator,
};

#[test]
fn preview_sql_keeps_ordinary_offset_and_sort_semantics() {
    let sort = ("id".to_owned(), false);
    let sql = preview_select_sql("public", "events", Some(&sort), &[]);
    assert_eq!(
        sql,
        "SELECT * FROM \"public\".\"events\" ORDER BY \"id\" ASC LIMIT $1::bigint OFFSET $2::bigint"
    );
}

#[test]
fn composite_primary_key_sort_adds_stable_tie_break_columns() {
    let sort = ("tenant_id".to_owned(), false);
    let keys = vec!["tenant_id".to_owned(), "event_id".to_owned()];
    let sql = preview_select_sql("public", "events", Some(&sort), &keys);
    assert!(sql.contains("ORDER BY \"tenant_id\" ASC, \"event_id\" ASC"));
}

#[test]
fn keyset_sql_uses_exclusive_boundaries_and_reverses_previous_pages() {
    let relation = "\"public\".\"events\"";
    let ascending_after = build_keyset_sql(
        relation,
        &[("id", "\"pg_catalog\".\"int8\"".to_owned())],
        false,
        TableDataCursorDirection::After,
    );
    assert_eq!(
        ascending_after,
        "SELECT * FROM \"public\".\"events\" WHERE \"id\" > ($1::text)::\"pg_catalog\".\"int8\" ORDER BY \"id\" ASC LIMIT $2::bigint"
    );

    let descending_after = build_keyset_sql(
        relation,
        &[("id", "\"pg_catalog\".\"int8\"".to_owned())],
        true,
        TableDataCursorDirection::After,
    );
    assert!(descending_after.contains("WHERE \"id\" < ($1::text)"));
    assert!(descending_after.contains("ORDER BY \"id\" DESC"));

    let ascending_before = build_keyset_sql(
        relation,
        &[("id", "\"pg_catalog\".\"int8\"".to_owned())],
        false,
        TableDataCursorDirection::Before,
    );
    assert!(ascending_before.contains("WHERE \"id\" < ($1::text)"));
    assert!(ascending_before.contains("ORDER BY \"id\" DESC"));

    let descending_before = build_keyset_sql(
        relation,
        &[("id", "\"pg_catalog\".\"int8\"".to_owned())],
        true,
        TableDataCursorDirection::Before,
    );
    assert!(descending_before.contains("WHERE \"id\" > ($1::text)"));
    assert!(descending_before.contains("ORDER BY \"id\" ASC"));
}

#[test]
fn composite_keyset_uses_typed_tuple_boundaries() {
    let sql = build_keyset_sql(
        "\"public\".\"events\"",
        &[
            ("tenant_id", "\"pg_catalog\".\"int4\"".to_owned()),
            ("event_id", "\"pg_catalog\".\"int8\"".to_owned()),
        ],
        false,
        TableDataCursorDirection::After,
    );
    assert_eq!(
        sql,
        "SELECT * FROM \"public\".\"events\" WHERE (\"tenant_id\", \"event_id\") > (($1::text)::\"pg_catalog\".\"int4\", ($2::text)::\"pg_catalog\".\"int8\") ORDER BY \"tenant_id\" ASC, \"event_id\" ASC LIMIT $3::bigint"
    );
}

#[test]
fn table_filters_quote_identifiers_and_bind_escaped_values() {
    let filters = vec![
        TableColumnFilter {
            column: "display\"name".into(),
            operator: TableFilterOperator::Contains,
            value: Some("a%_\\b'".into()),
        },
        TableColumnFilter {
            column: "deleted_at".into(),
            operator: TableFilterOperator::IsNull,
            value: None,
        },
    ];
    let columns = vec![
        (
            "display\"name".into(),
            "\"pg_catalog\".\"text\"".into(),
            true,
        ),
        (
            "deleted_at".into(),
            "\"pg_catalog\".\"timestamptz\"".into(),
            false,
        ),
    ];
    let (predicate, values) = build_filter_predicates(&filters, &columns).unwrap();
    assert_eq!(
        predicate,
        r#""display""name"::text ILIKE $1::text ESCAPE '\' AND "deleted_at" IS NULL"#
    );
    assert_eq!(values, ["%a\\%\\_\\\\b'%"]);
    let sql =
        build_filtered_offset_sql("\"public\".\"people\"", None, &[], &predicate, values.len());
    assert!(sql.ends_with("LIMIT $2::bigint OFFSET $3::bigint"));
}

#[test]
fn typed_comparisons_and_null_only_filters_keep_placeholder_order() {
    let filters = vec![
        TableColumnFilter {
            column: "amount".into(),
            operator: TableFilterOperator::GreaterThan,
            value: Some("12.50".into()),
        },
        TableColumnFilter {
            column: "active".into(),
            operator: TableFilterOperator::Equals,
            value: Some("true".into()),
        },
    ];
    let columns = vec![
        ("amount".into(), "\"pg_catalog\".\"numeric\"".into(), false),
        ("active".into(), "\"pg_catalog\".\"bool\"".into(), false),
    ];
    let (predicate, values) = build_filter_predicates(&filters, &columns).unwrap();
    assert_eq!(
        predicate,
        r#""amount" > (($1::text)::"pg_catalog"."numeric") AND "active" = (($2::text)::"pg_catalog"."bool")"#
    );
    assert_eq!(values, ["12.50", "true"]);

    let null_filter = [TableColumnFilter {
        column: "active".into(),
        operator: TableFilterOperator::IsNotNull,
        value: None,
    }];
    let (predicate, values) = build_filter_predicates(&null_filter, &columns).unwrap();
    assert_eq!(predicate, "\"active\" IS NOT NULL");
    assert!(values.is_empty());
    let keyset = build_keyset_sql_with_filters(
        "\"public\".\"people\"",
        &[("id", "\"pg_catalog\".\"int8\"".into())],
        false,
        TableDataCursorDirection::After,
        &predicate,
        values.len(),
    );
    assert!(keyset.contains("WHERE \"active\" IS NOT NULL AND \"id\" > ($1::text)"));
    assert!(keyset.ends_with("LIMIT $2::bigint"));
}

#[test]
fn enum_equality_filter_uses_the_enum_type_cast() {
    let filter = [TableColumnFilter {
        column: "state".into(),
        operator: TableFilterOperator::Equals,
        value: Some("in review".into()),
    }];
    let columns = vec![("state".into(), "\"app\".\"workflow_state\"".into(), false)];
    let (predicate, values) = build_filter_predicates(&filter, &columns).unwrap();
    assert_eq!(
        predicate,
        r#""state" = (($1::text)::"app"."workflow_state")"#
    );
    assert_eq!(values, ["in review"]);
}

#[test]
fn filters_reject_unknown_columns_unsupported_contains_and_oversized_values() {
    let columns = vec![("count".into(), "\"pg_catalog\".\"int4\"".into(), false)];
    let unknown = [TableColumnFilter {
        column: "other".into(),
        operator: TableFilterOperator::Equals,
        value: Some("1".into()),
    }];
    assert!(build_filter_predicates(&unknown, &columns).is_err());
    let unsupported = [TableColumnFilter {
        column: "count".into(),
        operator: TableFilterOperator::Contains,
        value: Some("1".into()),
    }];
    assert!(build_filter_predicates(&unsupported, &columns).is_err());
    let oversized = [TableColumnFilter {
        column: "count".into(),
        operator: TableFilterOperator::Equals,
        value: Some("x".repeat(MAX_TABLE_FILTER_VALUE_BYTES + 1)),
    }];
    assert!(build_filter_predicates(&oversized, &columns).is_err());
}

#[test]
fn cell_update_sql_binds_value_and_checks_original_value_and_composite_key() {
    let request = CellUpdateRequest {
        schema: "public".into(),
        table: "items".into(),
        column: "label\"value".into(),
        primary_key_columns: vec!["tenant_id".into(), "item_id".into()],
        primary_key_values: vec!["7".into(), "5a72e91a-9b81-4f81-b6a1-68f0ca7a8b77".into()],
        value: Some("x'; DELETE FROM items; --".into()),
        expected_value: Some("old".into()),
    };
    let sql = build_cell_update_sql(
        "\"public\".\"items\"",
        &request,
        "\"pg_catalog\".\"text\"",
        &[
            ("tenant_id".into(), "\"pg_catalog\".\"int4\"".into()),
            ("item_id".into(), "\"pg_catalog\".\"uuid\"".into()),
        ],
    );
    assert_eq!(
        sql,
        "UPDATE \"public\".\"items\" SET \"label\"\"value\" = (($1::text)::\"pg_catalog\".\"text\") WHERE \"tenant_id\" = (($2::text)::\"pg_catalog\".\"int4\") AND \"item_id\" = (($3::text)::\"pg_catalog\".\"uuid\") AND \"label\"\"value\" IS NOT DISTINCT FROM (($4::text)::\"pg_catalog\".\"text\")"
    );
    assert!(!sql.contains("DELETE FROM items"));
}

#[test]
fn enum_cell_update_casts_bound_values_to_the_qualified_enum_type() {
    let request = CellUpdateRequest {
        schema: "app".into(),
        table: "tickets".into(),
        column: "state".into(),
        primary_key_columns: vec!["id".into()],
        primary_key_values: vec!["8".into()],
        value: Some("in review".into()),
        expected_value: Some("draft".into()),
    };
    let sql = build_cell_update_sql(
        "\"app\".\"tickets\"",
        &request,
        "\"app\".\"workflow_state\"",
        &[("id".into(), "\"pg_catalog\".\"int4\"".into())],
    );
    assert_eq!(
        sql,
        "UPDATE \"app\".\"tickets\" SET \"state\" = (($1::text)::\"app\".\"workflow_state\") WHERE \"id\" = (($2::text)::\"pg_catalog\".\"int4\") AND \"state\" IS NOT DISTINCT FROM (($3::text)::\"app\".\"workflow_state\")"
    );
    assert!(!sql.contains("in review"));
}
