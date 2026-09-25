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

mod result_grid;

#[cfg(test)]
pub(crate) use result_grid::visible_column_window;
#[cfg(test)]
use result_grid::{
    is_inline_editable_cell, result_cell_value, table_column_width, visible_cell_prefix,
};
pub(crate) use result_grid::{render_result, result_column_widths};

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
#[path = "../../../tests/unit/ui/home/query.rs"]
mod tests;
