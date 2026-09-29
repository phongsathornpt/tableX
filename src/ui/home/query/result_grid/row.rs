use std::borrow::Cow;

use super::*;

#[allow(clippy::too_many_arguments)]
pub(super) fn render_result_row(
    result: &QueryResult,
    row: &[String],
    row_index: usize,
    source_row_index: usize,
    eager_cell_tooltips: bool,
    table_data_offset: usize,
    workspace_layout: bool,
    column_widths: &[f32],
    pinned_indices: &[usize],
    visible_scrollable_indices: &[usize],
    total_pinned_width: f32,
    leading_column_width: f32,
    trailing_column_width: f32,
    scroll_offset_x: f32,
    workspace: &Entity<DatabaseWorkspace>,
    active_cell_edit: Option<&crate::ui::database_workspace::ActiveCellEditView>,
    query_pending: bool,
    cx: &mut Context<DatabaseWorkspace>,
) -> gpui_kit::AnyElement {
    #[cfg(test)]
    RESULT_ROW_RENDER_COUNT.fetch_add(1, Ordering::Relaxed);
    let bg_color = if row_index.is_multiple_of(2) {
        cx.theme().table
    } else {
        cx.theme().table_even
    };

    let scroll_shift = (leading_column_width - scroll_offset_x).min(0.0);
    let mut scrollable_cells = h_flex()
        .h_full()
        .items_center()
        .gap_0()
        .ml(px(scroll_shift))
        .overflow_hidden();
    scrollable_cells = scrollable_cells.children(visible_scrollable_indices.iter().filter_map(
        |&column_index| {
            render_cell_item(
                result,
                row,
                row_index,
                source_row_index,
                column_index,
                column_widths.get(column_index).copied().unwrap_or(120.0),
                eager_cell_tooltips,
                workspace_layout,
                workspace,
                active_cell_edit,
                query_pending,
                cx,
            )
        },
    ));
    if trailing_column_width > 0. {
        scrollable_cells =
            scrollable_cells.child(div().w(px(trailing_column_width)).flex_shrink_0());
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
        scrollable_cells = scrollable_cells.child(row_actions);
    }

    let mut row_element = h_flex()
        .relative()
        .w_full()
        .h(px(38.))
        .items_center()
        .gap_0()
        .px_2()
        .py_1()
        .bg(bg_color)
        .border_b_1()
        .border_color(cx.theme().table_row_border)
        .overflow_hidden();

    if total_pinned_width > 0. {
        let mut pinned_cells = h_flex()
            .w(px(total_pinned_width))
            .h_full()
            .flex_shrink_0()
            .items_center()
            .bg(bg_color)
            .border_r_1()
            .border_color(cx.theme().border)
            .overflow_hidden();

        if workspace_layout {
            pinned_cells = pinned_cells.child(
                div()
                    .w(px(40.))
                    .h_full()
                    .flex_shrink_0()
                    .items_center()
                    .text_xs()
                    .font_family(cx.theme().mono_font_family.clone())
                    .text_color(cx.theme().muted_foreground)
                    .child((table_data_offset + row_index + 1).to_string()),
            );
        }

        pinned_cells = pinned_cells.children(pinned_indices.iter().filter_map(|&column_index| {
            render_cell_item(
                result,
                row,
                row_index,
                source_row_index,
                column_index,
                column_widths.get(column_index).copied().unwrap_or(120.0),
                eager_cell_tooltips,
                workspace_layout,
                workspace,
                active_cell_edit,
                query_pending,
                cx,
            )
        }));

        row_element = row_element.child(pinned_cells);
    }

    row_element = row_element.child(
        div()
            .flex_1()
            .h_full()
            .overflow_hidden()
            .child(scrollable_cells),
    );

    row_element.into_any_element()
}

