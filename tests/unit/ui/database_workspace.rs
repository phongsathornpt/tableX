use std::collections::HashSet;
use std::rc::Rc;
use std::sync::Arc;
use std::time::{Instant, SystemTime, UNIX_EPOCH};

use gpui_kit::component::{Theme, ThemeMode};
use gpui_kit::{AppContext as _, TestAppContext, point, px, test::TestWindowExt};

use super::connection_editor::missing_required_fields_message;
use super::{DatabaseWorkspace, ObjectExplorer};
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
            workspace.query_result = Some(Arc::new(QueryResult {
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
            }));
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
