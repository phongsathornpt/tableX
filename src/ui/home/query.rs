use gpui_kit::base::{h_flex, v_flex};
use gpui_kit::component::{
    ActiveTheme as _, Disableable as _, Icon, IconName, Sizable as _,
    button::{Button, ButtonVariants as _},
    input::{Input, InputState, Textarea, TextareaState},
    menu::{DropdownMenu as _, PopupMenuItem},
    popover::Popover,
    scroll::ScrollableElement as _,
};
use gpui_kit::{
    Context, Entity, FontWeight, InteractiveElement as _, IntoElement, ParentElement as _,
    StatefulInteractiveElement as _, Styled as _, div, px, uniform_list,
};
use std::rc::Rc;
#[cfg(test)]
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

use crate::domain::query::{TableColumnFilter, TableFilterOperator};
use crate::infrastructure::QueryResult;
use crate::infrastructure::postgres::sql::{build_delete_sql, build_update_sql, quote_identifier};
use crate::ui::DatabaseWorkspace;
use crate::ui::database_workspace::{MAX_ENUM_MENU_OPTIONS, QueryDockTab};
use gpui_kit::base::Selectable as _;

const TABLE_CELL_PREVIEW_CHARS: usize = 160;

fn table_filter_operators(column_type: &str) -> Vec<(TableFilterOperator, &'static str)> {
    let mut operators = vec![
        (TableFilterOperator::Equals, "Equals"),
        (TableFilterOperator::NotEquals, "Does not equal"),
    ];
    if matches!(
        column_type.to_ascii_lowercase().as_str(),
        "text" | "varchar" | "bpchar" | "name" | "citext"
    ) {
        operators.extend([
            (TableFilterOperator::Contains, "Contains"),
            (TableFilterOperator::StartsWith, "Starts with"),
        ]);
    } else if matches!(
        column_type.to_ascii_lowercase().as_str(),
        "int2"
            | "int4"
            | "int8"
            | "float4"
            | "float8"
            | "numeric"
            | "date"
            | "time"
            | "timetz"
            | "timestamp"
            | "timestamptz"
    ) {
        operators.extend([
            (TableFilterOperator::GreaterThan, "Greater than"),
            (TableFilterOperator::LessThan, "Less than"),
        ]);
    }
    operators.extend([
        (TableFilterOperator::IsNull, "Is NULL"),
        (TableFilterOperator::IsNotNull, "Is not NULL"),
    ]);
    operators
}

fn table_filter_operator_label(operator: TableFilterOperator) -> &'static str {
    match operator {
        TableFilterOperator::Equals => "Equals",
        TableFilterOperator::NotEquals => "Does not equal",
        TableFilterOperator::Contains => "Contains",
        TableFilterOperator::StartsWith => "Starts with",
        TableFilterOperator::GreaterThan => "Greater than",
        TableFilterOperator::LessThan => "Less than",
        TableFilterOperator::IsNull => "Is NULL",
        TableFilterOperator::IsNotNull => "Is not NULL",
    }
}

fn table_filter_chip_label(filter: &TableColumnFilter) -> String {
    let value = filter.value.as_deref().unwrap_or_default();
    format!(
        "{} {}{} ×",
        filter.column,
        table_filter_operator_label(filter.operator),
        if value.is_empty() {
            String::new()
        } else {
            format!(" {value}")
        }
    )
}

#[cfg(test)]
static EAGER_CELL_TOOLTIPS_FOR_BENCHMARK: AtomicBool = AtomicBool::new(false);
#[cfg(test)]
static ROW_SQL_ACTIONS_FOR_BENCHMARK: AtomicBool = AtomicBool::new(true);
#[cfg(test)]
static RESULT_ROW_RENDER_COUNT: AtomicUsize = AtomicUsize::new(0);
#[cfg(test)]
static RESULT_CELL_RENDER_COUNT: AtomicUsize = AtomicUsize::new(0);

#[cfg(test)]
pub(crate) fn set_eager_cell_tooltips_for_benchmark(enabled: bool) {
    EAGER_CELL_TOOLTIPS_FOR_BENCHMARK.store(enabled, Ordering::Relaxed);
}

#[cfg(test)]
pub(crate) fn set_row_sql_actions_for_benchmark(enabled: bool) {
    ROW_SQL_ACTIONS_FOR_BENCHMARK.store(enabled, Ordering::Relaxed);
}

#[cfg(test)]
pub(crate) fn reset_render_counts_for_benchmark() {
    RESULT_ROW_RENDER_COUNT.store(0, Ordering::Relaxed);
    RESULT_CELL_RENDER_COUNT.store(0, Ordering::Relaxed);
}

#[cfg(test)]
pub(crate) fn render_counts_for_benchmark() -> (usize, usize) {
    (
        RESULT_ROW_RENDER_COUNT.load(Ordering::Relaxed),
        RESULT_CELL_RENDER_COUNT.load(Ordering::Relaxed),
    )
}

pub(crate) fn render_panel(
    cx: &mut Context<DatabaseWorkspace>,
    query_input: &Entity<TextareaState>,
    query_running: bool,
    write_confirmation_pending: bool,
) -> impl IntoElement {
    v_flex()
        .w_full()
        .gap_2()
        .p_3()
        .rounded_md()
        .border_1()
        .border_color(cx.theme().border)
        .bg(cx.theme().secondary)
        .child(
            h_flex()
                .items_center()
                .justify_between()
                .child(
                    h_flex()
                        .items_center()
                        .gap_2()
                        .child(IconName::FileText)
                        .child(div().font_weight(FontWeight::SEMIBOLD).child("SQL editor"))
                        .child(
                            div()
                                .text_xs()
                                .text_color(cx.theme().muted_foreground)
                                .child("Read query"),
                        ),
                )
                .child(
                    h_flex()
                        .gap_1()
                        .child(
                            Button::new("format-query")
                                .ghost()
                                .xsmall()
                                .label("Format")
                                .on_click(
                                    cx.listener(|this, _, window, cx| {
                                        this.format_query(window, cx)
                                    }),
                                ),
                        )
                        .child(
                            Button::new("run-query")
                                .primary()
                                .small()
                                .icon(IconName::Play)
                                .label(if query_running { "Running..." } else { "Run" })
                                .disabled(query_running)
                                .on_click(cx.listener(|this, _, _, cx| this.execute_query(cx))),
                        ),
                ),
        )
        .child(
            h_flex()
                .justify_between()
                .items_center()
                .child(
                    div()
                        .text_xs()
                        .text_color(cx.theme().muted_foreground)
                        .child("Run executes one read query · up to 500 rows"),
                )
                .child(
                    Button::new("run-write-query")
                        .danger()
                        .small()
                        .label(if write_confirmation_pending {
                            "Confirm write"
                        } else {
                            "Run write"
                        })
                        .disabled(query_running)
                        .on_click(cx.listener(|this, _, _, cx| this.execute_write_query(cx))),
                ),
        )
        .child(Textarea::new(query_input).h(px(112.)).w_full())
}

