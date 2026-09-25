use super::{remember_page_cursor, table_cursor_for_offset};
use crate::domain::database_object::TableCursor;

#[test]
fn schema_pagination_keeps_cursor_history_for_previous_pages() {
    let mut cursors = vec![None];
    remember_page_cursor(
        &mut cursors,
        0,
        true,
        Some(TableCursor {
            schema: "public".into(),
            table: "orders_100".into(),
        }),
    );
    remember_page_cursor(
        &mut cursors,
        1,
        true,
        Some(TableCursor {
            schema: "public".into(),
            table: "orders_200".into(),
        }),
    );

    assert_eq!(
        table_cursor_for_offset(&cursors, 0, 100),
        None,
        "the first page has no cursor"
    );
    assert_eq!(
        table_cursor_for_offset(&cursors, 100, 100),
        Some(TableCursor {
            schema: "public".into(),
            table: "orders_100".into(),
        })
    );
    assert_eq!(
        table_cursor_for_offset(&cursors, 200, 100),
        Some(TableCursor {
            schema: "public".into(),
            table: "orders_200".into(),
        })
    );
}

#[test]
fn reloading_a_page_discards_later_cursor_history() {
    let cursor = TableCursor {
        schema: "public".into(),
        table: "orders_100".into(),
    };
    let mut cursors = vec![None, Some(cursor.clone()), Some(cursor)];
    remember_page_cursor(&mut cursors, 1, false, None);

    assert_eq!(
        cursors,
        [
            None,
            Some(TableCursor {
                schema: "public".into(),
                table: "orders_100".into(),
            })
        ]
    );
}
