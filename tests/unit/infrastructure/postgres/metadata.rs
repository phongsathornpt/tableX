use super::{MAX_TABLE_PAGE_SIZE, TableListRequest};
use crate::domain::database_object::{TableCursor, TableRelationType};

#[test]
fn table_list_request_clamps_page_size_and_trims_search() {
    let request = TableListRequest {
        search: "  orders  ".into(),
        relation_type: Some(TableRelationType::View),
        limit: usize::MAX,
        offset: 200,
        schema: Some("public".into()),
        after: None,
    }
    .normalized()
    .unwrap();

    assert_eq!(request.search, "orders");
    assert_eq!(request.limit, MAX_TABLE_PAGE_SIZE);
    assert_eq!(request.offset, 200);
    assert_eq!(request.relation_type, Some(TableRelationType::View));
}

#[test]
fn table_list_request_rejects_unbounded_search_input() {
    let request = TableListRequest {
        search: "x".repeat(257),
        ..TableListRequest::default()
    };

    assert!(request.normalized().is_err());
}

#[test]
fn table_cursor_is_retained_without_a_schema_filter() {
    let request = TableListRequest {
        after: Some(TableCursor {
            schema: "public".into(),
            table: "orders".into(),
        }),
        ..TableListRequest::default()
    }
    .normalized()
    .unwrap();

    assert_eq!(
        request.after,
        Some(TableCursor {
            schema: "public".into(),
            table: "orders".into(),
        })
    );
}

#[test]
fn schema_filtered_request_rejects_cursor_from_another_schema() {
    let request = TableListRequest {
        schema: Some("public".into()),
        after: Some(TableCursor {
            schema: "archive".into(),
            table: "orders".into(),
        }),
        ..TableListRequest::default()
    }
    .normalized()
    .unwrap();

    assert_eq!(request.after, None);
}
