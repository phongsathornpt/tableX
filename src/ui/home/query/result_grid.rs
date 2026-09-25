use super::*;
mod row;

use row::render_result_row;
pub(in crate::ui::home::query) use row::table_column_width;
#[cfg(test)]
pub(in crate::ui::home::query) use row::{
    is_inline_editable_cell, result_cell_value, visible_cell_prefix,
};

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
        .flex_shrink_0()
        .border_b_1()
        .border_color(cx.theme().border)
        .bg(cx.theme().table_head);
    if workspace_layout {
        header = header.child(
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
        let sortable = result.editable.is_some();
        let sort_direction = if sortable {
            table_data_sort.and_then(|(sorted_column, descending)| {
                (sorted_column == column).then_some(*descending)
            })
        } else {
            None
        };
        let is_sorted = sort_direction.is_some();
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
        Some(
            h_flex()
                .w(px(column_width))
                .h(px(56.))
                .flex_shrink_0()
                .items_center()
                .gap_0()
                .child(sort_control)
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
            let column_widths = column_widths.clone();
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

pub(crate) fn visible_column_window(
    columns: &[String],
    column_widths: &[f32],
    hidden_columns: &std::collections::HashSet<String>,
    workspace_layout: bool,
    scroll: Option<&gpui_kit::ScrollHandle>,
) -> (Vec<usize>, f32, f32) {
    let Some(scroll) = scroll.filter(|_| workspace_layout) else {
        return (
            columns
                .iter()
                .enumerate()
                .filter_map(|(index, name)| (!hidden_columns.contains(name)).then_some(index))
                .collect(),
            0.,
            0.,
        );
    };
    let viewport_width = scroll.bounds().size.width.as_f32();
    if viewport_width <= 0. {
        return (
            columns
                .iter()
                .enumerate()
                .filter_map(|(index, name)| (!hidden_columns.contains(name)).then_some(index))
                .collect(),
            0.,
            0.,
        );
    }

    let viewport_left = (-scroll.offset().x.as_f32()).max(0.);
    let viewport_right = viewport_left + viewport_width;
    let content_left = if workspace_layout { 40. } else { 0. };
    let mut column_left = content_left;
    let mut first_visible_left = None;
    let mut visible_columns = Vec::new();
    let mut trailing_width = 0.;

    for (index, name) in columns.iter().enumerate() {
        if hidden_columns.contains(name) {
            continue;
        }
        let width = column_widths[index];
        let column_right = column_left + width;
        if column_right > viewport_left && column_left < viewport_right {
            first_visible_left.get_or_insert(column_left);
            visible_columns.push(index);
        } else if first_visible_left.is_some() && column_left >= viewport_right {
            trailing_width += width;
        }
        column_left = column_right;
    }

    let Some(first_visible_left) = first_visible_left else {
        return (
            columns
                .iter()
                .enumerate()
                .filter_map(|(index, name)| (!hidden_columns.contains(name)).then_some(index))
                .collect(),
            0.,
            0.,
        );
    };
    (
        visible_columns,
        first_visible_left - content_left,
        trailing_width,
    )
}
