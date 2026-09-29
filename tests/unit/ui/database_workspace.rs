use std::collections::HashSet;
use std::rc::Rc;
use std::sync::Arc;
use std::time::{Instant, SystemTime, UNIX_EPOCH};

use gpui_kit::component::{Theme, ThemeMode};
use gpui_kit::{AppContext as _, TestAppContext, point, px, test::TestWindowExt};

use super::connection_editor::missing_required_fields_message;
use super::{DatabaseWorkspace, MoveDirection, ObjectExplorer};
use crate::domain::database_object::{TableRelationType, TableSummary};
use crate::domain::query::{EditableTable, QueryResult, TableFilterOperator};
use crate::infrastructure::{ConnectionStore, CredentialStore};

#[test]
fn missing_required_fields_message_only_lists_empty_fields() {
    assert_eq!(
        missing_required_fields_message("", "localhost", "postgres", "postgres").as_deref(),
        Some("Name is required")
    );
    assert_eq!(
        missing_required_fields_message("", "", "", "").as_deref(),
        Some("Name, host, database, and user are required")
    );
    assert_eq!(
        missing_required_fields_message("analytics", "localhost", "postgres", "postgres"),
        None
    );
}

#[gpui_kit::test]
fn empty_connection_store_uses_empty_state_without_global_notice(cx: &mut TestAppContext) {
    cx.update(|cx| {
        gpui_kit::init(cx);
        Theme::change(ThemeMode::Dark, None, cx);
        Theme::sync_base(cx);
    });
    let connection_path = std::env::temp_dir().join(format!(
        "tablex-ui-empty-connections-{}-{}.json",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let window_handle = cx.add_window(move |window, cx| {
        DatabaseWorkspace::new_with_stores(
            window,
            cx,
            ConnectionStore::from_path(connection_path),
            CredentialStore::new(),
        )
    });

    let workspace = cx
        .read_window(&window_handle, |workspace, _| workspace)
        .unwrap();
    cx.update(|cx| {
        let workspace = workspace.read(cx);
        assert!(workspace.connections.is_empty());
        assert!(workspace.notice.is_none());
    });
    cx.update_window(window_handle.into(), |_, window, cx| {
        window.render_frame(cx);
        assert!(window.try_find("add-connection").is_some());
        assert!(window.try_find("browse-database").is_none());
        assert!(window.try_find("new-query").is_none());
    })
    .unwrap();
}

#[gpui_kit::test]
fn table_grid_supports_inline_edit_and_column_filter_controls(cx: &mut TestAppContext) {
    assert!(super::is_inline_edit_type(Some("numeric")));
    assert!(super::is_inline_edit_type(Some("jsonb")));
    cx.update(|cx| {
        gpui_kit::init(cx);
        Theme::change(ThemeMode::Dark, None, cx);
        Theme::sync_base(cx);
    });
    let connection_path = std::env::temp_dir().join(format!(
        "tablex-ui-inline-test-{}-{}.json",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let window_handle = cx.add_window(move |window, cx| {
        DatabaseWorkspace::new_with_stores(
            window,
            cx,
            ConnectionStore::from_path(connection_path),
            CredentialStore::new(),
        )
    });
    let workspace = cx
        .read_window(&window_handle, |workspace, _| workspace)
        .unwrap();
    cx.update(|cx| {
        workspace.update(cx, |workspace, cx| {
            workspace.object_explorer = Some(ObjectExplorer::new(vec!["public".into()]));
            workspace.selected_table = Some(("public".into(), "people".into()));
            workspace.table_sidebar_visible = false;
            workspace.query_dock_tab = super::QueryDockTab::Results;
            let result = QueryResult {
                columns: vec!["id".into(), "name".into(), "active".into(), "state".into()],
                column_types: vec!["int4".into(), "text".into(), "bool".into(), "mood".into()],
                column_enum_values: vec![
                    None,
                    None,
                    None,
                    Some(vec!["draft".into(), "in review".into(), "ready".into()]),
                ],
                rows: vec![vec![
                    "1".into(),
                    "Alice".into(),
                    "true".into(),
                    "ready".into(),
                ]],
                null_cells: vec![vec![false; 4]],
                truncated_cells: vec![vec![false; 4]],
                offset: 0,
                limit: 25,
                has_next: false,
                truncated: false,
                editable: Some(EditableTable {
                    schema: "public".into(),
                    table: "people".into(),
                    primary_key_columns: vec!["id".into()],
                }),
            };
            workspace.result_column_widths =
                super::super::homepage::query::result_column_widths(&result);
            workspace.query_result = Some(Arc::new(result));
            cx.notify();
        });
    });

    cx.update_window(window_handle.into(), |_, window, cx| {
        window.render_frame(cx);
        workspace.update(cx, |workspace, cx| {
            workspace.begin_table_cell_edit(0, 1, window, cx);
        });
        window.render_frame(cx);
        assert!(window.try_find("cell-edit-save").is_some());
        assert!(workspace.read(cx).active_cell_edit().is_some());
        window.press("escape", cx);
        assert!(workspace.read(cx).active_cell_edit().is_none());

        workspace.update(cx, |workspace, cx| {
            workspace.begin_table_cell_edit(0, 3, window, cx);
        });
        window.render_frame(cx);
        assert!(window.try_find("enum-cell-editor-0-3").is_some());
        let enum_edit = workspace.read(cx).active_cell_edit().unwrap();
        assert_eq!(enum_edit.enum_value.as_deref(), Some("ready"));
        assert_eq!(enum_edit.enum_values.as_deref().unwrap().len(), 3);
        workspace.update(cx, |workspace, cx| {
            workspace.select_table_cell_enum_value("in review".into(), window, cx);
        });
        assert_eq!(
            workspace
                .read(cx)
                .active_cell_edit()
                .and_then(|edit| edit.enum_value),
            Some("in review".into())
        );
        workspace.update(cx, |workspace, cx| workspace.cancel_table_cell_edit(cx));

        window.click("filter-column-1", cx);
        window.render_frame(cx);
        assert!(window.try_find("apply-column-filter-1").is_some());
        let (editing_column, operator, _) = workspace.read(cx).table_filter_editor();
        assert_eq!(editing_column.as_deref(), Some("name"));
        assert_eq!(operator, TableFilterOperator::Contains);

        workspace.update(cx, |workspace, cx| {
            workspace.table_filter_input.update(cx, |input, cx| {
                input.set_value("Alice", window, cx);
            });
        });
        window.render_frame(cx);
        window.click("apply-column-filter-1", cx);

        window.click("filter-column-3", cx);
        window.render_frame(cx);
        assert!(window.try_find("enum-filter-values-3").is_some());
        workspace.update(cx, |workspace, cx| {
            workspace.set_table_filter_input("ready".into(), window, cx);
        });
        window.render_frame(cx);
        window.click("apply-column-filter-3", cx);
    })
    .unwrap();
    cx.update(|cx| {
        let filters = workspace.read(cx).table_column_filters();
        assert_eq!(filters.len(), 2);
        assert_eq!(filters[0].column, "name");
        assert_eq!(filters[0].operator, TableFilterOperator::Contains);
        assert_eq!(filters[0].value.as_deref(), Some("Alice"));
        assert_eq!(filters[1].column, "state");
        assert_eq!(filters[1].operator, TableFilterOperator::Equals);
        assert_eq!(filters[1].value.as_deref(), Some("ready"));
    });
}

#[gpui_kit::test]
fn macos_source_list_and_toolbar_toggles_and_navigation(cx: &mut TestAppContext) {
    cx.update(|cx| {
        gpui_kit::init(cx);
        Theme::change(ThemeMode::Dark, None, cx);
        Theme::sync_base(cx);
    });
    let connection_path = std::env::temp_dir().join(format!(
        "tablex-ui-macos-test-{}-{}.json",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let store = ConnectionStore::from_path(connection_path);
    let mut connections = store.load().unwrap_or_default();
    connections.push(crate::domain::connection::ConnectionSummary {
        id: "conn-macos".into(),
        name: "macOS DB".into(),
        database: "postgres".into(),
        host: "localhost".into(),
        port: 5432,
        user: "postgres".into(),
        ssl: crate::domain::connection::ConnectionSslMode::Prefer,
        reject_unauthorized: false,
        ca_certificate_path: None,
    });
    store.save(&connections).unwrap();

    let window_handle = cx.add_window(move |window, cx| {
        DatabaseWorkspace::new_with_stores(window, cx, store, CredentialStore::new())
    });
    let workspace = cx
        .read_window(&window_handle, |workspace, _| workspace)
        .unwrap();

    cx.update_window(window_handle.into(), |_, window, cx| {
        window.render_frame(cx);

        // Verify toolbar controls
        assert!(window.try_find("toggle-sidebar-button").is_some());
        assert!(window.try_find("toolbar-toggle-sql").is_some());
        assert!(window.try_find("toolbar-refresh-objects").is_some());
        assert!(window.try_find("connection-selector").is_some());
        assert!(window.try_find("database-selector").is_some());

        // Test sidebar toggle
        assert!(workspace.read(cx).table_sidebar_visible);
        window.click("toggle-sidebar-button", cx);
        assert!(!workspace.read(cx).table_sidebar_visible);
        window.click("toggle-sidebar-button", cx);
        assert!(workspace.read(cx).table_sidebar_visible);

        // Set up explorer to test Source List workspace navigation
        workspace.update(cx, |workspace, cx| {
            workspace.object_explorer = Some(ObjectExplorer::new(vec!["public".into()]));
            workspace.selected_table = Some(("public".into(), "users".into()));
            cx.notify();
        });
        window.render_frame(cx);

        // Verify Source List navigation items
        assert!(window.try_find("workspace-nav-tables").is_some());
        assert!(window.try_find("workspace-nav-sql").is_some());
        assert!(window.try_find("connection-settings-nav").is_some());

        // Navigate via Source List
        window.click("workspace-nav-sql", cx);
        assert!(workspace.read(cx).sql_console_expanded);
        assert_eq!(
            workspace.read(cx).query_dock_tab,
            super::QueryDockTab::Query
        );

        window.click("workspace-nav-tables", cx);
        assert_eq!(
            workspace.read(cx).query_dock_tab,
            super::QueryDockTab::Results
        );

        // Toggle SQL console drawer via toolbar button
        assert!(workspace.read(cx).sql_console_expanded);
        window.click("toolbar-toggle-sql", cx);
        assert!(!workspace.read(cx).sql_console_expanded);
        window.click("toolbar-toggle-sql", cx);
        assert!(workspace.read(cx).sql_console_expanded);
    })
    .unwrap();
}

#[gpui_kit::test]
fn rapid_table_switching_supersedes_earlier_generations_and_cleans_slate(cx: &mut TestAppContext) {
    cx.update(|cx| {
        gpui_kit::init(cx);
        Theme::change(ThemeMode::Dark, None, cx);
        Theme::sync_base(cx);
    });
    let connection_path = std::env::temp_dir().join(format!(
        "tablex-ui-table-switch-test-{}-{}.json",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let store = ConnectionStore::from_path(connection_path);
    let mut connections = store.load().unwrap_or_default();
    let conn_id = crate::domain::connection::ConnectionId::new();
    connections.push(crate::domain::connection::ConnectionSummary {
        id: conn_id.clone(),
        name: "test".to_string(),
        host: "localhost".to_string(),
        port: 5432,
        database: "testdb".to_string(),
        user: "postgres".to_string(),
        ssl: crate::domain::connection::ConnectionSslMode::Disable,
        reject_unauthorized: false,
        ca_certificate_path: None,
    });
    store.save(&connections).unwrap();

    let window_conn_id = conn_id.clone();
    let window_handle = cx.add_window(move |window, cx| {
        let mut workspace =
            DatabaseWorkspace::new_with_stores(window, cx, store, CredentialStore::new());
        workspace.workspace.selected_connection = Some(window_conn_id);
        workspace.connection_status = super::ConnectionStatus::Connected;
        workspace
    });
    let workspace = cx
        .read_window(&window_handle, |workspace, _| workspace)
        .unwrap();

    cx.update_window(window_handle.into(), |_, window, cx| {
        // Set up initial state as if Table A was loaded with custom sort, filter, and offset
        workspace.update(cx, |ws, cx| {
            ws.selected_table = Some(("public".into(), "table_a".into()));
            ws.table_data_sort = Some(("created_at".into(), true));
            ws.table_data_offset = 50;
            ws.table_column_filters = vec![crate::domain::query::TableColumnFilter {
                column: "status".into(),
                operator: TableFilterOperator::Equals,
                value: Some("active".into()),
            }];
            ws.query_result = Some(Arc::new(QueryResult {
                columns: vec!["id".into(), "created_at".into()],
                column_types: vec!["int4".into(), "timestamptz".into()],
                column_enum_values: vec![None, None],
                rows: vec![vec!["1".into(), "2026-01-01".into()]],
                null_cells: vec![vec![false, false]],
                truncated_cells: vec![vec![false, false]],
                offset: 50,
                limit: 25,
                has_next: true,
                editable: None,
                truncated: false,
            }));
            cx.notify();
        });

        // Trigger preview on Table B while Table A was running
        workspace.update(cx, |ws, cx| {
            super::query::preview_table_page(ws, "public", "table_b", window, cx);
        });

        // Verify clean slate immediately after Table B click:
        let ws_read = workspace.read(cx);
        assert_eq!(
            ws_read.selected_table,
            Some(("public".into(), "table_b".into()))
        );
        assert_eq!(ws_read.table_data_sort, None); // Sort reset
        assert_eq!(ws_read.table_data_offset, 0); // Offset reset
        assert!(ws_read.table_column_filters.is_empty()); // Filters reset
        assert!(ws_read.query_result.is_none()); // Stale Table A data cleared
        assert!(ws_read.table_preview_loading); // Canvas is in loading state
        assert_eq!(ws_read.table_preview_error, None);
        assert_eq!(ws_read.query_dock_tab, super::QueryDockTab::Results); // Switched to Results
        let table_b_generation = ws_read.query_generation;

        // Simulate stale Table A in-flight result returning with OLD generation
        workspace.update(cx, |ws, cx| {
            let stale_result = QueryResult {
                columns: vec!["id".into(), "created_at".into()],
                column_types: vec!["int4".into(), "timestamptz".into()],
                column_enum_values: vec![None, None],
                rows: vec![vec!["999".into(), "stale".into()]],
                null_cells: vec![vec![false, false]],
                truncated_cells: vec![vec![false, false]],
                offset: 50,
                limit: 25,
                has_next: false,
                editable: None,
                truncated: false,
            };
            super::query::finish_table_preview(
                ws,
                &conn_id,
                table_b_generation.wrapping_sub(1), // Stale generation
                Ok(stale_result),
                cx,
            );
        });

        // Verify stale result was discarded:
        let ws_read = workspace.read(cx);
        assert!(ws_read.query_result.is_none());
        assert!(ws_read.table_preview_loading);

        // Simulate Table B result returning with current generation
        workspace.update(cx, |ws, cx| {
            let table_b_result = QueryResult {
                columns: vec!["id".into(), "name".into()],
                column_types: vec!["int4".into(), "text".into()],
                column_enum_values: vec![None, None],
                rows: vec![vec!["1".into(), "Table B row".into()]],
                null_cells: vec![vec![false, false]],
                truncated_cells: vec![vec![false, false]],
                offset: 0,
                limit: 25,
                has_next: false,
                editable: None,
                truncated: false,
            };
            super::query::finish_table_preview(
                ws,
                &conn_id,
                table_b_generation,
                Ok(table_b_result),
                cx,
            );
        });

        // Verify Table B result is accepted and loading finished
        let ws_read = workspace.read(cx);
        assert!(!ws_read.table_preview_loading);
        assert_eq!(
            ws_read.query_result.as_ref().unwrap().rows[0][1],
            "Table B row"
        );
    })
    .unwrap();
}

#[gpui_kit::test]
fn table_preview_error_shows_retryable_state_and_retry_reloads(cx: &mut TestAppContext) {
    cx.update(|cx| {
        gpui_kit::init(cx);
        Theme::change(ThemeMode::Dark, None, cx);
        Theme::sync_base(cx);
    });
    let connection_path = std::env::temp_dir().join(format!(
        "tablex-ui-preview-error-test-{}-{}.json",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let store = ConnectionStore::from_path(connection_path);
    let mut connections = store.load().unwrap_or_default();
    let conn_id = crate::domain::connection::ConnectionId::new();
    connections.push(crate::domain::connection::ConnectionSummary {
        id: conn_id.clone(),
        name: "test".to_string(),
        host: "localhost".to_string(),
        port: 5432,
        database: "testdb".to_string(),
        user: "postgres".to_string(),
        ssl: crate::domain::connection::ConnectionSslMode::Disable,
        reject_unauthorized: false,
        ca_certificate_path: None,
    });
    store.save(&connections).unwrap();

    let window_conn_id = conn_id.clone();
    let window_handle = cx.add_window(move |window, cx| {
        let mut workspace =
            DatabaseWorkspace::new_with_stores(window, cx, store, CredentialStore::new());
        workspace.workspace.selected_connection = Some(window_conn_id);
        workspace.connection_status = super::ConnectionStatus::Connected;
        workspace.object_explorer = Some(ObjectExplorer::new(vec!["public".into()]));
        workspace.selected_table = Some(("public".into(), "restricted".into()));
        workspace
    });
    let workspace = cx
        .read_window(&window_handle, |workspace, _| workspace)
        .unwrap();

    cx.update_window(window_handle.into(), |_, window, cx| {
        let query_gen = workspace.read(cx).query_generation;
        // Simulate query failure
        workspace.update(cx, |ws, cx| {
            super::query::finish_table_preview(
                ws,
                &conn_id,
                query_gen,
                Err(crate::infrastructure::DatabaseError::new(
                    "permission denied for table restricted",
                )),
                cx,
            );
        });

        // Verify error state
        assert_eq!(
            workspace.read(cx).table_preview_error.as_deref(),
            Some("permission denied for table restricted")
        );
        assert!(!workspace.read(cx).table_preview_loading);
        assert!(workspace.read(cx).query_result.is_none());

        // Render frame and verify retry button is present in canvas
        window.render_frame(cx);
        assert!(window.try_find("retry-table-preview").is_some());

        // Click retry
        window.click("retry-table-preview", cx);
        assert!(workspace.read(cx).table_preview_loading);
        assert_eq!(workspace.read(cx).table_preview_error, None);
    })
    .unwrap();
}

#[gpui_kit::test]
fn active_profile_respects_session_database_override(cx: &mut TestAppContext) {
    cx.update(|cx| {
        gpui_kit::init(cx);
        Theme::change(ThemeMode::Dark, None, cx);
        Theme::sync_base(cx);
    });
    let connection_path = std::env::temp_dir().join(format!(
        "tablex-ui-db-override-test-{}-{}.json",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let store = ConnectionStore::from_path(connection_path);
    let mut connections = store.load().unwrap_or_default();
    connections.push(crate::domain::connection::ConnectionSummary {
        id: "conn-1".into(),
        name: "Test Server".into(),
        database: "default_db".into(),
        host: "localhost".into(),
        port: 5432,
        user: "postgres".into(),
        ssl: crate::domain::connection::ConnectionSslMode::Disable,
        reject_unauthorized: false,
        ca_certificate_path: None,
    });
    store.save(&connections).unwrap();

    let window_handle = cx.add_window(move |window, cx| {
        DatabaseWorkspace::new_with_stores(window, cx, store, CredentialStore::new())
    });
    let workspace = cx
        .read_window(&window_handle, |workspace, _| workspace)
        .unwrap();

    cx.update(|cx| {
        let ws = workspace.read(cx);
        let profile = ws.active_profile(&"conn-1".into()).unwrap();
        assert_eq!(profile.database, "default_db");
    });

    cx.update(|cx| {
        workspace.update(cx, |ws, cx| {
            ws.active_database = Some("tenant_analytics".into());
            ws.available_databases = vec!["default_db".into(), "tenant_analytics".into()];
            cx.notify();
        });
    });

    cx.update(|cx| {
        let ws = workspace.read(cx);
        let profile = ws.active_profile(&"conn-1".into()).unwrap();
        assert_eq!(profile.database, "tenant_analytics");
        assert_eq!(ws.available_databases.len(), 2);
    });
}

#[gpui_kit::test]
#[ignore = "headless UI performance benchmark; invoke explicitly with --ignored"]
fn renders_large_database_workspace_headlessly(cx: &mut TestAppContext) {
    const TABLE_COUNT: usize = 500;
    const SCHEMA_COUNT: usize = 500;
    const RESULT_ROWS: usize = 500;
    const RESULT_COLUMNS: usize = 40;
    const WARMUP_FRAMES: usize = 4;
    const MEASURED_FRAMES: usize = 50;

    cx.update(|cx| {
        gpui_kit::init(cx);
        Theme::change(ThemeMode::Dark, None, cx);
        Theme::sync_base(cx);
    });

    let connection_path = std::env::temp_dir().join(format!(
        "tablex-ui-perf-fixture-{}-{}.json",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let window_handle = cx.add_window(move |window, cx| {
        DatabaseWorkspace::new_with_stores(
            window,
            cx,
            ConnectionStore::from_path(connection_path),
            CredentialStore::new(),
        )
    });
    let workspace = cx
        .read_window(&window_handle, |workspace, _| workspace)
        .unwrap();
    cx.update(|cx| {
        workspace.update(cx, |workspace, cx| {
            let schema_names = (0..SCHEMA_COUNT)
                .map(|index| format!("perf_schema_{index:03}"))
                .collect();
            let mut explorer = ObjectExplorer::new(schema_names);
            explorer.set_tables(
                (0..TABLE_COUNT)
                    .map(|index| TableSummary {
                        id: index.to_string(),
                        schema: "public".into(),
                        name: format!("perf_relation_{index:03}"),
                        relation_type: TableRelationType::Table,
                    })
                    .collect(),
                cx,
            );
            workspace.object_explorer = Some(explorer);
            workspace.selected_table = Some(("public".into(), "perf_wide".into()));
            workspace.query_result = Some(Arc::new(QueryResult {
                columns: (0..RESULT_COLUMNS)
                    .map(|index| format!("column_{index:02}"))
                    .collect(),
                column_types: vec!["text".into(); RESULT_COLUMNS],
                column_enum_values: vec![None; RESULT_COLUMNS],
                rows: (0..RESULT_ROWS)
                    .map(|row| {
                        (0..RESULT_COLUMNS)
                            .map(|column| {
                                let value = format!("row_{row:03}_column_{column:02}");
                                if column % 4 == 0 {
                                    format!("{value}_{}", "payload".repeat(146))
                                } else {
                                    value
                                }
                            })
                            .collect()
                    })
                    .collect(),
                null_cells: vec![vec![false; RESULT_COLUMNS]; RESULT_ROWS],
                truncated_cells: vec![vec![false; RESULT_COLUMNS]; RESULT_ROWS],
                offset: 0,
                limit: RESULT_ROWS,
                has_next: false,
                truncated: false,
                editable: Some(EditableTable {
                    schema: "public".into(),
                    table: "perf_wide".into(),
                    primary_key_columns: vec!["column_00".into()],
                }),
            }));
            workspace.table_data_filter = Some("_column_39".into());
            workspace.filtered_table_data_rows = crate::ui::home::query::matching_row_indices(
                workspace.query_result().unwrap(),
                workspace.table_data_filter.as_deref(),
                workspace.table_data_filter_column.as_deref(),
                workspace.table_data_empty_filter.as_ref(),
                usize::MAX,
            )
            .map(Rc::new);
            workspace.table_data_limit = RESULT_ROWS;
        });
    });

    let mut cached_frame_times = Vec::with_capacity(MEASURED_FRAMES);
    let mut eager_tooltip_frame_times = Vec::with_capacity(MEASURED_FRAMES);
    let mut filter_cache_miss_frame_times = Vec::with_capacity(MEASURED_FRAMES);
    let mut results_only_frame_times = Vec::with_capacity(MEASURED_FRAMES);
    let mut results_without_row_actions_frame_times = Vec::with_capacity(MEASURED_FRAMES);
    let mut navigator_only_frame_times = Vec::with_capacity(MEASURED_FRAMES);
    let mut schema_filter_menu_frame_times = Vec::with_capacity(MEASURED_FRAMES);
    let mut schema_filter_menu_open_time = std::time::Duration::ZERO;
    let mut render_counts = (0, 0);
    crate::ui::home::query::set_eager_cell_tooltips_for_benchmark(false);
    cx.update_window(window_handle.into(), |_, window, cx| {
        window.render_frame(cx);
        let workspace_state = workspace.read(cx);
        let explorer_scroll_handle = workspace_state
            .object_explorer
            .as_ref()
            .expect("large workspace fixture should contain an object explorer")
            .scroll_handle();
        assert!(
            explorer_scroll_handle
                .0
                .borrow()
                .last_item_size
                .is_some_and(|size| size.contents.height > size.item.height)
        );
        let explorer_scroll = explorer_scroll_handle.0.borrow().base_handle.clone();
        explorer_scroll.set_offset(point(px(0.), px(-1_000.)));
        assert!(explorer_scroll.offset().y < px(0.));
        if RESULT_ROWS > 0 {
            assert!(
                workspace_state
                    .table_result_scroll
                    .0
                    .borrow()
                    .last_item_size
                    .is_some_and(|size| size.contents.height > size.item.height)
            );
            let result_scroll = workspace_state
                .table_result_scroll
                .0
                .borrow()
                .base_handle
                .clone();
            assert!(
                workspace_state
                    .table_result_horizontal_scroll
                    .max_offset()
                    .x
                    .as_f32()
                    > 0.
            );
            assert!(
                workspace_state
                    .table_result_horizontal_scroll
                    .bounds()
                    .size
                    .width
                    < result_scroll.bounds().size.width
            );
            let benchmark_columns = (0..RESULT_COLUMNS)
                .map(|index| format!("column_{index}"))
                .collect::<Vec<_>>();
            let benchmark_widths = vec![120.; RESULT_COLUMNS];
            let hidden_columns = HashSet::new();
            let (initial_columns, _, _) = crate::ui::home::query::visible_column_window(
                &benchmark_columns,
                &benchmark_widths,
                &hidden_columns,
                true,
                Some(&workspace_state.table_result_horizontal_scroll),
            );
            assert!(initial_columns.len() < RESULT_COLUMNS);
            workspace_state
                .table_result_horizontal_scroll
                .set_offset(point(px(-1200.), px(0.)));
            window.render_frame(cx);
            let workspace_state = workspace.read(cx);
            let (scrolled_columns, _, _) = crate::ui::home::query::visible_column_window(
                &benchmark_columns,
                &benchmark_widths,
                &hidden_columns,
                true,
                Some(&workspace_state.table_result_horizontal_scroll),
            );
            assert_ne!(initial_columns, scrolled_columns);
        }
        for _ in 0..WARMUP_FRAMES {
            window.render_frame(cx);
        }
        crate::ui::home::query::reset_render_counts_for_benchmark();
        for _ in 0..MEASURED_FRAMES {
            let start = Instant::now();
            window.render_frame(cx);
            cached_frame_times.push(start.elapsed());
        }
        render_counts = crate::ui::home::query::render_counts_for_benchmark();
        crate::ui::home::query::set_eager_cell_tooltips_for_benchmark(true);
        for _ in 0..WARMUP_FRAMES {
            window.render_frame(cx);
        }
        for _ in 0..MEASURED_FRAMES {
            let start = Instant::now();
            window.render_frame(cx);
            eager_tooltip_frame_times.push(start.elapsed());
        }
        crate::ui::home::query::set_eager_cell_tooltips_for_benchmark(false);
        workspace.update(cx, |workspace, cx| {
            workspace.filtered_table_data_rows = None;
            cx.notify();
        });
        for _ in 0..WARMUP_FRAMES {
            window.render_frame(cx);
        }
        for _ in 0..MEASURED_FRAMES {
            let start = Instant::now();
            window.render_frame(cx);
            filter_cache_miss_frame_times.push(start.elapsed());
        }

        workspace.update(cx, |workspace, cx| {
            workspace.table_sidebar_visible = false;
            workspace.table_data_filter = None;
            workspace.table_data_filter_column = None;
            workspace.table_data_empty_filter = None;
            workspace.filtered_table_data_rows = None;
            workspace.table_data_filter_pending = false;
            cx.notify();
        });
        for _ in 0..WARMUP_FRAMES {
            window.render_frame(cx);
        }
        for _ in 0..MEASURED_FRAMES {
            let start = Instant::now();
            window.render_frame(cx);
            results_only_frame_times.push(start.elapsed());
        }

        crate::ui::home::query::set_row_sql_actions_for_benchmark(false);
        for _ in 0..WARMUP_FRAMES {
            window.render_frame(cx);
        }
        for _ in 0..MEASURED_FRAMES {
            let start = Instant::now();
            window.render_frame(cx);
            results_without_row_actions_frame_times.push(start.elapsed());
        }
        crate::ui::home::query::set_row_sql_actions_for_benchmark(true);

        workspace.update(cx, |workspace, cx| {
            workspace.table_sidebar_visible = true;
            workspace.query_result = None;
            cx.notify();
        });
        for _ in 0..WARMUP_FRAMES {
            window.render_frame(cx);
        }
        for _ in 0..MEASURED_FRAMES {
            let start = Instant::now();
            window.render_frame(cx);
            navigator_only_frame_times.push(start.elapsed());
        }

        window.render_frame(cx);
        let menu_open_started = Instant::now();
        window.click("table-filter-menu", cx);
        schema_filter_menu_open_time = menu_open_started.elapsed();
        for _ in 0..WARMUP_FRAMES {
            window.render_frame(cx);
        }
        for _ in 0..MEASURED_FRAMES {
            let start = Instant::now();
            window.render_frame(cx);
            schema_filter_menu_frame_times.push(start.elapsed());
        }
    })
    .unwrap();
    assert_eq!(cached_frame_times.len(), MEASURED_FRAMES);
    assert_eq!(eager_tooltip_frame_times.len(), MEASURED_FRAMES);
    assert_eq!(filter_cache_miss_frame_times.len(), MEASURED_FRAMES);
    assert_eq!(results_only_frame_times.len(), MEASURED_FRAMES);
    assert_eq!(
        results_without_row_actions_frame_times.len(),
        MEASURED_FRAMES
    );
    assert_eq!(navigator_only_frame_times.len(), MEASURED_FRAMES);
    assert_eq!(schema_filter_menu_frame_times.len(), MEASURED_FRAMES);
    let summarize = |samples: &mut [std::time::Duration]| {
        samples.sort_unstable();
        let median = samples[MEASURED_FRAMES / 2].as_secs_f64() * 1000.0;
        let p95 = samples[MEASURED_FRAMES * 95 / 100].as_secs_f64() * 1000.0;
        (median, p95)
    };
    let (cached_median_ms, cached_p95_ms) = summarize(&mut cached_frame_times);
    let (eager_tooltip_median_ms, eager_tooltip_p95_ms) = summarize(&mut eager_tooltip_frame_times);
    let (cache_miss_median_ms, cache_miss_p95_ms) = summarize(&mut filter_cache_miss_frame_times);
    let (results_only_median_ms, results_only_p95_ms) = summarize(&mut results_only_frame_times);
    let (results_without_row_actions_median_ms, results_without_row_actions_p95_ms) =
        summarize(&mut results_without_row_actions_frame_times);
    let (navigator_only_median_ms, navigator_only_p95_ms) =
        summarize(&mut navigator_only_frame_times);
    let (schema_filter_menu_median_ms, schema_filter_menu_p95_ms) =
        summarize(&mut schema_filter_menu_frame_times);
    let (rendered_rows, rendered_cells) = render_counts;
    eprintln!(
        "GPUI headless filtered workspace: {TABLE_COUNT} relations, {RESULT_ROWS}x{RESULT_COLUMNS} result cells (25% with 1 KiB text); lazy tooltip median={cached_median_ms:.2} ms, p95={cached_p95_ms:.2} ms; eager tooltip median={eager_tooltip_median_ms:.2} ms, p95={eager_tooltip_p95_ms:.2} ms; filter-cache-miss fallback median={cache_miss_median_ms:.2} ms, p95={cache_miss_p95_ms:.2} ms"
    );
    eprintln!(
        "GPUI headless components: editable result grid median={results_only_median_ms:.2} ms, p95={results_only_p95_ms:.2} ms; result grid without per-row SQL actions median={results_without_row_actions_median_ms:.2} ms, p95={results_without_row_actions_p95_ms:.2} ms; object navigator only median={navigator_only_median_ms:.2} ms, p95={navigator_only_p95_ms:.2} ms; filtered workspace builds {:.1} rows and {:.1} cells per frame",
        rendered_rows as f64 / MEASURED_FRAMES as f64,
        rendered_cells as f64 / MEASURED_FRAMES as f64,
    );
    eprintln!(
        "GPUI headless schema filter ({} options): open action={:.2} ms; popup-open frame median={schema_filter_menu_median_ms:.2} ms, p95={schema_filter_menu_p95_ms:.2} ms",
        SCHEMA_COUNT,
        schema_filter_menu_open_time.as_secs_f64() * 1000.0,
    );
}

#[gpui_kit::test]
fn column_interaction_resizing_pinning_reordering_and_reset(cx: &mut TestAppContext) {
    cx.update(|cx| {
        gpui_kit::init(cx);
        Theme::change(ThemeMode::Dark, None, cx);
        Theme::sync_base(cx);
    });
    let connection_path = std::env::temp_dir().join(format!(
        "tablex-ui-column-test-{}-{}.json",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let window_handle = cx.add_window(move |window, cx| {
        DatabaseWorkspace::new_with_stores(
            window,
            cx,
            ConnectionStore::from_path(connection_path),
            CredentialStore::new(),
        )
    });
    let workspace = cx
        .read_window(&window_handle, |workspace, _| workspace)
        .unwrap();

    cx.update(|cx| {
        workspace.update(cx, |workspace, cx| {
            workspace.object_explorer = Some(ObjectExplorer::new(vec!["public".into()]));
            workspace.selected_table = Some(("public".into(), "people".into()));
            workspace.table_sidebar_visible = false;
            workspace.query_dock_tab = super::QueryDockTab::Results;
            let result = QueryResult {
                columns: vec![
                    "id".into(),
                    "name".into(),
                    "email".into(),
                    "bio".into(),
                ],
                column_types: vec![
                    "int4".into(),
                    "text".into(),
                    "varchar".into(),
                    "text".into(),
                ],
                column_enum_values: vec![None, None, None, None],
                rows: vec![
                    vec![
                        "1".into(),
                        "Alice".into(),
                        "alice@example.com".into(),
                        "Senior software engineer who likes distributed systems and Rust programming".into(),
                    ],
                    vec![
                        "2".into(),
                        "Bob".into(),
                        "bob@example.com".into(),
                        "Designer".into(),
                    ],
                ],
                null_cells: vec![vec![false; 4], vec![false; 4]],
                truncated_cells: vec![vec![false; 4], vec![false; 4]],
                offset: 0,
                limit: 25,
                has_next: false,
                truncated: false,
                editable: Some(EditableTable {
                    schema: "public".into(),
                    table: "people".into(),
                    primary_key_columns: vec!["id".into()],
                }),
            };
            workspace.query_result = Some(Arc::new(result));
            workspace.refresh_result_column_widths();
            cx.notify();
        });
    });

    // 1. Initial State: PK column is pinned by default
    cx.update(|cx| {
        let ws = workspace.read(cx);
        assert!(ws.is_column_pinned("public", "people", "id"));
        assert!(!ws.is_column_pinned("public", "people", "name"));
        assert_eq!(
            ws.pinned_columns_for("public", "people"),
            HashSet::from(["id".to_string()])
        );
        assert!(ws.custom_column_order_for("public", "people").is_none());
    });

    // 2. Resizing & Clamping [80.0, 500.0]
    cx.update(|cx| {
        workspace.update(cx, |ws, _| {
            // Under minimum: clamped to 80.0
            ws.set_column_width("public", "people", "name", 30.0);
            assert_eq!(ws.result_column_widths[1], 80.0);

            // Over maximum: clamped to 500.0
            ws.set_column_width("public", "people", "name", 850.0);
            assert_eq!(ws.result_column_widths[1], 500.0);

            // Normal valid width
            ws.set_column_width("public", "people", "name", 220.0);
            assert_eq!(ws.result_column_widths[1], 220.0);
        });
    });

    // 3. Auto-fit column width
    cx.update(|cx| {
        workspace.update(cx, |ws, _| {
            ws.autofit_column_width("public", "people", "bio");
            // Long bio string in rows should produce a width >= 200.0 and <= 500.0
            let bio_width = ws.result_column_widths[3];
            assert!(
                (200.0..=500.0).contains(&bio_width),
                "autofit width was {bio_width}"
            );
        });
    });

    // 4. Pinning / Unpinning
    cx.update(|cx| {
        workspace.update(cx, |ws, _| {
            // Pin name
            ws.toggle_pin_column("public", "people", "name");
            assert!(ws.is_column_pinned("public", "people", "name"));
            assert!(ws.is_column_pinned("public", "people", "id"));

            // Unpin id
            ws.toggle_pin_column("public", "people", "id");
            assert!(!ws.is_column_pinned("public", "people", "id"));
            assert!(ws.is_column_pinned("public", "people", "name"));
        });
    });

    // 5. Reordering (move_column & reorder_column)
    cx.update(|cx| {
        workspace.update(cx, |ws, _| {
            // Move email left (from idx 2 to idx 1)
            ws.move_column("public", "people", "email", MoveDirection::Left);
            assert_eq!(
                ws.custom_column_order_for("public", "people"),
                Some(
                    &[
                        "id".to_string(),
                        "email".to_string(),
                        "name".to_string(),
                        "bio".to_string()
                    ][..]
                )
            );

            // Move id left when already at start (no change, no panic)
            ws.move_column("public", "people", "id", MoveDirection::Left);
            assert_eq!(
                ws.custom_column_order_for("public", "people"),
                Some(
                    &[
                        "id".to_string(),
                        "email".to_string(),
                        "name".to_string(),
                        "bio".to_string()
                    ][..]
                )
            );

            // Move email right (from idx 1 back to idx 2)
            ws.move_column("public", "people", "email", MoveDirection::Right);
            assert_eq!(
                ws.custom_column_order_for("public", "people"),
                Some(
                    &[
                        "id".to_string(),
                        "name".to_string(),
                        "email".to_string(),
                        "bio".to_string()
                    ][..]
                )
            );

            // Reorder: drop bio before id (drag & drop reorder to target_index 0)
            ws.reorder_column("public", "people", "bio", 0);
            assert_eq!(
                ws.custom_column_order_for("public", "people"),
                Some(
                    &[
                        "bio".to_string(),
                        "id".to_string(),
                        "name".to_string(),
                        "email".to_string()
                    ][..]
                )
            );
        });
    });

    // 6. Reset Column Layout
    cx.update(|cx| {
        workspace.update(cx, |ws, _| {
            ws.reset_column_layout("public", "people");
            assert!(ws.custom_column_order_for("public", "people").is_none());
            // Pinned columns revert to default PK
            assert!(ws.is_column_pinned("public", "people", "id"));
            assert!(!ws.is_column_pinned("public", "people", "name"));
        });
    });

    // 7. Verify frame renders without issue
    cx.update_window(window_handle.into(), |_, window, cx| {
        window.render_frame(cx);
    })
    .unwrap();
}

#[gpui_kit::test]
fn thai_multiline_text_rendering_in_data_grid(cx: &mut TestAppContext) {
    cx.update(|cx| {
        gpui_kit::init(cx);
        Theme::change(ThemeMode::Dark, None, cx);
        Theme::sync_base(cx);
    });
    let connection_path = std::env::temp_dir().join(format!(
        "tablex-ui-thai-test-{}-{}.json",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let window_handle = cx.add_window(move |window, cx| {
        DatabaseWorkspace::new_with_stores(
            window,
            cx,
            ConnectionStore::from_path(connection_path),
            CredentialStore::new(),
        )
    });
    let workspace = cx
        .read_window(&window_handle, |workspace, _| workspace)
        .unwrap();

    cx.update(|cx| {
        workspace.update(cx, |workspace, cx| {
            workspace.object_explorer = Some(ObjectExplorer::new(vec!["public".into()]));
            workspace.selected_table = Some(("public".into(), "services".into()));
            workspace.table_sidebar_visible = false;
            workspace.query_dock_tab = super::QueryDockTab::Results;
            let result = QueryResult {
                columns: vec!["id".into(), "desc".into()],
                column_types: vec!["int4".into(), "varchar".into()],
                column_enum_values: vec![None, None],
                rows: vec![
                    vec![
                        "1".into(),
                        "บริการให้คำแนะนำด้านการดูแลและ\nการป้องกันกำจัดโรคพืช โรคแมลง และ ศัตรูพืช".into(),
                    ],
                    vec!["2".into(), "ผลิตภัณฑ์กำจัดศัตรูพืช".into()],
                    vec!["3".into(), "ปุ๋ยและสารบำรุงพืช".into()],
                    vec!["4".into(), "บริการโดรน\n..".into()],
                ],
                null_cells: vec![vec![false; 2]; 4],
                truncated_cells: vec![vec![false; 2]; 4],
                offset: 0,
                limit: 25,
                has_next: false,
                truncated: false,
                editable: Some(EditableTable {
                    schema: "public".into(),
                    table: "services".into(),
                    primary_key_columns: vec!["id".into()],
                }),
            };
            workspace.query_result = Some(Arc::new(result));
            workspace.refresh_result_column_widths();
            cx.notify();
        });
    });

    // Verify auto-fit handles multiline Thai text cleanly
    cx.update(|cx| {
        workspace.update(cx, |ws, _| {
            ws.autofit_column_width("public", "services", "desc");
            let desc_width = ws.result_column_widths[1];
            assert!((80.0..=500.0).contains(&desc_width));
        });
    });

    // Verify rendering of the frame with Thai multiline text
    cx.update_window(window_handle.into(), |_, window, cx| {
        window.render_frame(cx);
    })
    .unwrap();
}

#[gpui_kit::test]
fn datagrid_scroll_navigation_methods(cx: &mut gpui_kit::TestAppContext) {
    cx.update(|cx| {
        gpui_kit::init(cx);
        Theme::change(ThemeMode::Dark, None, cx);
        Theme::sync_base(cx);
    });
    let window_handle = cx.add_window(DatabaseWorkspace::new);
    let workspace = cx
        .read_window(&window_handle, |workspace, _| workspace)
        .unwrap();

    cx.update(|cx| {
        workspace.update(cx, |ws, _| {
            let result = QueryResult {
                columns: vec!["id".into(), "title".into()],
                column_types: vec!["int4".into(), "text".into()],
                column_enum_values: vec![None, None],
                rows: (0..100)
                    .map(|i| vec![i.to_string(), format!("Item {i}")])
                    .collect(),
                null_cells: vec![vec![false; 2]; 100],
                truncated_cells: vec![vec![false; 2]; 100],
                offset: 0,
                limit: 100,
                has_next: false,
                truncated: false,
                editable: None,
            };
            ws.query_result = Some(Arc::new(result));

            // Test horizontal scroll helpers
            ws.scroll_to_first_column();
            assert_eq!(
                ws.table_result_horizontal_scroll.offset().x,
                gpui_kit::px(0.)
            );

            ws.scroll_horizontal_by(-100.0);
            assert_eq!(
                ws.table_result_horizontal_scroll.offset().x,
                gpui_kit::px(-100.0)
            );

            ws.scroll_horizontal_by(50.0);
            assert_eq!(
                ws.table_result_horizontal_scroll.offset().x,
                gpui_kit::px(-50.0)
            );

            ws.scroll_to_first_column();
            assert_eq!(
                ws.table_result_horizontal_scroll.offset().x,
                gpui_kit::px(0.)
            );

            // Test vertical navigation methods
            ws.scroll_to_top();
            ws.scroll_page_down();
            ws.scroll_page_up();
            ws.scroll_to_bottom();
            ws.scroll_to_top();
        });
    });
}

#[gpui_kit::test]
fn datagrid_2d_scroll_and_zero_jitter_pinned_columns(cx: &mut gpui_kit::TestAppContext) {
    cx.update(|cx| {
        gpui_kit::init(cx);
        Theme::change(ThemeMode::Dark, None, cx);
        Theme::sync_base(cx);
    });
    let window_handle = cx.add_window(DatabaseWorkspace::new);
    let workspace = cx
        .read_window(&window_handle, |workspace, _| workspace)
        .unwrap();

    cx.update(|cx| {
        workspace.update(cx, |ws, _| {
            let result = QueryResult {
                columns: vec!["id".into(), "name".into(), "email".into(), "city".into()],
                column_types: vec!["int4".into(), "text".into(), "text".into(), "text".into()],
                column_enum_values: vec![None, None, None, None],
                rows: (0..20)
                    .map(|i| {
                        vec![
                            i.to_string(),
                            format!("Name {i}"),
                            format!("user{i}@test.com"),
                            format!("City {i}"),
                        ]
                    })
                    .collect(),
                null_cells: vec![vec![false; 4]; 20],
                truncated_cells: vec![vec![false; 4]; 20],
                offset: 0,
                limit: 25,
                has_next: false,
                truncated: false,
                editable: Some(EditableTable {
                    schema: "public".into(),
                    table: "users".into(),
                    primary_key_columns: vec!["id".into()],
                }),
            };
            ws.query_result = Some(Arc::new(result));
            ws.selected_table = Some(("public".into(), "users".into()));
            ws.refresh_result_column_widths();

            // Simulate scrolled state (scroll_offset_x > 0)
            ws.scroll_horizontal_by(-120.0);
            assert_eq!(
                ws.table_result_horizontal_scroll.offset().x,
                gpui_kit::px(-120.0)
            );
        });
    });

    // Verify window frame renders cleanly with horizontal offset and pinned columns
    cx.update_window(window_handle.into(), |_, window, cx| {
        window.render_frame(cx);
    })
    .unwrap();
}
