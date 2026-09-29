use super::*;
mod row;

use row::render_result_row;
pub(crate) use row::table_column_width;
#[cfg(test)]
pub(crate) use row::{
    flatten_cell_preview, is_inline_editable_cell, is_mono_column_type, result_cell_value,
    visible_cell_prefix,
};

use crate::ui::database_workspace::MoveDirection;
use gpui_kit::component::menu::{ContextMenuExt, PopupMenu};
use gpui_kit::{
    AnyElement, AppContext, ClipboardItem, DragMoveEvent, ElementId, Render, SharedString, Window,
};

#[derive(Clone)]
struct ColumnDragGhost {
    column: String,
}

impl Render for ColumnDragGhost {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .px_3()
            .py_1()
            .rounded_md()
            .bg(cx.theme().primary)
            .text_color(cx.theme().primary_foreground)
            .text_sm()
            .font_weight(FontWeight::SEMIBOLD)
            .shadow_md()
            .child(self.column.clone())
    }
}

#[derive(Clone)]
struct EmptyDragPreview;

impl Render for EmptyDragPreview {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        div()
    }
}

#[derive(Clone)]
struct ResizeColumnDrag {
    schema: String,
    table: String,
    column: String,
    initial_width: f32,
    start_x: Rc<std::cell::Cell<Option<f32>>>,
}

#[allow(clippy::too_many_arguments)]
fn build_column_menu(
    menu: PopupMenu,
    workspace: Entity<DatabaseWorkspace>,
    schema: String,
    table: String,
    column_name: String,
    is_pinned: bool,
    col_pos: usize,
    total_columns: usize,
) -> PopupMenu {
    let pin_workspace = workspace.clone();
    let pin_schema = schema.clone();
    let pin_table = table.clone();
    let pin_column = column_name.clone();
    let pin_label = if is_pinned {
        "Unpin column"
    } else {
        "Pin column"
    };

    let autofit_workspace = workspace.clone();
    let autofit_schema = schema.clone();
    let autofit_table = table.clone();
    let autofit_column = column_name.clone();

    let move_left_workspace = workspace.clone();
    let move_left_schema = schema.clone();
    let move_left_table = table.clone();
    let move_left_column = column_name.clone();

    let move_right_workspace = workspace.clone();
    let move_right_schema = schema.clone();
    let move_right_table = table.clone();
    let move_right_column = column_name.clone();

    let hide_workspace = workspace.clone();
    let hide_column = column_name.clone();

    let copy_column = column_name;

    menu.item(PopupMenuItem::new(pin_label).on_click(move |_, _, cx| {
        pin_workspace.update(cx, |this, _| {
            this.toggle_pin_column(&pin_schema, &pin_table, &pin_column);
        });
    }))
    .item(
        PopupMenuItem::new("Auto-fit width").on_click(move |_, _, cx| {
            autofit_workspace.update(cx, |this, _| {
                this.autofit_column_width(&autofit_schema, &autofit_table, &autofit_column);
            });
        }),
    )
    .item(PopupMenuItem::separator())
    .item(
        PopupMenuItem::new("Move Left")
            .disabled(col_pos == 0)
            .on_click(move |_, _, cx| {
                move_left_workspace.update(cx, |this, _| {
                    this.move_column(
                        &move_left_schema,
                        &move_left_table,
                        &move_left_column,
                        MoveDirection::Left,
                    );
                });
            }),
    )
    .item(
        PopupMenuItem::new("Move Right")
            .disabled(col_pos + 1 >= total_columns)
            .on_click(move |_, _, cx| {
                move_right_workspace.update(cx, |this, _| {
                    this.move_column(
                        &move_right_schema,
                        &move_right_table,
                        &move_right_column,
                        MoveDirection::Right,
                    );
                });
            }),
    )
    .item(PopupMenuItem::separator())
    .item(PopupMenuItem::new("Hide Column").on_click(move |_, _, cx| {
        hide_workspace.update(cx, |this, cx| {
            this.hide_table_data_column(&hide_column, cx);
        });
    }))
    .item(
        PopupMenuItem::new("Copy Column Name").on_click(move |_, _, cx| {
            cx.write_to_clipboard(ClipboardItem::new_string(copy_column.clone()));
        }),
    )
}

