pub(crate) mod connections;
pub(crate) mod empty;
pub(crate) mod notices;
pub(crate) mod query;
pub(crate) mod titlebar;

pub(crate) use titlebar::render as render_titlebar;

use gpui_kit::base::{h_flex, v_flex};
use gpui_kit::component::{
    ActiveTheme as _, Sizable as _, button::Button, input::TextareaState, resizable_panel,
    status_bar::StatusBar, v_resizable,
};
use gpui_kit::{
    Context, Entity, FontWeight, IntoElement, ParentElement as _, Styled as _, div, px,
};

use crate::domain::connection::ConnectionSummary;
use crate::infrastructure::QueryResult;
use crate::ui::{ConnectionEditor, DatabaseWorkspace, Notice, NoticeLevel};
use std::collections::HashSet;
use std::rc::Rc;

pub(crate) struct HomepageView<'a> {
    pub(crate) connections: &'a [ConnectionSummary],
    pub(crate) selected_connection: Option<&'a ConnectionSummary>,
    pub(crate) connection_editor: Option<&'a ConnectionEditor>,
    pub(crate) connection_test_running: bool,
    pub(crate) pending_delete: Option<&'a String>,
    pub(crate) notice: Option<&'a Notice>,
    pub(crate) connected: bool,
    pub(crate) server_version: Option<&'a str>,
    pub(crate) query_input: &'a Entity<TextareaState>,
    pub(crate) query_result: Option<&'a QueryResult>,
    pub(crate) result_column_widths: Rc<Vec<f32>>,
    pub(crate) query_running: bool,
    pub(crate) write_confirmation_pending: bool,
}

pub fn render(cx: &mut Context<DatabaseWorkspace>, view: HomepageView<'_>) -> impl IntoElement {
    let status = if let Some(notice) = view.notice {
        format!("{}: {}", notice.title, notice.message)
    } else if view.connected {
        "●  Connected".to_string()
    } else {
        "●  Not connected".to_string()
    };

    let connection_content = if let Some(editor) = view.connection_editor {
        editor
            .render(view.connection_test_running, cx)
            .into_any_element()
    } else {
        connections::render(
            cx,
            view.connections,
            view.selected_connection,
            view.connected,
            view.pending_delete,
        )
        .into_any_element()
    };

    let mut workspace = v_flex()
        .w(px(860.))
        .gap_6()
        .child(
            v_flex()
                .gap_1()
                .child(
                    div()
                        .text_2xl()
                        .font_weight(FontWeight::BOLD)
                        .text_color(cx.theme().primary)
                        .child("Database connections"),
                )
                .child(
                    div()
                        .text_sm()
                        .text_color(cx.theme().muted_foreground)
                        .child("Connect to a PostgreSQL server to browse and query your data."),
                ),
        )
        .child(connection_content);
    if view.connected {
        workspace = workspace.child(query::render_panel(
            cx,
            view.query_input,
            view.query_running,
            view.write_confirmation_pending,
        ));
        workspace = workspace.child(query::render_result(
            cx,
            view.query_result,
            view.result_column_widths,
            0,
            500,
            false,
            view.query_running,
            None,
            None,
            None,
            None,
            None,
            None,
            false,
            HashSet::new(),
            None,
            None,
            false,
            &[],
            None,
            None,
        ));
    } else if view.connection_editor.is_none() {
        workspace = workspace.child(empty::recent(cx));
    }

    v_flex()
        .size_full()
        .bg(cx.theme().background)
        .text_color(cx.theme().foreground)
        .child(if let Some(notice) = view.notice {
            notices::render(cx, notice).into_any_element()
        } else {
            div().into_any_element()
        })
        .child(
            v_flex()
                .flex_1()
                .items_center()
                .justify_center()
                .child(workspace),
        )
        .child(
            StatusBar::new().left(status).right(
                view.server_version
                    .map_or("PostgreSQL".to_string(), |version| {
                        format!("PostgreSQL {version}")
                    }),
            ),
        )
}