pub(crate) fn render_workspace_dock(
    cx: &mut Context<DatabaseWorkspace>,
    query_input: &Entity<TextareaState>,
    query_result: Option<&QueryResult>,
    query_running: bool,
    write_confirmation_pending: bool,
    active_tab: QueryDockTab,
) -> impl IntoElement {
    let query_selected = active_tab == QueryDockTab::Query;
    let results_selected = active_tab == QueryDockTab::Results;
    let query_workspace = cx.entity();
    let format_workspace = query_workspace.clone();
    let run_workspace = query_workspace.clone();
    let write_workspace = query_workspace.clone();
    v_flex()
        .size_full()
        .min_h(px(0.))
        .bg(cx.theme().background)
        .child(
            h_flex()
                .h(px(42.))
                .items_center()
                .justify_between()
                .px_3()
                .border_b_1()
                .border_color(cx.theme().border)
                .bg(cx.theme().secondary)
                .child(
                    h_flex()
                        .h_full()
                        .items_center()
                        .gap_1()
                        .child(
                            Button::new("query-dock-tab")
                                .ghost()
                                .small()
                                .icon(IconName::FileText)
                                .label("Query")
                                .selected(query_selected)
                                .on_click(cx.listener(|this, _, window, cx| {
                                    this.set_query_dock_tab(QueryDockTab::Query, window, cx)
                                })),
                        )
                        .child(
                            Button::new("results-dock-tab")
                                .ghost()
                                .small()
                                .icon(Icon::new(gpui_kit::assets::IconName::Table))
                                .label("Results")
                                .selected(results_selected)
                                .on_click(cx.listener(|this, _, window, cx| {
                                    this.set_query_dock_tab(QueryDockTab::Results, window, cx)
                                })),
                        ),
                )
                .child(
                    h_flex()
                        .items_center()
                        .gap_2()
                        .child(
                            div()
                                .text_xs()
                                .text_color(cx.theme().muted_foreground)
                                .child("PostgreSQL"),
                        )
                        .child(
                            h_flex()
                                .items_center()
                                .gap_1()
                                .px_2()
                                .py_1()
                                .rounded_sm()
                                .bg(cx.theme().accent)
                                .child(Icon::new(gpui_kit::assets::IconName::LockKeyhole))
                                .child(
                                    div()
                                        .text_xs()
                                        .text_color(cx.theme().accent_foreground)
                                        .child("Read only"),
                                ),
                        )
                        .child(
                            Button::new("format-query-dock")
                                .ghost()
                                .xsmall()
                                .label("Format")
                                .on_click(move |_, window, cx| {
                                    format_workspace
                                        .update(cx, |this, cx| this.format_query(window, cx));
                                }),
                        )
                        .child(
                            Button::new("run-query-dock")
                                .primary()
                                .small()
                                .icon(IconName::Play)
                                .label(if query_running { "Running…" } else { "Run" })
                                .disabled(query_running)
                                .on_click(move |_, _, cx| {
                                    run_workspace.update(cx, |this, cx| this.execute_query(cx));
                                }),
                        )
                        .child(
                            Button::new("run-write-query-dock")
                                .danger()
                                .xsmall()
                                .label(if write_confirmation_pending {
                                    "Confirm write"
                                } else {
                                    "Run write"
                                })
                                .disabled(query_running)
                                .on_click(move |_, _, cx| {
                                    write_workspace
                                        .update(cx, |this, cx| this.execute_write_query(cx));
                                }),
                        ),
                ),
        )
        .child(if query_selected {
            v_flex()
                .flex_1()
                .min_h(px(0.))
                .px_3()
                .py_2()
                .gap_1()
                .child(Textarea::new(query_input).flex_1().w_full())
                .child(
                    h_flex()
                        .h(px(20.))
                        .items_center()
                        .gap_2()
                        .text_xs()
                        .text_color(cx.theme().muted_foreground)
                        .child(IconName::Info)
                        .child("One read-only statement · result limit 500 rows"),
                )
                .into_any_element()
        } else {
            v_flex()
                .flex_1()
                .min_h(px(0.))
                .justify_center()
                .items_center()
                .gap_2()
                .child(Icon::new(gpui_kit::assets::IconName::Table))
                .child(div().text_sm().font_weight(FontWeight::SEMIBOLD).child(
                    if query_result.is_some() {
                        "Query results are shown in the data canvas"
                    } else {
                        "No query results yet"
                    },
                ))
                .child(
                    div()
                        .text_xs()
                        .text_color(cx.theme().muted_foreground)
                        .child(query_result.map_or_else(
                            || "Run a query to populate the table view.".to_owned(),
                            |result| {
                                format!(
                                    "{} rows · {} columns",
                                    result.rows.len(),
                                    result.columns.len()
                                )
                            },
                        )),
                )
                .into_any_element()
        })
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn render_result(
    cx: &mut Context<DatabaseWorkspace>,
    result: Option<&QueryResult>,
    table_data_offset: usize,
    table_data_limit: usize,
    table_data_has_next: bool,
    query_running: bool,
    table_data_sort: Option<&(String, bool)>,
    table_data_filter_input: Option<&Entity<InputState>>,
    table_data_filter: Option<&str>,
    filter_column: Option<String>,
    empty_filter: Option<(String, bool)>,
    cached_filtered_row_indices: Option<Rc<Vec<usize>>>,
    filter_pending: bool,
    hidden_columns: std::collections::HashSet<String>,
    result_scroll: Option<&gpui_kit::UniformListScrollHandle>,
    horizontal_scroll: Option<&gpui_kit::ScrollHandle>,
    workspace_layout: bool,
    table_column_filters: &[TableColumnFilter],
    active_cell_edit: Option<crate::ui::database_workspace::ActiveCellEditView>,
    table_filter_editor: Option<(Option<String>, TableFilterOperator, Entity<InputState>)>,
) -> impl IntoElement {
    let Some(result) = result else {
        let mut empty_state = v_flex()
            .w_full()
            .items_center()
            .justify_center()
            .gap_2()
            .child(Icon::new(gpui_kit::assets::IconName::Table))
            .child(
                div()
                    .font_weight(FontWeight::SEMIBOLD)
                    .child("Select a table or run a query"),
            )
            .child(
                div()
                    .text_sm()
                    .text_color(cx.theme().muted_foreground)
                    .child("Choose an object from the navigator to inspect its rows."),
            );
        if workspace_layout {
            empty_state = empty_state.flex_1().min_h(px(0.));
        } else {
            empty_state = empty_state
                .p_4()
                .rounded_md()
                .border_1()
                .border_color(cx.theme().border);
        }
        return empty_state.into_any_element();
    };

    let workspace = cx.entity();
    #[cfg(test)]
    let eager_cell_tooltips = EAGER_CELL_TOOLTIPS_FOR_BENCHMARK.load(Ordering::Relaxed);
    #[cfg(not(test))]
    let eager_cell_tooltips = false;
    let column_widths = result
        .columns
        .iter()
        .enumerate()
        .map(|(index, column)| {
            table_column_width(
                column,
                result
                    .column_types
                    .get(index)
                    .map(String::as_str)
                    .unwrap_or_default(),
            )
        })
        .collect::<Vec<_>>();
    let (visible_column_indices, leading_column_width, trailing_column_width) =
        visible_column_window(
            &result.columns,
            &column_widths,
            &hidden_columns,
            workspace_layout,
            horizontal_scroll,
        );
    let mut header = h_flex()
        .w_full()
        .gap_0()
        .px_2()
        .h(px(56.))
        .border_b_1()
        .border_color(cx.theme().border)
        .bg(cx.theme().table_head);
    if workspace_layout {
        header = header.child(
            div()
                .w(px(40.))
                .text_xs()
                .text_color(cx.theme().muted_foreground)
                .child("#"),
        );
    }
    if leading_column_width > 0. {
        header = header.child(div().w(px(leading_column_width)));
    }
    header = header.children(visible_column_indices.iter().filter_map(|index| {
        let index = *index;
        let column = result.columns.get(index)?;
        let column_type = result
            .column_types
            .get(index)
            .cloned()
            .unwrap_or_else(|| "unknown".to_owned());
        let is_sorted = table_data_sort
            .map(|(sorted_column, _)| sorted_column == column)
            .unwrap_or(false);
        let sort_label = if is_sorted {
            if table_data_sort.is_some_and(|(_, descending)| *descending) {
                format!("{column_type} ↓")
            } else {
                format!("{column_type} ↑")
            }
        } else {
            column_type.to_owned()
        };
        let column_width = column_widths[index];
        let column_name = column.clone();
        let sort_workspace = workspace.clone();
        let filter_workspace = workspace.clone();
        let filter_column_name = column.clone();
        let filter_is_active = table_column_filters
            .iter()
            .any(|filter| filter.column == *column);
        let enum_values = result
            .column_enum_values
            .get(index)
            .and_then(Option::as_ref)
            .filter(|values| values.len() <= MAX_ENUM_MENU_OPTIONS)
            .cloned()
            .unwrap_or_default();
        let operators = table_filter_operators(&column_type);
        let filter_button = Button::new(format!("filter-column-{index}"))
            .ghost()
            .xsmall()
            .icon(Icon::new(gpui_kit::assets::IconName::ListFilter))
            .selected(filter_is_active)
            .tooltip(if filter_is_active {
                format!("Edit filter for {column}")
            } else {
                format!("Filter {column}")
            });
        let filter_control = if result.editable.is_some() {
            let content_filter_editor = table_filter_editor.clone();
            Popover::new(format!("column-filter-popover-{index}"))
                .trigger(filter_button)
                .on_open_change({
                    let workspace = filter_workspace.clone();
                    let column = filter_column_name.clone();
                    move |open, window, cx| {
                        if *open {
                            workspace.update(cx, |this, cx| {
                                this.open_table_filter_editor(column.clone(), window, cx)
                            });
                        }
                    }
                })
                .content(move |_, _window, _cx| {
                    let Some((editing_column, selected_operator, input)) =
                        content_filter_editor.clone()
                    else {
                        return div().into_any_element();
                    };
                    if editing_column.as_deref() != Some(filter_column_name.as_str()) {
                        return div().into_any_element();
                    }
                    let operator_label = table_filter_operator_label(selected_operator);
                    let menu_workspace = filter_workspace.clone();
                    let menu_operators = operators.clone();
                    let operator_menu = Button::new(format!("filter-operator-{index}"))
                        .outline()
                        .small()
                        .dropdown_caret(true)
                        .label(operator_label)
                        .dropdown_menu(move |menu, _, _| {
                            menu_operators.iter().fold(menu, |menu, (operator, label)| {
                                let workspace = menu_workspace.clone();
                                let operator_value = *operator;
                                menu.item(
                                    PopupMenuItem::new(*label)
                                        .checked(operator_value == selected_operator)
                                        .on_click(move |_, _, cx| {
                                            workspace.update(cx, |this, cx| {
                                                this.set_table_filter_editor_operator(
                                                    operator_value,
                                                    cx,
                                                )
                                            });
                                        }),
                                )
                            })
                        });
                    let is_null_operator = matches!(
                        selected_operator,
                        TableFilterOperator::IsNull | TableFilterOperator::IsNotNull
                    );
                    let apply_workspace = filter_workspace.clone();
                    let clear_workspace = filter_workspace.clone();
                    let clear_column = filter_column_name.clone();
                    let value_control = if enum_values.is_empty() {
                        Input::new(&input).w_full().into_any_element()
                    } else {
                        let values_for_menu = enum_values.clone();
                        let workspace_for_value = filter_workspace.clone();
                        h_flex()
                            .gap_1()
                            .child(Input::new(&input).flex_1())
                            .child(
                                Button::new(format!("enum-filter-values-{index}"))
                                    .outline()
                                    .small()
                                    .dropdown_caret(true)
                                    .label("Values")
                                    .dropdown_menu(move |menu, _, _| {
                                        values_for_menu.iter().fold(menu, |menu, value| {
                                            let workspace = workspace_for_value.clone();
                                            let value = value.clone();
                                            menu.item(PopupMenuItem::new(value.clone()).on_click(
                                                move |_, window, cx| {
                                                    workspace.update(cx, |this, cx| {
                                                        this.set_table_filter_input(
                                                            value.clone(),
                                                            window,
                                                            cx,
                                                        )
                                                    });
                                                },
                                            ))
                                        })
                                    }),
                            )
                            .into_any_element()
                    };
                    v_flex()
                        .w(px(272.))
                        .gap_2()
                        .child(
                            div()
                                .text_sm()
                                .font_weight(FontWeight::SEMIBOLD)
                                .child(format!("Filter {filter_column_name}")),
                        )
                        .child(operator_menu)
                        .child(if is_null_operator {
                            div().into_any_element()
                        } else {
                            value_control
                        })
                        .child(
                            h_flex()
                                .justify_between()
                                .child(
                                    Button::new(format!("clear-column-filter-{index}"))
                                        .ghost()
                                        .small()
                                        .label("Clear")
                                        .on_click(move |_, window, cx| {
                                            clear_workspace.update(cx, |this, cx| {
                                                this.clear_table_column_filter(
                                                    &clear_column,
                                                    window,
                                                    cx,
                                                )
                                            });
                                        }),
                                )
                                .child(
                                    Button::new(format!("apply-column-filter-{index}"))
                                        .primary()
                                        .small()
                                        .label("Apply")
                                        .on_click(move |_, window, cx| {
                                            apply_workspace.update(cx, |this, cx| {
                                                this.apply_table_column_filter(window, cx)
                                            });
                                        }),
                                ),
                        )
                        .into_any_element()
                })
                .into_any_element()
        } else {
            div().w(px(30.)).into_any_element()
        };
        Some(
            h_flex()
                .w(px(column_width))
                .h(px(48.))
                .items_center()
                .gap_0()
                .child(
                    Button::new(format!("sort-column-{index}"))
                        .ghost()
                        .flex_1()
                        .small()
                        .h(px(48.))
                        .justify_start()
                        .label(format!("{column_name}\n{sort_label}"))
                        .tooltip(format!("Sort by {column_name} ({column_type})"))
                        .on_click(move |_, window, cx| {
                            sort_workspace.update(cx, |this, cx| {
                                this.set_table_data_sort(column_name.clone(), window, cx)
                            })
                        }),
                )
                .child(filter_control),
        )
    }));
    if trailing_column_width > 0. {
        header = header.child(div().w(px(trailing_column_width)));
    }
    if result.editable.is_some() && !workspace_layout {
        header = header.child(
            div()
                .w(px(112.))
                .text_xs()
                .text_color(cx.theme().muted_foreground)
                .child("Row SQL"),
        );
    }
    let max_visible_rows = if workspace_layout { usize::MAX } else { 100 };
    let filters_active = table_data_filter.is_some() || empty_filter.is_some();
    let filter_pending =
        filter_pending || (filters_active && cached_filtered_row_indices.is_none());
    let filtered_row_indices = filters_active
        .then_some(cached_filtered_row_indices)
        .flatten();
    let visible_row_count = if filter_pending {
        0
    } else {
        filtered_row_indices
            .as_ref()
            .map_or(result.rows.len().min(max_visible_rows), |indices| {
                indices.len()
            })
    };
    let rows = if filter_pending {
        v_flex()
            .w_full()
            .items_center()
            .justify_center()
            .py_8()
            .child(IconName::Search)
            .child(div().text_sm().child("Filtering loaded rows…"))
            .into_any_element()
    } else if visible_row_count == 0 {
        v_flex()
            .w_full()
            .items_center()
            .gap_2()
            .py_8()
            .child(IconName::Search)
            .child(div().text_sm().child(
                if table_data_filter.is_some() || empty_filter.is_some() {
                    "No loaded rows match this filter."
                } else {
                    "This table has no rows on the loaded page."
                },
            ))
            .child(
                div()
                    .text_xs()
                    .text_color(cx.theme().muted_foreground)
                    .child(if table_data_filter.is_some() || empty_filter.is_some() {
                        "Clear the filter or load another page."
                    } else {
                        "Try another table or run a different query."
                    }),
            )
            .into_any_element()
    } else {
        if workspace_layout {
            let row_indices = filtered_row_indices;
            let column_widths = Rc::new(column_widths.clone());
            let visible_column_indices = Rc::new(visible_column_indices);
            let workspace = workspace.clone();
            let active_cell_edit = active_cell_edit.clone();
            let scroll = result_scroll.expect("workspace results require a uniform scroll handle");
            uniform_list(
                "query-result-rows",
                visible_row_count,
                cx.processor(move |this, range: std::ops::Range<usize>, _window, cx| {
                    let Some(result) = this.query_result() else {
                        return Vec::new();
                    };
                    range
                        .filter_map(|visible_index| {
                            let source_index = row_indices
                                .as_ref()
                                .map_or(Some(visible_index), |indices| {
                                    indices.get(visible_index).copied()
                                })?;
                            let row = result.rows.get(source_index)?;
                            Some(render_result_row(
                                result,
                                row,
                                visible_index,
                                source_index,
                                eager_cell_tooltips,
                                table_data_offset,
                                true,
                                &column_widths,
                                &visible_column_indices,
                                leading_column_width,
                                trailing_column_width,
                                &workspace,
                                active_cell_edit.as_ref(),
                                query_running,
                                cx,
                            ))
                        })
                        .collect::<Vec<_>>()
                }),
            )
            .size_full()
            .track_scroll(scroll)
            .into_any_element()
        } else {
            (0..visible_row_count)
                .filter_map(|visible_index| {
                    let source_index = filtered_row_indices
                        .as_ref()
                        .map_or(Some(visible_index), |indices| {
                            indices.get(visible_index).copied()
                        })?;
                    let row = result.rows.get(source_index)?;
                    Some(render_result_row(
                        result,
                        row,
                        visible_index,
                        source_index,
                        eager_cell_tooltips,
                        table_data_offset,
                        false,
                        &column_widths,
                        &visible_column_indices,
                        leading_column_width,
                        trailing_column_width,
                        &workspace,
                        active_cell_edit.as_ref(),
                        query_running,
                        cx,
                    ))
                })
                .fold(v_flex().w_full(), |rows, row| rows.child(row))
                .into_any_element()
        }
    };
    let footer = if result
        .editable
        .as_ref()
        .is_some_and(|table| table.primary_key_columns.is_empty())
    {
        "This table has no primary key, so row update SQL is unavailable."
    } else if result.truncated || result.rows.len() > 100 {
        "Showing the first 100 rows; the query result is capped at 500 rows."
    } else {
        ""
    };

    let mut result_heading = h_flex()
        .justify_between()
        .child(
            h_flex()
                .items_center()
                .gap_2()
                .child(IconName::FileText)
                .child(
                    v_flex()
                        .gap_1()
                        .child(div().font_weight(FontWeight::SEMIBOLD).child(
                            if result.editable.is_some() {
                                "Rows"
                            } else {
                                "Query results"
                            },
                        ))
                        .child(
                            div()
                                .text_xs()
                                .text_color(cx.theme().muted_foreground)
                                .child(format!(
                                    "{} columns · scroll horizontally for more · read-only preview",
                                    result.columns.len()
                                )),
                        ),
                ),
        )
        .child(
            div()
                .text_sm()
                .text_color(cx.theme().muted_foreground)
                .child(if filter_pending {
                    "Filtering loaded rows…".to_owned()
                } else if table_data_filter.is_some() {
                    format!("{visible_row_count} visible · {} loaded", result.rows.len())
                } else {
                    format!("{} loaded", result.rows.len())
                }),
        );
    if let Some(table) = result.editable.as_ref() {
        let workspace = workspace.clone();
        let insert_sql = format!(
            "INSERT INTO {}.{} DEFAULT VALUES;",
            quote_identifier(&table.schema),
            quote_identifier(&table.table)
        );
        result_heading = result_heading.child(
            Button::new("insert-table-row")
                .outline()
                .xsmall()
                .label("Insert row SQL")
                .on_click(move |_, window, cx| {
                    workspace.update(cx, |this, cx| {
                        this.prepare_query(insert_sql.clone(), window, cx);
                    });
                }),
        );
    }

    let filter_bar = if result.editable.is_some() {
        if let Some(filter_input) = table_data_filter_input {
            let apply_workspace = workspace.clone();
            h_flex()
                .items_center()
                .gap_2()
                .w(px(340.))
                .flex_shrink_0()
                .child(Input::new(filter_input).flex_1())
                .child(
                    Button::new("apply-row-filter")
                        .outline()
                        .small()
                        .label("Filter rows")
                        .on_click(move |_, _, cx| {
                            apply_workspace.update(cx, |this, cx| this.apply_table_data_filter(cx))
                        }),
                )
                .child(if workspace_layout {
                    div().into_any_element()
                } else {
                    div()
                        .text_xs()
                        .text_color(cx.theme().muted_foreground)
                        .child(table_data_filter.map_or_else(
                            || "Searches loaded rows".to_owned(),
                            |filter| format!("Filter: {filter}"),
                        ))
                        .into_any_element()
                })
                .child(if table_data_filter.is_some() {
                    let clear_workspace = workspace.clone();
                    Button::new("clear-row-filter")
                        .ghost()
                        .xsmall()
                        .label("Clear")
                        .on_click(move |_, window, cx| {
                            clear_workspace.update(cx, |this, cx| {
                                this.clear_table_data_filter(window, cx);
                            })
                        })
                        .into_any_element()
                } else {
                    div().into_any_element()
                })
                .into_any_element()
        } else {
            div().into_any_element()
        }
    } else {
        div().into_any_element()
    };

    let workspace_toolbar = if workspace_layout {
        let refresh_workspace = workspace.clone();
        let sort_workspace = workspace.clone();
        let size_workspace = workspace.clone();
        let filter_scope_workspace = workspace.clone();
        let empty_filter_workspace = workspace.clone();
        let result_columns = Rc::new(result.columns.clone());
        let filter_columns = result_columns.clone();
        let columns_workspace = workspace.clone();
        let columns_to_toggle = result_columns.clone();
        let hidden_columns_for_menu = hidden_columns.clone();
        let visible_column_count = result
            .columns
            .iter()
            .filter(|column| !hidden_columns.contains(*column))
            .count();
        let sort_columns = result_columns.clone();
        let filter_columns_for_menu = result_columns;
        h_flex()
            .items_center()
            .gap_2()
            .flex_1()
            .child(
                Button::new("filter-column-scope")
                    .outline()
                    .small()
                    .dropdown_caret(true)
                    .label(filter_column.as_deref().unwrap_or("All columns"))
                    .dropdown_menu(move |menu, _, _| {
                        let workspace = filter_scope_workspace.clone();
                        let menu = menu.item(
                            PopupMenuItem::new("All columns")
                                .checked(filter_column.is_none())
                                .on_click(move |_, _, cx| {
                                    workspace.update(cx, |this, cx| {
                                        this.set_table_data_filter_column(None, cx)
                                    });
                                }),
                        );
                        filter_columns.iter().fold(menu, |menu, column| {
                            let workspace = filter_scope_workspace.clone();
                            let column_name = column.clone();
                            menu.item(
                                PopupMenuItem::new(column.clone())
                                    .checked(filter_column.as_ref() == Some(column))
                                    .on_click(move |_, _, cx| {
                                        workspace.update(cx, |this, cx| {
                                            this.set_table_data_filter_column(
                                                Some(column_name.clone()),
                                                cx,
                                            )
                                        });
                                    }),
                            )
                        })
                    }),
            )
            .child(
                Button::new("add-table-filter")
                    .outline()
                    .small()
                    .dropdown_caret(true)
                    .label(empty_filter.as_ref().map_or_else(
                        || "Add filter".to_owned(),
                        |(column, is_empty)| {
                            format!(
                                "{column} {}",
                                if *is_empty { "is empty" } else { "has value" }
                            )
                        },
                    ))
                    .dropdown_menu(move |menu, _, _| {
                        let workspace = empty_filter_workspace.clone();
                        let menu = menu.item(PopupMenuItem::new("Clear value filter").on_click(
                            move |_, _, cx| {
                                workspace.update(cx, |this, cx| {
                                    this.set_table_data_empty_filter(None, cx)
                                });
                            },
                        ));
                        filter_columns_for_menu.iter().fold(menu, |menu, column| {
                            let workspace_empty = empty_filter_workspace.clone();
                            let empty_column = column.clone();
                            let workspace_value = empty_filter_workspace.clone();
                            let value_column = column.clone();
                            menu.item(PopupMenuItem::new(format!("{column} is empty")).on_click(
                                move |_, _, cx| {
                                    workspace_empty.update(cx, |this, cx| {
                                        this.set_table_data_empty_filter(
                                            Some((empty_column.clone(), true)),
                                            cx,
                                        );
                                    });
                                },
                            ))
                            .item(
                                PopupMenuItem::new(format!("{column} has a value")).on_click(
                                    move |_, _, cx| {
                                        workspace_value.update(cx, |this, cx| {
                                            this.set_table_data_empty_filter(
                                                Some((value_column.clone(), false)),
                                                cx,
                                            );
                                        });
                                    },
                                ),
                            )
                        })
                    }),
            )
            .child(
                Button::new("sort-table-rows")
                    .outline()
                    .small()
                    .dropdown_caret(true)
                    .label(table_data_sort.map_or_else(
                        || "Sort".to_owned(),
                        |(column, descending)| {
                            format!("{column} {}", if *descending { "↓" } else { "↑" })
                        },
                    ))
                    .dropdown_menu(move |menu, _, _| {
                        sort_columns.iter().fold(menu, |menu, column| {
                            let workspace = sort_workspace.clone();
                            let column_name = column.clone();
                            menu.item(PopupMenuItem::new(column.clone()).on_click(
                                move |_, window, cx| {
                                    workspace.update(cx, |this, cx| {
                                        this.set_table_data_sort(column_name.clone(), window, cx)
                                    });
                                },
                            ))
                        })
                    }),
            )
            .child(div().flex_1())
            .child(
                Button::new("table-page-size")
                    .outline()
                    .small()
                    .dropdown_caret(true)
                    .label(format!("Rows {table_data_limit}"))
                    .dropdown_menu(move |menu, _, _| {
                        [25_usize, 50, 100].into_iter().fold(menu, |menu, limit| {
                            let workspace = size_workspace.clone();
                            menu.item(PopupMenuItem::new(format!("{limit} rows")).on_click(
                                move |_, window, cx| {
                                    workspace.update(cx, |this, cx| {
                                        this.set_table_data_limit(limit, window, cx)
                                    });
                                },
                            ))
                        })
                    }),
            )
            .child(
                Button::new("visible-table-columns")
                    .outline()
                    .small()
                    .icon(Icon::new(gpui_kit::assets::IconName::Table))
                    .dropdown_caret(true)
                    .label(format!("Columns ({visible_column_count})"))
                    .dropdown_menu(move |menu, _, _| {
                        columns_to_toggle
                            .iter()
                            .enumerate()
                            .fold(menu, |menu, (index, column)| {
                                let workspace = columns_workspace.clone();
                                let toggle_columns = columns_to_toggle.clone();
                                let column_index = index;
                                menu.item(
                                    PopupMenuItem::new(column.clone())
                                        .checked(!hidden_columns_for_menu.contains(column))
                                        .on_click(move |_, _, cx| {
                                            workspace.update(cx, |this, cx| {
                                                this.toggle_table_data_column(
                                                    &toggle_columns[column_index],
                                                    toggle_columns.as_ref(),
                                                    cx,
                                                );
                                            });
                                        }),
                                )
                            })
                    }),
            )
            .child(
                Button::new("refresh-table-data")
                    .ghost()
                    .small()
                    .icon(Icon::new(gpui_kit::assets::IconName::RefreshCw))
                    .tooltip("Refresh loaded rows")
                    .disabled(query_running)
                    .on_click(move |_, window, cx| {
                        refresh_workspace
                            .update(cx, |this, cx| this.refresh_selected_table(window, cx));
                    }),
            )
            .into_any_element()
    } else {
        div().into_any_element()
    };

    let page_footer = if result.editable.is_some() {
        let workspace = workspace.clone();
        let page_size_menu = {
            let workspace = workspace.clone();
            Button::new("table-page-size")
                .outline()
                .xsmall()
                .label(format!("Page size: {table_data_limit}"))
                .dropdown_menu(move |menu, _, _| {
                    [25_usize, 50, 100].into_iter().fold(menu, |menu, limit| {
                        let workspace = workspace.clone();
                        menu.item(PopupMenuItem::new(format!("Rows {limit}")).on_click(
                            move |_, window, cx| {
                                workspace.update(cx, |this, cx| {
                                    this.set_table_data_limit(limit, window, cx)
                                })
                            },
                        ))
                    })
                })
        };
        let row_range = if result.rows.is_empty() {
            "0 rows".to_owned()
        } else {
            format!(
                "Rows {}–{}",
                table_data_offset + 1,
                table_data_offset + result.rows.len()
            )
        };
        h_flex()
            .items_center()
            .justify_between()
            .child(
                div()
                    .text_xs()
                    .text_color(cx.theme().muted_foreground)
                    .child(format!("{row_range} · page size {table_data_limit}")),
            )
            .child(page_size_menu)
            .child(
                h_flex()
                    .gap_1()
                    .child(
                        Button::new("previous-data-page")
                            .ghost()
                            .xsmall()
                            .label("Prev")
                            .disabled(query_running || table_data_offset == 0)
                            .on_click({
                                let workspace = workspace.clone();
                                move |_, window, cx| {
                                    workspace.update(cx, |this, cx| {
                                        this.previous_table_data_page(window, cx)
                                    })
                                }
                            }),
                    )
                    .child(
                        Button::new("next-data-page")
                            .primary()
                            .xsmall()
                            .label("Next")
                            .disabled(query_running || !table_data_has_next)
                            .on_click(move |_, window, cx| {
                                workspace
                                    .update(cx, |this, cx| this.next_table_data_page(window, cx))
                            }),
                    ),
            )
            .into_any_element()
    } else {
        div().into_any_element()
    };

    let column_filter_chips = if workspace_layout && result.editable.is_some() {
        h_flex()
            .w_full()
            .flex_wrap()
            .gap_1()
            .px_3()
            .py_1()
            .children(
                table_column_filters
                    .iter()
                    .enumerate()
                    .map(|(index, filter)| {
                        let workspace = workspace.clone();
                        let column = filter.column.clone();
                        Button::new(format!("active-column-filter-{index}"))
                            .outline()
                            .xsmall()
                            .label(table_filter_chip_label(filter))
                            .on_click(move |_, window, cx| {
                                workspace.update(cx, |this, cx| {
                                    this.clear_table_column_filter(&column, window, cx)
                                });
                            })
                            .into_any_element()
                    }),
            )
            .into_any_element()
    } else {
        div().into_any_element()
    };

    let cell_edit_bar = active_cell_edit
        .as_ref()
        .filter(|_| workspace_layout && result.editable.is_some())
        .map(|edit| {
            let editing_null = edit.set_null;
            let column = result
                .columns
                .get(edit.column_index)
                .map(String::as_str)
                .unwrap_or("cell");
            let row_label = table_data_offset + edit.row_index + 1;
            let null_workspace = workspace.clone();
            let save_workspace = workspace.clone();
            let cancel_workspace = workspace.clone();
            h_flex()
                .w_full()
                .items_center()
                .gap_2()
                .px_3()
                .py_1()
                .border_b_1()
                .border_color(cx.theme().border)
                .bg(cx.theme().secondary)
                .child(
                    div()
                        .flex_1()
                        .text_sm()
                        .child(format!("Editing {column} · row {row_label}")),
                )
                .child(
                    Button::new("cell-edit-null-toggle")
                        .outline()
                        .xsmall()
                        .label(if editing_null {
                            "Restore value"
                        } else {
                            "Set SQL NULL"
                        })
                        .disabled(query_running)
                        .on_click(move |_, _, cx| {
                            null_workspace
                                .update(cx, |this, cx| this.set_cell_edit_null(!editing_null, cx));
                        }),
                )
                .child(
                    Button::new("cell-edit-save")
                        .primary()
                        .xsmall()
                        .label("Save")
                        .disabled(query_running)
                        .on_click(move |_, _, cx| {
                            save_workspace.update(cx, |this, cx| this.save_table_cell_edit(cx));
                        }),
                )
                .child(
                    Button::new("cell-edit-cancel")
                        .ghost()
                        .xsmall()
                        .label("Cancel")
                        .disabled(query_running)
                        .on_click(move |_, _, cx| {
                            cancel_workspace.update(cx, |this, cx| this.cancel_table_cell_edit(cx));
                        }),
                )
                .into_any_element()
        })
        .unwrap_or_else(|| div().into_any_element());

    if workspace_layout {
        let workspace = workspace.clone();
        let row_range = if result.rows.is_empty() {
            "0 rows".to_owned()
        } else {
            let range = format!(
                "Rows {}–{}",
                table_data_offset + 1,
                table_data_offset + result.rows.len()
            );
            if table_data_has_next {
                format!("{range} · more rows available")
            } else {
                range
            }
        };
        v_flex()
            .w_full()
            .flex_1()
            .min_h(px(0.))
            .bg(cx.theme().background)
            .child(
                h_flex()
                    .w_full()
                    .h(px(60.))
                    .items_center()
                    .gap_2()
                    .px_3()
                    .border_b_1()
                    .border_color(cx.theme().border)
                    .child(filter_bar)
                    .child(workspace_toolbar),
            )
            .child(column_filter_chips)
            .child(cell_edit_bar)
            .child({
                let grid = v_flex()
                    .size_full()
                    .min_h(px(0.))
                    .min_w(px(result
                        .columns
                        .iter()
                        .enumerate()
                        .filter(|(_, column)| !hidden_columns.contains(*column))
                        .map(|(index, _)| column_widths[index])
                        .sum::<f32>()
                        + if workspace_layout { 40. } else { 0. }
                        + if result.editable.is_some() {
                            if workspace_layout { 32. } else { 112. }
                        } else {
                            0.
                        }))
                    .border_b_1()
                    .border_color(cx.theme().border)
                    .child(header)
                    .child(if workspace_layout {
                        div().flex_1().min_h(px(0.)).child(rows).into_any_element()
                    } else {
                        div()
                            .flex_1()
                            .min_h(px(0.))
                            .overflow_y_scrollbar()
                            .child(rows)
                            .into_any_element()
                    });
                if workspace_layout {
                    let scroll = horizontal_scroll
                        .expect("workspace results require a horizontal scroll handle");
                    div()
                        .id("query-result-horizontal-scroll")
                        .flex_1()
                        .min_h(px(0.))
                        .overflow_x_scroll()
                        .track_scroll(scroll)
                        .child(grid)
                        .horizontal_scrollbar(scroll)
                        .into_any_element()
                } else {
                    div()
                        .flex_1()
                        .min_h(px(0.))
                        .overflow_x_scrollbar()
                        .child(grid)
                        .into_any_element()
                }
            })
            .child(
                h_flex()
                    .h(px(40.))
                    .flex_shrink_0()
                    .items_center()
                    .justify_between()
                    .px_3()
                    .border_t_1()
                    .border_color(cx.theme().border)
                    .child(
                        div()
                            .text_xs()
                            .text_color(cx.theme().muted_foreground)
                            .child(if !footer.is_empty() {
                                footer.to_owned()
                            } else if table_data_filter.is_some() || empty_filter.is_some() {
                                format!("{visible_row_count} visible · {row_range}")
                            } else {
                                row_range
                            }),
                    )
                    .child(
                        h_flex()
                            .gap_1()
                            .child(
                                Button::new("previous-data-page")
                                    .ghost()
                                    .xsmall()
                                    .label("Previous")
                                    .disabled(query_running || table_data_offset == 0)
                                    .on_click({
                                        let workspace = workspace.clone();
                                        move |_, window, cx| {
                                            workspace.update(cx, |this, cx| {
                                                this.previous_table_data_page(window, cx)
                                            })
                                        }
                                    }),
                            )
                            .child(
                                Button::new("next-data-page")
                                    .primary()
                                    .xsmall()
                                    .label("Next")
                                    .disabled(query_running || !table_data_has_next)
                                    .on_click(move |_, window, cx| {
                                        workspace.update(cx, |this, cx| {
                                            this.next_table_data_page(window, cx)
                                        })
                                    }),
                            ),
                    ),
            )
            .into_any_element()
    } else {
        v_flex()
            .w_full()
            .gap_2()
            .p_4()
            .rounded_md()
            .border_1()
            .border_color(cx.theme().border)
            .child(result_heading)
            .child(filter_bar)
            .child(
                div().overflow_x_scrollbar().child(
                    v_flex()
                        .border_1()
                        .border_color(cx.theme().border)
                        .rounded_md()
                        .overflow_hidden()
                        .child(header)
                        .child(rows),
                ),
            )
            .child(
                div()
                    .text_sm()
                    .text_color(cx.theme().muted_foreground)
                    .child(footer),
            )
            .child(page_footer)
            .into_any_element()
    }
}

pub(crate) fn visible_column_window(
    columns: &[String],
    column_widths: &[f32],
    hidden_columns: &std::collections::HashSet<String>,
    workspace_layout: bool,
    scroll: Option<&gpui_kit::ScrollHandle>,
) -> (Vec<usize>, f32, f32) {
    let all_visible_columns = columns
        .iter()
        .enumerate()
        .filter_map(|(index, name)| (!hidden_columns.contains(name)).then_some(index))
        .collect::<Vec<_>>();
    let Some(scroll) = scroll.filter(|_| workspace_layout) else {
        return (all_visible_columns, 0., 0.);
    };
    let viewport_width = scroll.bounds().size.width.as_f32();
    if viewport_width <= 0. {
        return (all_visible_columns, 0., 0.);
    }

    let viewport_left = (-scroll.offset().x.as_f32()).max(0.);
    let viewport_right = viewport_left + viewport_width;
    let mut column_left = if workspace_layout { 40. } else { 0. };
    let first_visible_position = all_visible_columns.iter().position(|index| {
        let right = column_left + column_widths[*index];
        let intersects = right > viewport_left && column_left < viewport_right;
        column_left = right;
        intersects
    });
    let Some(first_visible_position) = first_visible_position else {
        return (all_visible_columns, 0., 0.);
    };

    column_left = if workspace_layout { 40. } else { 0. };
    for index in all_visible_columns.iter().take(first_visible_position) {
        column_left += column_widths[*index];
    }
    let first_visible_left = column_left;
    let mut visible_end_position = first_visible_position;
    for (position, index) in all_visible_columns
        .iter()
        .enumerate()
        .skip(first_visible_position)
    {
        let right = column_left + column_widths[*index];
        if column_left >= viewport_right {
            break;
        }
        visible_end_position = position + 1;
        column_left = right;
    }
    let visible_columns =
        all_visible_columns[first_visible_position..visible_end_position].to_vec();
    let trailing_width = all_visible_columns[visible_end_position..]
        .iter()
        .map(|index| column_widths[*index])
        .sum();
    let leading_width = first_visible_left - if workspace_layout { 40. } else { 0. };
    (visible_columns, leading_width, trailing_width)
}

#[allow(clippy::too_many_arguments)]
fn render_result_row(
    result: &QueryResult,
    row: &[String],
    row_index: usize,
    source_row_index: usize,
    eager_cell_tooltips: bool,
    table_data_offset: usize,
    workspace_layout: bool,
    column_widths: &[f32],
    visible_column_indices: &[usize],
    leading_column_width: f32,
    trailing_column_width: f32,
    workspace: &Entity<DatabaseWorkspace>,
    active_cell_edit: Option<&crate::ui::database_workspace::ActiveCellEditView>,
    query_pending: bool,
    cx: &mut Context<DatabaseWorkspace>,
) -> gpui_kit::AnyElement {
    #[cfg(test)]
    RESULT_ROW_RENDER_COUNT.fetch_add(1, Ordering::Relaxed);
    let mut row_element = h_flex()
        .w_full()
        .h(px(38.))
        .items_center()
        .gap_0()
        .px_3()
        .py_1()
        .bg(if row_index.is_multiple_of(2) {
            cx.theme().table
        } else {
            cx.theme().table_even
        })
        .border_b_1()
        .border_color(cx.theme().table_row_border);
    if workspace_layout {
        row_element = row_element.child(
            div()
                .w(px(40.))
                .text_xs()
                .font_family(cx.theme().mono_font_family.clone())
                .text_color(cx.theme().muted_foreground)
                .child((table_data_offset + row_index + 1).to_string()),
        );
    }
    if leading_column_width > 0. {
        row_element = row_element.child(div().w(px(leading_column_width)));
    }
    row_element = row_element.children(visible_column_indices.iter().filter_map(|column_index| {
        let cell = row.get(*column_index)?;
        #[cfg(test)]
        RESULT_CELL_RENDER_COUNT.fetch_add(1, Ordering::Relaxed);
        let visible_prefix = visible_cell_prefix(cell);
        let visible_cell_value = visible_prefix
            .map(|prefix| gpui_kit::SharedString::new(format!("{prefix}…")))
            .unwrap_or_else(|| gpui_kit::SharedString::new(cell));
        let column_type = result
            .column_types
            .get(*column_index)
            .map(String::as_str)
            .unwrap_or_default();
        let is_boolean = column_type.eq_ignore_ascii_case("bool");
        let is_null = result
            .null_cells
            .get(source_row_index)
            .and_then(|nulls| nulls.get(*column_index))
            .copied()
            .unwrap_or(false);
        let editing = active_cell_edit.filter(|editor| {
            editor.row_index == source_row_index && editor.column_index == *column_index
        });
        let editable =
            workspace_layout && is_inline_editable_cell(result, source_row_index, *column_index);
        let mut cell_element = div()
            .w(px(column_widths[*column_index]))
            .px_2()
            .overflow_hidden()
            .id(format!("cell-{row_index}-{column_index}"));
        if let Some(editor) = editing {
            let save_key_workspace = workspace.clone();
            let cancel_key_workspace = workspace.clone();
            let editor_control = if let Some(enum_values) = editor
                .enum_values
                .as_ref()
                .filter(|values| !values.is_empty() && values.len() <= MAX_ENUM_MENU_OPTIONS)
            {
                let workspace_for_value = workspace.clone();
                let values_for_menu = enum_values.clone();
                let selected_value = editor.enum_value.clone();
                let value_label = if editor.set_null {
                    "NULL".to_owned()
                } else {
                    selected_value
                        .clone()
                        .unwrap_or_else(|| "Choose a value".to_owned())
                };
                Button::new(format!("enum-cell-editor-{row_index}-{column_index}"))
                    .outline()
                    .small()
                    .w_full()
                    .disabled(editor.set_null || query_pending)
                    .dropdown_caret(true)
                    .label(value_label)
                    .dropdown_menu(move |menu, _, _| {
                        values_for_menu.iter().fold(menu, |menu, value| {
                            let workspace = workspace_for_value.clone();
                            let value = value.clone();
                            menu.item(
                                PopupMenuItem::new(value.clone())
                                    .checked(selected_value.as_ref() == Some(&value))
                                    .on_click(move |_, window, cx| {
                                        workspace.update(cx, |this, cx| {
                                            this.select_table_cell_enum_value(
                                                value.clone(),
                                                window,
                                                cx,
                                            );
                                        });
                                    }),
                            )
                        })
                    })
                    .into_any_element()
            } else {
                Input::new(&editor.input)
                    .w_full()
                    .small()
                    .disabled(editor.set_null || query_pending)
                    .into_any_element()
            };
            let enter_saves = editor
                .enum_values
                .as_ref()
                .is_none_or(|values| values.is_empty() || values.len() > MAX_ENUM_MENU_OPTIONS);
            let editor = div()
                .on_key_down(move |event, _, cx| match event.keystroke.key.as_str() {
                    "enter" if enter_saves => save_key_workspace.update(cx, |this, cx| {
                        this.save_table_cell_edit(cx);
                    }),
                    "escape" => cancel_key_workspace.update(cx, |this, cx| {
                        this.cancel_table_cell_edit(cx);
                    }),
                    _ => {}
                })
                .child(editor_control);
            cell_element = cell_element.child(editor);
        } else {
            if is_boolean {
                let is_true = cell.eq_ignore_ascii_case("true");
                cell_element = cell_element.child(
                    h_flex()
                        .items_center()
                        .font_family(cx.theme().mono_font_family.clone())
                        .px_2()
                        .rounded_full()
                        .text_xs()
                        .bg(if is_true {
                            cx.theme().sidebar_accent
                        } else {
                            cx.theme().secondary
                        })
                        .text_color(if is_true || is_null {
                            cx.theme().sidebar_accent_foreground
                        } else {
                            cx.theme().muted_foreground
                        })
                        .child(visible_cell_value.clone()),
                );
            } else {
                cell_element = cell_element
                    .text_sm()
                    .font_family(cx.theme().mono_font_family.clone())
                    .whitespace_nowrap()
                    .text_color(if is_null {
                        cx.theme().muted_foreground
                    } else {
                        cx.theme().foreground
                    })
                    .child(visible_cell_value.clone());
            }
            if editable {
                let edit_workspace = workspace.clone();
                let edit_row_index = source_row_index;
                let edit_column_index = *column_index;
                cell_element = cell_element.on_click(move |_, window, cx| {
                    edit_workspace.update(cx, |this, cx| {
                        this.begin_table_cell_edit(edit_row_index, edit_column_index, window, cx);
                    });
                });
            }
        }
        Some(if visible_prefix.is_some() && editing.is_none() {
            let eager_tooltip_value =
                eager_cell_tooltips.then(|| gpui_kit::SharedString::new(cell));
            let tooltip_workspace = workspace.clone();
            let tooltip_row_index = source_row_index;
            let tooltip_column_index = *column_index;
            cell_element
                .tooltip(move |window, cx| {
                    // Large cell values are owned only when a tooltip is actually requested.
                    let tooltip_value = eager_tooltip_value.clone().unwrap_or_else(|| {
                        tooltip_workspace
                            .read(cx)
                            .query_result()
                            .and_then(|result| {
                                result_cell_value(result, tooltip_row_index, tooltip_column_index)
                            })
                            .map(gpui_kit::SharedString::new)
                            .unwrap_or_default()
                    });
                    gpui_kit::component::tooltip::Tooltip::new(tooltip_value).build(window, cx)
                })
                .into_any_element()
        } else {
            cell_element.into_any_element()
        })
    }));
    if trailing_column_width > 0. {
        row_element = row_element.child(div().w(px(trailing_column_width)));
    }
    #[cfg(test)]
    let row_sql_actions_enabled = ROW_SQL_ACTIONS_FOR_BENCHMARK.load(Ordering::Relaxed);
    #[cfg(not(test))]
    let row_sql_actions_enabled = true;
    let has_row_sql_actions = row_sql_actions_enabled
        && result.editable.as_ref().is_some_and(|table| {
            !table.primary_key_columns.is_empty()
                && table.primary_key_columns.iter().all(|key| {
                    result
                        .columns
                        .iter()
                        .position(|column| column == key)
                        .is_some_and(|index| row.get(index).is_some())
                })
        });
    if has_row_sql_actions {
        let action_workspace = workspace.clone();
        let action_row_index = source_row_index;
        let mut row_action_button = Button::new(format!("row-sql-actions-{row_index}"))
            .outline()
            .xsmall();
        if workspace_layout {
            row_action_button = row_action_button
                .ghost()
                .w(px(32.))
                .icon(Icon::new(gpui_kit::assets::IconName::Ellipsis))
                .tooltip("Row SQL actions");
        } else {
            row_action_button = row_action_button.w(px(112.)).label("SQL actions");
        }
        let row_actions = row_action_button.dropdown_menu(move |menu, _, cx| {
            let (update_sql, delete_sql) = {
                let workspace = action_workspace.read(cx);
                workspace
                    .query_result()
                    .and_then(|result| {
                        let table = result.editable.as_ref()?;
                        let row = result.rows.get(action_row_index)?;
                        Some((
                            build_update_sql(table, &result.columns, row),
                            build_delete_sql(table, &result.columns, row),
                        ))
                    })
                    .unwrap_or_default()
            };
            let menu = if let Some(sql) = update_sql.clone() {
                let workspace = action_workspace.clone();
                menu.item(PopupMenuItem::new("Prepare UPDATE SQL").on_click(
                    move |_, window, cx| {
                        workspace.update(cx, |this, cx| {
                            this.prepare_query(sql.clone(), window, cx);
                        });
                    },
                ))
            } else {
                menu
            };
            if let Some(sql) = delete_sql.clone() {
                let workspace = action_workspace.clone();
                menu.item(PopupMenuItem::new("Prepare DELETE SQL").on_click(
                    move |_, window, cx| {
                        workspace.update(cx, |this, cx| {
                            this.prepare_query(sql.clone(), window, cx);
                        });
                    },
                ))
            } else {
                menu
            }
        });
        row_element = row_element.child(row_actions);
    }
    row_element.into_any_element()
}

fn table_column_width(name: &str, column_type: &str) -> f32 {
    if name.eq_ignore_ascii_case("id") || ends_with_ascii_case_insensitive(name, "_id") {
        88.
    } else if contains_ascii_case_insensitive(name, "email") {
        210.
    } else if contains_ascii_case_insensitive(column_type, "timestamp")
        || ends_with_ascii_case_insensitive(name, "_at")
    {
        230.
    } else if contains_ascii_case_insensitive(column_type, "bool") {
        112.
    } else if name.eq_ignore_ascii_case("role") || name.eq_ignore_ascii_case("status") {
        140.
    } else {
        176.
    }
}

fn is_inline_editable_cell(result: &QueryResult, row_index: usize, column_index: usize) -> bool {
    let Some(table) = result.editable.as_ref() else {
        return false;
    };
    let Some(column) = result.columns.get(column_index) else {
        return false;
    };
    !table.primary_key_columns.is_empty()
        && !table.primary_key_columns.iter().any(|key| key == column)
        && result
            .rows
            .get(row_index)
            .and_then(|row| row.get(column_index))
            .is_some()
        && (crate::ui::database_workspace::is_inline_edit_type(
            result.column_types.get(column_index).map(String::as_str),
        ) || result
            .column_enum_values
            .get(column_index)
            .is_some_and(|values| values.as_ref().is_some_and(|values| !values.is_empty())))
}

fn ends_with_ascii_case_insensitive(value: &str, suffix: &str) -> bool {
    value
        .get(value.len().saturating_sub(suffix.len())..)
        .is_some_and(|value_suffix| value_suffix.eq_ignore_ascii_case(suffix))
}

fn visible_cell_prefix(value: &str) -> Option<&str> {
    if value.len() <= TABLE_CELL_PREVIEW_CHARS {
        return None;
    }
    value
        .char_indices()
        .nth(TABLE_CELL_PREVIEW_CHARS)
        .map(|(end, _)| &value[..end])
}

fn result_cell_value(result: &QueryResult, row: usize, column: usize) -> Option<&str> {
    result.rows.get(row)?.get(column).map(String::as_str)
}

fn contains_ascii_case_insensitive(value: &str, pattern: &str) -> bool {
    let value = value.as_bytes();
    let pattern = pattern.as_bytes();
    pattern.is_empty()
        || value.windows(pattern.len()).any(|window| {
            window
                .iter()
                .zip(pattern)
                .all(|(left, right)| left.eq_ignore_ascii_case(right))
        })
}

struct AsciiCaseInsensitiveMatcher {
    pattern: Vec<u8>,
    prefix_lengths: Vec<usize>,
}

impl AsciiCaseInsensitiveMatcher {
    fn new(pattern: &str) -> Self {
        let pattern = pattern
            .bytes()
            .map(|byte| byte.to_ascii_lowercase())
            .collect::<Vec<_>>();
        let mut prefix_lengths = vec![0; pattern.len()];
        let mut prefix_length = 0;
        for index in 1..pattern.len() {
            while prefix_length > 0 && pattern[index] != pattern[prefix_length] {
                prefix_length = prefix_lengths[prefix_length - 1];
            }
            if pattern[index] == pattern[prefix_length] {
                prefix_length += 1;
            }
            prefix_lengths[index] = prefix_length;
        }
        Self {
            pattern,
            prefix_lengths,
        }
    }

    fn contains(&self, value: &str) -> bool {
        if self.pattern.is_empty() {
            return true;
        }

        let mut matched = 0;
        for byte in value.bytes().map(|byte| byte.to_ascii_lowercase()) {
            while matched > 0 && byte != self.pattern[matched] {
                matched = self.prefix_lengths[matched - 1];
            }
            if byte == self.pattern[matched] {
                matched += 1;
                if matched == self.pattern.len() {
                    return true;
                }
            }
        }
        false
    }
}

#[cfg(test)]
pub(crate) fn matching_row_indices(
    result: &QueryResult,
    filter: Option<&str>,
    filter_column: Option<&str>,
    empty_filter: Option<&(String, bool)>,
    max_visible_rows: usize,
) -> Option<Vec<usize>> {
    matching_row_indices_with_cancellation(
        result,
        filter,
        filter_column,
        empty_filter,
        max_visible_rows,
        || true,
    )
}

pub(crate) fn matching_row_indices_with_cancellation(
    result: &QueryResult,
    filter: Option<&str>,
    filter_column: Option<&str>,
    empty_filter: Option<&(String, bool)>,
    max_visible_rows: usize,
    mut should_continue: impl FnMut() -> bool,
) -> Option<Vec<usize>> {
    if filter.is_none() && empty_filter.is_none() {
        return None;
    }
    if max_visible_rows == 0 {
        return Some(Vec::new());
    }
    let empty_filter_column_index = empty_filter
        .as_ref()
        .and_then(|(column, _)| result.columns.iter().position(|name| name == column));
    let filter_column_index =
        filter_column.and_then(|column| result.columns.iter().position(|name| name == column));
    let filter_matcher = filter.map(AsciiCaseInsensitiveMatcher::new);

    let mut indices = Vec::new();
    for (row_index, row) in result.rows.iter().enumerate() {
        if !should_continue() {
            return None;
        }
        let matches_filter = empty_filter.is_none_or(|(_, is_empty)| {
            empty_filter_column_index
                .and_then(|index| row.get(index))
                .is_some_and(|cell| {
                    let empty = cell.is_empty() || cell.eq_ignore_ascii_case("NULL");
                    empty == *is_empty
                })
        }) && filter_matcher.as_ref().is_none_or(|matcher| {
            if filter_column.is_some() {
                filter_column_index
                    .and_then(|index| row.get(index))
                    .is_some_and(|cell| matcher.contains(cell))
            } else {
                row.iter().any(|cell| matcher.contains(cell))
            }
        });
        if matches_filter {
            indices.push(row_index);
            if indices.len() == max_visible_rows {
                break;
            }
        }
    }
    Some(indices)
}

#[cfg(test)]
mod tests {
    use super::{
        AsciiCaseInsensitiveMatcher, TABLE_CELL_PREVIEW_CHARS, contains_ascii_case_insensitive,
        matching_row_indices, matching_row_indices_with_cancellation, result_cell_value,
        table_column_width, visible_cell_prefix,
    };
    use crate::domain::query::QueryResult;

    #[test]
    fn matches_ascii_without_allocating_a_lowercased_copy() {
        assert!(contains_ascii_case_insensitive("Audit_Logs", "AUDIT"));
        assert!(contains_ascii_case_insensitive("éAudit", "éAUDIT"));
        assert!(!contains_ascii_case_insensitive("Audit", "logs"));
        assert!(contains_ascii_case_insensitive("anything", ""));
    }

    #[test]
    fn compiled_filter_matcher_preserves_ascii_insensitive_substring_semantics() {
        for (value, pattern) in [
            ("Audit_Logs", "AUDIT"),
            ("éAudit", "éAUDIT"),
            ("abababac", "ababac"),
            ("anything", ""),
            ("Audit", "logs"),
        ] {
            assert_eq!(
                AsciiCaseInsensitiveMatcher::new(pattern).contains(value),
                contains_ascii_case_insensitive(value, pattern),
                "value={value:?}, pattern={pattern:?}"
            );
        }
    }

    #[test]
    #[ignore = "filter matcher microbenchmark; invoke explicitly with --ignored"]
    fn benchmarks_long_near_match_filter() {
        use std::time::Instant;

        let values = (0..12).map(|_| "a".repeat(16 * 1024)).collect::<Vec<_>>();
        let pattern = format!("{}b", "a".repeat(63));
        let matcher = AsciiCaseInsensitiveMatcher::new(&pattern);
        let mut old_times = Vec::with_capacity(9);
        let mut new_times = Vec::with_capacity(9);
        for _ in 0..9 {
            let started = Instant::now();
            let old_matches = values
                .iter()
                .filter(|value| contains_ascii_case_insensitive(value, &pattern))
                .count();
            old_times.push(started.elapsed());
            let started = Instant::now();
            let new_matches = values
                .iter()
                .filter(|value| matcher.contains(value))
                .count();
            new_times.push(started.elapsed());
            assert_eq!(old_matches, new_matches);
        }
        old_times.sort_unstable();
        new_times.sort_unstable();
        let old_median = old_times[4];
        let new_median = new_times[4];
        let speedup = old_median.as_secs_f64() / new_median.as_secs_f64();
        eprintln!(
            "long near-match filter over 12 x 16 KiB strings: window scan={:.2} ms, compiled matcher={:.2} ms, speedup={speedup:.1}x",
            old_median.as_secs_f64() * 1000.0,
            new_median.as_secs_f64() * 1000.0,
        );
    }

    #[test]
    fn long_cell_preview_stops_on_a_character_boundary() {
        let value = format!("{}étail", "a".repeat(160));
        assert_eq!(visible_cell_prefix(&value), Some(&value[..160]));
        assert!(visible_cell_prefix("short value").is_none());
        assert!(visible_cell_prefix(&"é".repeat(80)).is_none());
    }

    #[test]
    fn tooltip_lookup_returns_the_full_untruncated_result_cell() {
        let value = format!("{}tail", "x".repeat(TABLE_CELL_PREVIEW_CHARS + 1));
        let result = QueryResult {
            columns: vec!["large_text".into()],
            column_types: vec!["text".into()],
            column_enum_values: vec![None],
            rows: vec![vec![value.clone()]],
            null_cells: vec![vec![false]],
            offset: 0,
            limit: 1,
            has_next: false,
            truncated: false,
            editable: None,
        };

        assert_eq!(result_cell_value(&result, 0, 0), Some(value.as_str()));
        assert_eq!(result_cell_value(&result, 1, 0), None);
    }

    #[test]
    fn assigns_column_widths_without_case_normalization() {
        assert_eq!(table_column_width("USER_EMAIL", "text"), 210.);
        assert_eq!(
            table_column_width("created_AT", "timestamp without time zone"),
            230.
        );
        assert_eq!(table_column_width("is_ACTIVE", "BOOLEAN"), 112.);
        assert_eq!(table_column_width("Status", "text"), 140.);
    }

    #[test]
    fn avoids_building_filtered_indices_without_active_filters() {
        let result = QueryResult {
            columns: vec!["name".into()],
            column_types: vec!["text".into()],
            column_enum_values: vec![None],
            rows: vec![vec!["first".into()], vec!["second".into()]],
            null_cells: vec![vec![false], vec![false]],
            offset: 0,
            limit: 2,
            has_next: false,
            truncated: false,
            editable: None,
        };

        assert_eq!(
            matching_row_indices(&result, None, None, None, usize::MAX),
            None
        );
        assert_eq!(
            matching_row_indices(&result, Some("OND"), Some("name"), None, usize::MAX),
            Some(vec![1])
        );
        assert_eq!(
            matching_row_indices(&result, Some("OND"), Some("missing"), None, usize::MAX),
            Some(vec![])
        );
    }

    #[test]
    fn filtered_row_matching_can_be_cancelled_and_respects_zero_limit() {
        let result = QueryResult {
            columns: vec!["name".into()],
            column_types: vec!["text".into()],
            column_enum_values: vec![None],
            rows: vec![vec!["first".into()], vec!["second".into()]],
            null_cells: vec![vec![false], vec![false]],
            offset: 0,
            limit: 2,
            has_next: false,
            truncated: false,
            editable: None,
        };
        let mut rows_checked = 0;

        assert_eq!(
            matching_row_indices_with_cancellation(
                &result,
                Some("i"),
                None,
                None,
                usize::MAX,
                || {
                    rows_checked += 1;
                    rows_checked <= 1
                },
            ),
            None
        );
        assert_eq!(
            matching_row_indices_with_cancellation(&result, Some("i"), None, None, 0, || panic!(
                "zero visible rows should not scan input"
            ),),
            Some(vec![])
        );
    }
}
