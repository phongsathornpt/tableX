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

#[gpui_kit::test]
fn finish_database_switch_rolls_back_on_error(cx: &mut gpui_kit::TestAppContext) {
    use crate::infrastructure::{ConnectionStore, CredentialStore};
    use gpui_kit::AppContext as _;
    use gpui_kit::component::{Theme, ThemeMode};
    use std::time::{SystemTime, UNIX_EPOCH};

    cx.update(|cx| {
        gpui_kit::init(cx);
        Theme::change(ThemeMode::Dark, None, cx);
        Theme::sync_base(cx);
    });
    let connection_path = std::env::temp_dir().join(format!(
        "tablex-ui-db-switch-test-{}-{}.json",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let store = ConnectionStore::from_path(connection_path);
    let mut connections = store.load().unwrap_or_default();
    connections.push(crate::domain::connection::ConnectionSummary {
        id: "conn-switch".into(),
        name: "Test Server".into(),
        database: "primary_db".into(),
        host: "localhost".into(),
        port: 5432,
        user: "postgres".into(),
        ssl: crate::domain::connection::ConnectionSslMode::Disable,
        reject_unauthorized: false,
        ca_certificate_path: None,
    });
    store.save(&connections).unwrap();

    let window_handle = cx.add_window(move |window, cx| {
        crate::ui::DatabaseWorkspace::new_with_stores(window, cx, store, CredentialStore::new())
    });
    let workspace = cx
        .read_window(&window_handle, |workspace, _| workspace)
        .unwrap();

    cx.update(|cx| {
        workspace.update(cx, |ws, cx| {
            ws.active_database = Some("primary_db".into());
            ws.database_switch_loading = true;
            super::finish_database_switch(
                ws,
                &"conn-switch".into(),
                ws.connection_generation,
                ws.table_page_generation,
                Some("primary_db".into()),
                "target_failed_db".into(),
                Err(crate::infrastructure::DatabaseError::new(
                    "FATAL: database does not accept connections",
                )),
                cx,
            );
        });
        let ws = workspace.read(cx);
        assert_eq!(ws.active_database.as_deref(), Some("primary_db"));
        assert!(!ws.database_switch_loading);
        assert!(
            ws.notice
                .as_ref()
                .unwrap()
                .title
                .contains("Failed to switch")
        );
        assert!(
            ws.notice
                .as_ref()
                .unwrap()
                .message
                .contains("Remaining on primary_db")
        );
    });
}