#[allow(clippy::too_many_arguments)]
pub fn render_workspace(
    cx: &mut Context<DatabaseWorkspace>,
    notice: Option<&Notice>,
    selected_table: Option<&(String, String)>,
    query_input: &Entity<TextareaState>,
    query_result: Option<&QueryResult>,
    result_column_widths: Rc<Vec<f32>>,
    query_running: bool,
    write_confirmation_pending: bool,
    table_data_offset: usize,
    table_data_limit: usize,
    table_data_has_next: bool,
    table_data_sort: Option<&(String, bool)>,
    table_data_filter_input: &Entity<gpui_kit::component::input::InputState>,
    table_data_filter: Option<&str>,
    table_data_filter_column: Option<String>,
    table_data_empty_filter: Option<(String, bool)>,
    filtered_table_data_rows: Option<Rc<Vec<usize>>>,
    table_data_filter_pending: bool,
    hidden_table_data_columns: HashSet<String>,
    table_result_scroll: &gpui_kit::UniformListScrollHandle,
    table_result_horizontal_scroll: &gpui_kit::ScrollHandle,
    compact_layout: bool,
    query_dock_tab: crate::ui::database_workspace::QueryDockTab,
    table_column_filters: &[crate::domain::query::TableColumnFilter],
    active_cell_edit: Option<crate::ui::database_workspace::ActiveCellEditView>,
    table_filter_editor: Option<(
        Option<String>,
        crate::domain::query::TableFilterOperator,
        Entity<gpui_kit::component::input::InputState>,
    )>,
) -> impl IntoElement {
    let relation = selected_table.cloned().or_else(|| {
        query_result
            .and_then(|result| result.editable.as_ref())
            .map(|table| (table.schema.clone(), table.table.clone()))
    });
    v_flex()
        .size_full()
        .flex_1()
        .min_w(px(0.))
        .bg(cx.theme().background)
        .text_color(cx.theme().foreground)
        .child(
            if let Some(notice) = notice.filter(|notice| notice.level != NoticeLevel::Success) {
                notices::render(cx, notice).into_any_element()
            } else {
                div().into_any_element()
            },
        )
        .child(
            v_flex()
                .flex_1()
                .min_h(px(0.))
                .gap_0()
                .px_3()
                .pt_3()
                .child(
                    h_flex()
                        .h(px(82.))
                        .justify_between()
                        .items_center()
                        .px_1()
                        .pb_2()
                        .border_b_1()
                        .border_color(cx.theme().border)
                        .child(
                            v_flex().gap_1().child(if let Some((schema, table)) = relation.as_ref() {
                                h_flex()
                                    .items_center()
                                    .gap_2()
                                    .child(
                                        div()
                                            .text_xl()
                                            .font_weight(FontWeight::BOLD)
                                            .text_color(cx.theme().primary)
                                            .child(schema.clone()),
                                    )
                                    .child(gpui_kit::component::Icon::new(
                                        gpui_kit::assets::IconName::ChevronRight,
                                    ))
                                    .child(
                                        div()
                                            .text_xl()
                                            .font_weight(FontWeight::BOLD)
                                            .child(table.clone()),
                                    )
                                    .into_any_element()
                            } else {
                                div()
                                    .text_xl()
                                    .font_weight(FontWeight::BOLD)
                                    .child("Database workspace")
                                    .into_any_element()
                            })
                            .child(
                                div()
                                    .text_sm()
                                    .text_color(cx.theme().muted_foreground)
                                    .child(relation.as_ref().map_or_else(
                                        || "Browse tables and run safe queries.".to_owned(),
                                        |(schema, _)| {
                                            let columns = query_result
                                                .map(|result| result.columns.len())
                                                .unwrap_or_default();
                                            let rows = query_result
                                                .map(|result| result.rows.len())
                                                .unwrap_or_default();
                                            format!(
                                                "Table in schema {schema} · {columns} columns · {rows} rows loaded"
                                            )
                                        },
                                    )),
                            ),
                        )
                        .child(
                            h_flex()
                                .items_center()
                                .gap_2()
                                .child(if compact_layout {
                                    Button::new("show-table-sidebar")
                                        .outline()
                                        .xsmall()
                                        .label("Tables")
                                        .on_click(cx.listener(|this, _, _, cx| {
                                            this.toggle_table_sidebar(cx)
                                        }))
                                        .into_any_element()
                                } else {
                                    div().into_any_element()
                                })
                        ),
                )
                .child(
                    v_flex()
                        .flex_1()
                        .min_h(px(0.))
                        .child(
                            v_resizable("tablex-data-sql-split")
                                .child(
                                    resizable_panel()
                                        .size(px(660.))
                                        .size_range(px(300.)..px(920.))
                                        .child(query::render_result(
                                            cx,
                                            query_result,
                                            result_column_widths.clone(),
                                            table_data_offset,
                                            table_data_limit,
                                            table_data_has_next,
                                            query_running,
                                            table_data_sort,
                                            Some(table_data_filter_input),
                                            table_data_filter,
                                            table_data_filter_column,
                                            table_data_empty_filter,
                                            filtered_table_data_rows,
                                            table_data_filter_pending,
                                            hidden_table_data_columns,
                                            Some(table_result_scroll),
                                            Some(table_result_horizontal_scroll),
                                            true,
                                            table_column_filters,
                                            active_cell_edit.clone(),
                                            table_filter_editor.clone(),
                                        )),
                                )
                                .child(
                                    resizable_panel()
                                        .size(px(260.))
                                        .size_range(px(180.)..px(480.))
                                        .child(query::render_workspace_dock(
                                            cx,
                                            query_input,
                                            query_result,
                                            query_running,
                                            write_confirmation_pending,
                                            query_dock_tab,
                                        )),
                                ),
                        ),
                ),
        )
}

#[cfg(test)]
#[path = "../../tests/unit/ui/home.rs"]
mod tests;