#[allow(clippy::too_many_arguments)]
fn render_header_cell(
    index: usize,
    result: &QueryResult,
    column_widths: &[f32],
    schema: &str,
    table: &str,
    is_pinned: bool,
    col_pos: usize,
    total_columns: usize,
    table_data_sort: Option<&(String, bool)>,
    table_column_filters: &[TableColumnFilter],
    table_filter_editor: Option<&(Option<String>, TableFilterOperator, Entity<InputState>)>,
    workspace: &Entity<DatabaseWorkspace>,
    cx: &mut Context<DatabaseWorkspace>,
) -> Option<AnyElement> {
    let column = result.columns.get(index)?;
    let column_type = result
        .column_types
        .get(index)
        .cloned()
        .unwrap_or_else(|| "unknown".to_owned());
    let sortable = result.editable.is_some();
    let sort_direction = if sortable {
        table_data_sort.and_then(|(sorted_column, descending)| {
            (sorted_column == column).then_some(*descending)
        })
    } else {
        None
    };
    let is_sorted = sort_direction.is_some();
    let column_width = column_widths.get(index).copied().unwrap_or(120.0);
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
        .accessibility_label(if filter_is_active {
            format!("Edit filter for {column}")
        } else {
            format!("Filter {column}")
        })
        .tooltip(if filter_is_active {
            format!("Edit filter for {column}")
        } else {
            format!("Filter {column}")
        });
    let filter_control = if result.editable.is_some() {
        let content_filter_editor = table_filter_editor.cloned();
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
        div().w(px(0.)).into_any_element()
    };
    let sort_accessibility_label = match (sortable, sort_direction) {
        (false, _) => {
            format!("{column_name}, {column_type}. Sorting is unavailable for query results.")
        }
        (true, Some(true)) => {
            format!("Sort by {column_name}, currently descending. Activate to sort ascending.")
        }
        (true, Some(false)) => {
            format!("Sort by {column_name}, currently ascending. Activate to sort descending.")
        }
        (true, None) => format!("Sort by {column_name} ascending."),
    };
    let sort_tooltip = if sortable {
        match sort_direction {
            Some(true) => format!("{column_name} · {column_type} · Sorted descending"),
            Some(false) => format!("{column_name} · {column_type} · Sorted ascending"),
            None => format!("{column_name} · {column_type} · Sort ascending"),
        }
    } else {
        format!("{column_name} · {column_type} · Sort with SQL ORDER BY")
    };
    let sort_indicator = match sort_direction {
        Some(true) => "↓",
        Some(false) => "↑",
        None => "",
    };
    let column_label = v_flex()
        .w_full()
        .min_w(px(0.))
        .gap_0()
        .child(
            div()
                .w_full()
                .min_w(px(0.))
                .text_sm()
                .font_weight(if is_sorted {
                    FontWeight::SEMIBOLD
                } else {
                    FontWeight::MEDIUM
                })
                .whitespace_nowrap()
                .text_ellipsis()
                .child(column_name.clone()),
        )
        .child(
            h_flex()
                .w_full()
                .min_w(px(0.))
                .items_center()
                .justify_between()
                .gap_1()
                .child(
                    div()
                        .flex_1()
                        .min_w(px(0.))
                        .text_xs()
                        .text_color(cx.theme().muted_foreground)
                        .whitespace_nowrap()
                        .text_ellipsis()
                        .child(column_type.clone()),
                )
                .child(if is_sorted {
                    div()
                        .flex_shrink_0()
                        .text_xs()
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_color(cx.theme().primary)
                        .child(sort_indicator)
                        .into_any_element()
                } else {
                    div().into_any_element()
                }),
        );
    let sort_control = if sortable {
        Button::new(format!("sort-column-{index}"))
            .ghost()
            .flex_1()
            .min_w(px(0.))
            .small()
            .h(px(56.))
            .justify_start()
            .px_1()
            .accessibility_label(sort_accessibility_label)
            .tooltip(sort_tooltip)
            .child(column_label)
            .on_click(move |_, window, cx| {
                sort_workspace.update(cx, |this, cx| {
                    this.set_table_data_sort(column_name.clone(), window, cx)
                })
            })
            .into_any_element()
    } else {
        div()
            .flex_1()
            .min_w(px(0.))
            .h(px(56.))
            .px_1()
            .items_center()
            .child(column_label)
            .into_any_element()
    };

    let menu_workspace = workspace.clone();
    let menu_schema = schema.to_string();
    let menu_table = table.to_string();
    let menu_column = column.clone();
    let column_menu_btn = Button::new(format!("column-menu-{index}"))
        .ghost()
        .xsmall()
        .icon(Icon::new(gpui_kit::assets::IconName::Ellipsis))
        .tooltip("Column options")
        .dropdown_menu({
            let ws = menu_workspace;
            let s = menu_schema;
            let t = menu_table;
            let c = menu_column;
            move |menu, _, _| {
                build_column_menu(
                    menu,
                    ws.clone(),
                    s.clone(),
                    t.clone(),
                    c.clone(),
                    is_pinned,
                    col_pos,
                    total_columns,
                )
            }
        });

    let start_x = Rc::new(std::cell::Cell::new(None));
    let resize_drag = ResizeColumnDrag {
        schema: schema.to_string(),
        table: table.to_string(),
        column: column.clone(),
        initial_width: column_width,
        start_x: start_x.clone(),
    };
    let resize_workspace = workspace.clone();
    let autofit_workspace = workspace.clone();
    let autofit_schema = schema.to_string();
    let autofit_table = table.to_string();
    let autofit_col = column.clone();
    let resize_handle = div()
        .id(ElementId::from(SharedString::from(format!(
            "col-resize-{index}"
        ))))
        .absolute()
        .top_0()
        .bottom_0()
        .right(px(-3.))
        .w(px(7.))
        .cursor_col_resize()
        .on_drag(resize_drag, |_, _, _, cx| cx.new(|_| EmptyDragPreview))
        .on_drag_move(move |event: &DragMoveEvent<ResizeColumnDrag>, _, cx| {
            let (schema, table, column, new_width) = {
                let drag = event.drag(cx);
                let current_x = event.event.position.x.as_f32();
                let start = match drag.start_x.get() {
                    Some(s) => s,
                    None => {
                        drag.start_x.set(Some(current_x));
                        current_x
                    }
                };
                let delta = current_x - start;
                let new_width = (drag.initial_width + delta).clamp(80.0, 500.0);
                (
                    drag.schema.clone(),
                    drag.table.clone(),
                    drag.column.clone(),
                    new_width,
                )
            };
            resize_workspace.update(cx, |this, _| {
                this.set_column_width(&schema, &table, &column, new_width);
            });
        })
        .on_click(move |event, _, cx| {
            if event.click_count() >= 2 {
                autofit_workspace.update(cx, |this, _| {
                    this.autofit_column_width(&autofit_schema, &autofit_table, &autofit_col);
                });
            }
        });

    let reorder_workspace = workspace.clone();
    let reorder_schema = schema.to_string();
    let reorder_table = table.to_string();
    let reorder_col_name = column.clone();
    let right_click_ws = workspace.clone();
    let right_click_schema = schema.to_string();
    let right_click_table = table.to_string();
    let right_click_col = column.clone();

    let cell = h_flex()
        .id(ElementId::from(SharedString::from(format!(
            "header-cell-{index}"
        ))))
        .relative()
        .w(px(column_width))
        .h(px(56.))
        .flex_shrink_0()
        .items_center()
        .gap_0()
        .on_drag(
            ColumnDragGhost {
                column: column.clone(),
            },
            |ghost, _, _, cx| cx.new(|_| ghost.clone()),
        )
        .drag_over::<ColumnDragGhost>(|style, _, _, cx| {
            style.border_l(px(2.)).border_color(cx.theme().primary)
        })
        .on_drop(move |ghost: &ColumnDragGhost, _, cx| {
            if ghost.column != reorder_col_name {
                reorder_workspace.update(cx, |this, _| {
                    this.reorder_column(&reorder_schema, &reorder_table, &ghost.column, col_pos);
                });
            }
        })
        .child(sort_control)
        .child(
            h_flex()
                .items_center()
                .gap_0()
                .child(filter_control)
                .child(column_menu_btn),
        )
        .child(resize_handle)
        .context_menu(move |menu, _, _| {
            build_column_menu(
                menu,
                right_click_ws.clone(),
                right_click_schema.clone(),
                right_click_table.clone(),
                right_click_col.clone(),
                is_pinned,
                col_pos,
                total_columns,
            )
        });

    Some(cell.into_any_element())
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn render_result(
    cx: &mut Context<DatabaseWorkspace>,
    result: Option<&QueryResult>,
    column_widths: Rc<Vec<f32>>,
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
    table_preview_loading: bool,
    table_preview_error: Option<&str>,
    selected_table: Option<&(String, String)>,
    pinned_columns: &std::collections::HashSet<String>,
    custom_column_order: Option<&[String]>,
) -> impl IntoElement {
    let workspace = cx.entity();
    let Some(result) = result else {
        if table_preview_loading {
            let label = if let Some((schema, table)) = selected_table {
                format!("Loading {schema}.{table}…")
            } else {
                "Loading table preview…".to_string()
            };
            return v_flex()
                .size_full()
                .items_center()
                .justify_center()
                .gap_3()
                .child(
                    div()
                        .w(px(24.))
                        .h(px(24.))
                        .child(Icon::new(gpui_kit::assets::IconName::RefreshCw)),
                )
                .child(
                    div()
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_base()
                        .child(label),
                )
                .child(
                    div()
                        .text_xs()
                        .text_color(cx.theme().muted_foreground)
                        .child("Fetching table columns and rows from PostgreSQL…"),
                )
                .into_any_element();
        }

        if let Some(error) = table_preview_error {
            let retry_workspace = workspace.clone();
            return v_flex()
                .size_full()
                .items_center()
                .justify_center()
                .gap_3()
                .p_6()
                .child(
                    v_flex()
                        .w(px(460.))
                        .p_5()
                        .rounded_lg()
                        .bg(cx.theme().secondary)
                        .border_1()
                        .border_color(cx.theme().border)
                        .gap_3()
                        .items_center()
                        .child(
                            div()
                                .text_base()
                                .font_weight(FontWeight::SEMIBOLD)
                                .text_color(cx.theme().foreground)
                                .child("Could not load table"),
                        )
                        .child(
                            div()
                                .text_sm()
                                .text_color(cx.theme().muted_foreground)
                                .child(error.to_string()),
                        )
                        .child(
                            Button::new("retry-table-preview")
                                .primary()
                                .small()
                                .icon(Icon::new(gpui_kit::assets::IconName::RefreshCw))
                                .label("Retry")
                                .on_click(move |_, window, cx| {
                                    retry_workspace.update(cx, |this, cx| {
                                        this.retry_preview_table(window, cx);
                                    });
                                }),
                        ),
                )
                .into_any_element();
        }

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
    #[cfg(test)]
    let eager_cell_tooltips = EAGER_CELL_TOOLTIPS_FOR_BENCHMARK.load(Ordering::Relaxed);
    #[cfg(not(test))]
    let eager_cell_tooltips = false;
    let (current_schema, current_table) = if let Some((schema, table)) = selected_table {
        (schema.clone(), table.clone())
    } else if let Some(editable) = result.editable.as_ref() {
        (editable.schema.clone(), editable.table.clone())
    } else {
        (String::new(), String::new())
    };

    let mut ordered_indices = Vec::with_capacity(result.columns.len());
    let mut seen_indices = std::collections::HashSet::new();
    if let Some(custom_order) = custom_column_order {
        for name in custom_order {
            if let Some(idx) = result
                .columns
                .iter()
                .position(|c| c == name)
                .filter(|idx| seen_indices.insert(*idx))
            {
                ordered_indices.push(idx);
            }
        }
    }
    for idx in 0..result.columns.len() {
        if seen_indices.insert(idx) {
            ordered_indices.push(idx);
        }
    }

    let mut pinned_indices = Vec::new();
    let mut scrollable_indices = Vec::new();
    for &idx in &ordered_indices {
        let col_name = &result.columns[idx];
        if hidden_columns.contains(col_name) {
            continue;
        }
        if pinned_columns.contains(col_name) {
            pinned_indices.push(idx);
        } else {
            scrollable_indices.push(idx);
        }
    }

    let pinned_gutter_width = if workspace_layout { 40.0 } else { 0.0 };
    let total_pinned_width = pinned_gutter_width
        + pinned_indices
            .iter()
            .map(|&idx| column_widths.get(idx).copied().unwrap_or(120.0))
            .sum::<f32>();

    let scroll_offset_x = horizontal_scroll
        .filter(|_| workspace_layout)
        .map(|s| (-s.offset().x.as_f32()).max(0.0))
        .unwrap_or(0.0);

    let (visible_scrollable_indices, leading_column_width, trailing_column_width) =
        visible_column_window_with_pinned(
            &scrollable_indices,
            &column_widths,
            total_pinned_width,
            workspace_layout,
            horizontal_scroll,
        );

    let total_columns = result.columns.len();

    let scroll_shift = (leading_column_width - scroll_offset_x).min(0.0);
    let mut scrollable_headers = h_flex()
        .h_full()
        .items_center()
        .gap_0()
        .ml(px(scroll_shift));
    scrollable_headers =
        scrollable_headers.children(visible_scrollable_indices.iter().enumerate().filter_map(
            |(col_pos, &index)| {
                render_header_cell(
                    index,
                    result,
                    &column_widths,
                    &current_schema,
                    &current_table,
                    false,
                    pinned_indices.len() + col_pos,
                    total_columns,
                    table_data_sort,
                    table_column_filters,
                    table_filter_editor.as_ref(),
                    &workspace,
                    cx,
                )
            },
        ));
    if trailing_column_width > 0. {
        scrollable_headers =
            scrollable_headers.child(div().w(px(trailing_column_width)).flex_shrink_0());
    }
    if result.editable.is_some() && !workspace_layout {
        scrollable_headers = scrollable_headers.child(
            div()
                .w(px(112.))
                .text_xs()
                .text_color(cx.theme().muted_foreground)
                .child("Row SQL"),
        );
    }

    let mut header = h_flex()
        .relative()
        .w_full()
        .gap_0()
        .px_2()
        .h(px(56.))
        .flex_shrink_0()
        .border_b_1()
        .border_color(cx.theme().border)
        .bg(cx.theme().table_head)
        .overflow_hidden();

    if total_pinned_width > 0. {
        let mut pinned_headers = h_flex()
            .w(px(total_pinned_width))
            .h_full()
            .flex_shrink_0()
            .items_center()
            .gap_0()
            .bg(cx.theme().table_head)
            .border_r_1()
            .border_color(cx.theme().border);

        if workspace_layout {
            pinned_headers = pinned_headers.child(
                h_flex()
                    .w(px(40.))
                    .h_full()
                    .flex_shrink_0()
                    .items_center()
                    .text_xs()
                    .text_color(cx.theme().muted_foreground)
                    .child("#"),
            );
        }

        pinned_headers = pinned_headers.children(pinned_indices.iter().enumerate().filter_map(
            |(col_pos, &index)| {
                render_header_cell(
                    index,
                    result,
                    &column_widths,
                    &current_schema,
                    &current_table,
                    true,
                    col_pos,
                    total_columns,
                    table_data_sort,
                    table_column_filters,
                    table_filter_editor.as_ref(),
                    &workspace,
                    cx,
                )
            },
        ));

        header = header.child(pinned_headers);
    }

    header = header.child(
        div()
            .flex_1()
            .h_full()
            .overflow_hidden()
            .child(scrollable_headers),
    );

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
            let column_widths = column_widths.clone();
            let pinned_indices = Rc::new(pinned_indices);
            let visible_scrollable_indices = Rc::new(visible_scrollable_indices);
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
                                &pinned_indices,
                                &visible_scrollable_indices,
                                total_pinned_width,
                                leading_column_width,
                                trailing_column_width,
                                scroll_offset_x,
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
                        &pinned_indices,
                        &visible_scrollable_indices,
                        total_pinned_width,
                        leading_column_width,
                        trailing_column_width,
                        scroll_offset_x,
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
                    .dropdown_menu({
                        let reset_workspace = columns_workspace.clone();
                        let s = current_schema.clone();
                        let t = current_table.clone();
                        move |menu, _, _| {
                            let menu = columns_to_toggle.iter().enumerate().fold(
                                menu,
                                |menu, (index, column)| {
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
                                },
                            );
                            let r_ws = reset_workspace.clone();
                            let r_s = s.clone();
                            let r_t = t.clone();
                            menu.item(PopupMenuItem::separator()).item(
                                PopupMenuItem::new("Reset Column Layout").on_click(
                                    move |_, _, cx| {
                                        r_ws.update(cx, |this, _| {
                                            this.reset_column_layout(&r_s, &r_t);
                                        });
                                    },
                                ),
                            )
                        }
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
                let total_scrollable_width: f32 = scrollable_indices
                    .iter()
                    .map(|&idx| column_widths.get(idx).copied().unwrap_or(120.0))
                    .sum();

                let shadow_overlay =
                    (total_pinned_width > 0. && scroll_offset_x > 0.0).then(|| {
                        div()
                            .absolute()
                            .top_0()
                            .bottom_0()
                            .left(px(total_pinned_width))
                            .w(px(1.))
                            .shadow_md()
                    });

                let horizontal_bar = if workspace_layout && total_scrollable_width > 0. {
                    let scroll = horizontal_scroll
                        .expect("workspace results require a horizontal scroll handle");
                    Some(
                        div()
                            .id("query-result-horizontal-scroll")
                            .absolute()
                            .bottom_0()
                            .left(px(total_pinned_width))
                            .right_0()
                            .h(px(12.))
                            .overflow_x_scroll()
                            .track_scroll(scroll)
                            .horizontal_scrollbar(scroll)
                            .child(div().w(px(total_scrollable_width)).h(px(1.))),
                    )
                } else {
                    None
                };

                let grid = v_flex()
                    .w_full()
                    .h_full()
                    .border_b_1()
                    .border_color(cx.theme().border)
                    .child(header)
                    .child(if workspace_layout {
                        let result_scroll_handle = result_scroll
                            .expect("workspace results require a uniform scroll handle");
                        div()
                            .flex_1()
                            .min_h(px(0.))
                            .vertical_scrollbar(result_scroll_handle)
                            .child(rows)
                            .into_any_element()
                    } else {
                        div()
                            .flex_1()
                            .min_h(px(0.))
                            .overflow_y_scrollbar()
                            .child(rows)
                            .into_any_element()
                    });

                if workspace_layout {
                    let ws = workspace.clone();
                    div()
                        .id("query-result-grid-container")
                        .relative()
                        .flex_1()
                        .min_h(px(0.))
                        .overflow_hidden()
                        .child(grid)
                        .children(shadow_overlay)
                        .children(horizontal_bar)
                        .on_scroll_wheel(move |event: &gpui_kit::ScrollWheelEvent, window, cx| {
                            let line_height = window.line_height();
                            let delta = event.delta.pixel_delta(line_height);
                            let shift = event.modifiers.shift;

                            let raw_dx = delta.x.as_f32();
                            let raw_dy = delta.y.as_f32();
                            let dx = if shift
                                && raw_dx.abs() < f32::EPSILON
                                && raw_dy.abs() > f32::EPSILON
                            {
                                raw_dy
                            } else {
                                raw_dx
                            };

                            if dx.abs() > f32::EPSILON {
                                ws.update(cx, |this, _| {
                                    this.scroll_horizontal_by(dx);
                                });
                            }
                        })
                        .on_key_down({
                            let ws = workspace.clone();
                            move |event, _, cx| {
                                let cmd_or_ctrl = event.keystroke.modifiers.platform
                                    || event.keystroke.modifiers.control;
                                match event.keystroke.key.as_str() {
                                    "pageup" => {
                                        ws.update(cx, |this, _| {
                                            this.scroll_page_up();
                                        });
                                    }
                                    "pagedown" => {
                                        ws.update(cx, |this, _| {
                                            this.scroll_page_down();
                                        });
                                    }
                                    "home" if cmd_or_ctrl => {
                                        ws.update(cx, |this, _| {
                                            this.scroll_to_first_column();
                                        });
                                    }
                                    "home" => {
                                        ws.update(cx, |this, _| {
                                            this.scroll_to_top();
                                        });
                                    }
                                    "end" if cmd_or_ctrl => {
                                        ws.update(cx, |this, _| {
                                            this.scroll_to_last_column();
                                        });
                                    }
                                    "end" => {
                                        ws.update(cx, |this, _| {
                                            this.scroll_to_bottom();
                                        });
                                    }
                                    _ => {}
                                }
                            }
                        })
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

#[allow(dead_code)]
pub(crate) fn result_column_widths(result: &QueryResult) -> Rc<Vec<f32>> {
    Rc::new(
        result
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
            .collect(),
    )
}

pub(crate) fn visible_column_window_with_pinned(
    scrollable_indices: &[usize],
    column_widths: &[f32],
    total_pinned_width: f32,
    workspace_layout: bool,
    scroll: Option<&gpui_kit::ScrollHandle>,
) -> (Vec<usize>, f32, f32) {
    let Some(scroll) = scroll.filter(|_| workspace_layout) else {
        return (scrollable_indices.to_vec(), 0., 0.);
    };
    let viewport_width = scroll.bounds().size.width.as_f32();
    if viewport_width <= 0. {
        return (scrollable_indices.to_vec(), 0., 0.);
    }

    let viewport_left = (-scroll.offset().x.as_f32()).max(0.);
    let viewport_right = viewport_left + viewport_width;
    const COLUMN_OVERSCAN_PX: f32 = 250.0;
    let buffered_viewport_left = (viewport_left - COLUMN_OVERSCAN_PX).max(0.);
    let buffered_viewport_right = viewport_right + COLUMN_OVERSCAN_PX;
    let mut column_left = total_pinned_width;
    let mut first_visible_left = None;
    let mut visible_columns = Vec::new();
    let mut trailing_width = 0.;

    for &index in scrollable_indices {
        let width = column_widths.get(index).copied().unwrap_or(120.0);
        let column_right = column_left + width;
        if column_right > buffered_viewport_left && column_left < buffered_viewport_right {
            first_visible_left.get_or_insert(column_left);
            visible_columns.push(index);
        } else if first_visible_left.is_some() && column_left >= buffered_viewport_right {
            trailing_width += width;
        }
        column_left = column_right;
    }

    let Some(first_visible_left) = first_visible_left else {
        return (scrollable_indices.to_vec(), 0., 0.);
    };
    (
        visible_columns,
        first_visible_left - total_pinned_width,
        trailing_width,
    )
}

#[allow(dead_code)]
pub(crate) fn visible_column_window(
    columns: &[String],
    column_widths: &[f32],
    hidden_columns: &std::collections::HashSet<String>,
    workspace_layout: bool,
    scroll: Option<&gpui_kit::ScrollHandle>,
) -> (Vec<usize>, f32, f32) {
    let unpinned_indices: Vec<usize> = columns
        .iter()
        .enumerate()
        .filter_map(|(index, name)| (!hidden_columns.contains(name)).then_some(index))
        .collect();
    let total_pinned_width = if workspace_layout { 40. } else { 0. };
    visible_column_window_with_pinned(
        &unpinned_indices,
        column_widths,
        total_pinned_width,
        workspace_layout,
        scroll,
    )
}