#[allow(clippy::too_many_arguments)]
fn render_cell_item(
    result: &QueryResult,
    row: &[String],
    row_index: usize,
    source_row_index: usize,
    column_index: usize,
    column_width: f32,
    eager_cell_tooltips: bool,
    workspace_layout: bool,
    workspace: &Entity<DatabaseWorkspace>,
    active_cell_edit: Option<&crate::ui::database_workspace::ActiveCellEditView>,
    query_pending: bool,
    cx: &mut Context<DatabaseWorkspace>,
) -> Option<gpui_kit::AnyElement> {
    let cell = row.get(column_index)?;
    #[cfg(test)]
    RESULT_CELL_RENDER_COUNT.fetch_add(1, Ordering::Relaxed);
    let flattened_cell = flatten_cell_preview(cell);
    let cell_truncated = result
        .truncated_cells
        .get(source_row_index)
        .and_then(|cells| cells.get(column_index))
        .copied()
        .unwrap_or(false);
    let visible_prefix = visible_cell_prefix(&flattened_cell);
    let has_visible_prefix = visible_prefix.is_some();
    let visible_cell_value = match visible_prefix {
        Some(prefix) => gpui_kit::SharedString::new(format!("{prefix}…")),
        None if cell_truncated => gpui_kit::SharedString::new(format!("{flattened_cell}…")),
        None => gpui_kit::SharedString::new(&flattened_cell),
    };
    let column_type = result
        .column_types
        .get(column_index)
        .map(String::as_str)
        .unwrap_or_default();
    let is_boolean = column_type.eq_ignore_ascii_case("bool");
    let is_null = result
        .null_cells
        .get(source_row_index)
        .and_then(|nulls| nulls.get(column_index))
        .copied()
        .unwrap_or(false);
    let editing = active_cell_edit.filter(|editor| {
        editor.row_index == source_row_index && editor.column_index == column_index
    });
    let editable =
        workspace_layout && is_inline_editable_cell(result, source_row_index, column_index);
    let mut cell_element = div()
        .w(px(column_width))
        .h_full()
        .flex_shrink_0()
        .items_center()
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
            let font_family = if is_mono_column_type(column_type) {
                cx.theme().mono_font_family.clone()
            } else {
                cx.theme().font_family.clone()
            };
            cell_element = cell_element
                .text_sm()
                .font_family(font_family)
                .whitespace_nowrap()
                .text_ellipsis()
                .truncate()
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
            let edit_column_index = column_index;
            cell_element = cell_element.on_click(move |_, window, cx| {
                edit_workspace.update(cx, |this, cx| {
                    this.begin_table_cell_edit(edit_row_index, edit_column_index, window, cx);
                });
            });
        }
    }
    Some(
        if (has_visible_prefix || cell_truncated || cell.contains('\n') || cell.contains('\r'))
            && editing.is_none()
        {
            let eager_tooltip_value = eager_cell_tooltips.then(|| cell.to_owned());
            let tooltip_workspace = workspace.clone();
            let tooltip_row_index = source_row_index;
            let tooltip_column_index = column_index;
            let tooltip_is_truncated = cell_truncated;
            cell_element
                .tooltip(move |window, cx| {
                    let mut tooltip_value = eager_tooltip_value.clone().unwrap_or_else(|| {
                        tooltip_workspace
                            .read(cx)
                            .query_result()
                            .and_then(|result| {
                                result_cell_value(result, tooltip_row_index, tooltip_column_index)
                            })
                            .map(str::to_owned)
                            .unwrap_or_default()
                    });
                    if tooltip_is_truncated {
                        tooltip_value.push_str(
                        "\n\n[Preview shortened to fit display limits; this cell is read-only.]",
                    );
                    }
                    gpui_kit::component::tooltip::Tooltip::new(gpui_kit::SharedString::new(
                        tooltip_value,
                    ))
                    .build(window, cx)
                })
                .into_any_element()
        } else {
            cell_element.into_any_element()
        },
    )
}

pub(crate) fn table_column_width(name: &str, column_type: &str) -> f32 {
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

pub(crate) fn is_inline_editable_cell(
    result: &QueryResult,
    row_index: usize,
    column_index: usize,
) -> bool {
    let Some(table) = result.editable.as_ref() else {
        return false;
    };
    let Some(column) = result.columns.get(column_index) else {
        return false;
    };
    if result
        .truncated_cells
        .get(row_index)
        .and_then(|cells| cells.get(column_index))
        .copied()
        .unwrap_or(false)
    {
        return false;
    }
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

pub(crate) fn flatten_cell_preview(value: &str) -> Cow<'_, str> {
    if !value.contains(['\n', '\r', '\t']) {
        return Cow::Borrowed(value);
    }
    let mut flattened = String::with_capacity(value.len());
    let mut pending_space = false;
    for c in value.chars() {
        if c == '\n' || c == '\r' || c == '\t' || c == ' ' {
            pending_space = true;
        } else {
            if pending_space && !flattened.is_empty() {
                flattened.push(' ');
            }
            pending_space = false;
            flattened.push(c);
        }
    }
    Cow::Owned(flattened)
}

pub(crate) fn is_mono_column_type(type_name: &str) -> bool {
    let lower = type_name.to_ascii_lowercase();
    matches!(
        lower.as_str(),
        "int2"
            | "int4"
            | "int8"
            | "smallint"
            | "integer"
            | "bigint"
            | "float4"
            | "float8"
            | "numeric"
            | "decimal"
            | "real"
            | "double precision"
            | "serial"
            | "bigserial"
            | "smallserial"
            | "date"
            | "time"
            | "timetz"
            | "timestamp"
            | "timestamptz"
            | "interval"
            | "uuid"
            | "bytea"
            | "oid"
            | "inet"
            | "cidr"
            | "macaddr"
            | "bit"
            | "varbit"
    )
}

pub(crate) fn visible_cell_prefix(value: &str) -> Option<&str> {
    value
        .char_indices()
        .nth(TABLE_CELL_PREVIEW_CHARS)
        .map(|(end, _)| &value[..end])
}

pub(crate) fn result_cell_value(result: &QueryResult, row: usize, column: usize) -> Option<&str> {
    result.rows.get(row)?.get(column).map(String::as_str)
}
