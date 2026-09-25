use std::collections::{BTreeMap, HashMap, HashSet};

use crate::domain::database_object::{TableRelationType, TableSummary};
use crate::ui::DatabaseWorkspace;
use gpui_kit::base::{h_flex, v_flex};
use gpui_kit::component::{
    ActiveTheme as _, Disableable as _, Icon, IconName, Sizable as _,
    button::{Button, ButtonVariants as _},
    input::{Input, InputState},
    list::ListItem,
    menu::{DropdownMenu as _, PopupMenuItem},
    scroll::{ScrollableElement as _, ScrollbarHandle as _},
};
use gpui_kit::{
    Context, Entity, FontWeight, IntoElement, ParentElement as _, SharedString,
    StatefulInteractiveElement as _, Styled as _, UniformListScrollHandle, div, px, uniform_list,
};

pub struct ObjectExplorer {
    table_groups: Vec<SchemaTableGroup>,
    table_count: usize,
    schemas: std::rc::Rc<Vec<String>>,
    collapsed_schemas: HashSet<String>,
    category_expansion: HashMap<String, bool>,
    visible_rows: std::rc::Rc<Vec<ExplorerRow>>,
    scroll: UniformListScrollHandle,
}

struct SchemaTableGroup {
    schema: String,
    categories: Vec<TableCategory>,
}

struct TableCategory {
    label: &'static str,
    tables: Vec<TableSummary>,
}

enum ExplorerRow {
    Schema {
        name: String,
        collapsed: bool,
    },
    Category {
        schema: String,
        label: &'static str,
        count: usize,
        expanded: bool,
    },
    Table {
        schema: SharedString,
        name: SharedString,
        row_id: SharedString,
        tooltip_label: SharedString,
    },
}

impl ObjectExplorer {
    pub fn new(schemas: Vec<String>) -> Self {
        Self {
            table_groups: Vec::new(),
            table_count: 0,
            schemas: std::rc::Rc::new(schemas),
            collapsed_schemas: HashSet::new(),
            category_expansion: HashMap::new(),
            visible_rows: std::rc::Rc::new(Vec::new()),
            scroll: UniformListScrollHandle::new(),
        }
    }

    #[cfg(test)]
    pub(crate) fn scroll_handle(&self) -> &UniformListScrollHandle {
        &self.scroll
    }

    pub fn set_schemas(&mut self, schemas: Vec<String>) {
        let schemas = std::rc::Rc::new(schemas);
        self.schemas = schemas;
        self.collapsed_schemas
            .retain(|schema| self.schemas.contains(schema));
    }

    pub fn set_tables(&mut self, tables: Vec<TableSummary>, cx: &mut Context<DatabaseWorkspace>) {
        self.table_count = tables.len();
        self.table_groups = group_tables(tables);
        self.rebuild_visible_rows();
        self.scroll.set_offset(gpui_kit::point(px(0.), px(0.)));
        cx.notify();
    }

    pub fn toggle_schema(&mut self, schema: &str, cx: &mut Context<DatabaseWorkspace>) {
        if !self.collapsed_schemas.insert(schema.to_owned()) {
            self.collapsed_schemas.remove(schema);
        }
        self.rebuild_visible_rows();
        cx.notify();
    }

    pub fn toggle_category(
        &mut self,
        schema: &str,
        category: &str,
        default_expanded: bool,
        cx: &mut Context<DatabaseWorkspace>,
    ) {
        let key = format!("{schema}:{category}");
        let expanded = self
            .category_expansion
            .get(&key)
            .copied()
            .unwrap_or(default_expanded);
        self.category_expansion.insert(key, !expanded);
        self.rebuild_visible_rows();
        cx.notify();
    }

    fn rebuild_visible_rows(&mut self) {
        let mut rows = Vec::with_capacity(self.table_count + self.schemas.len());
        for schema_group in &self.table_groups {
            let schema = &schema_group.schema;
            let collapsed = self.collapsed_schemas.contains(schema);
            rows.push(ExplorerRow::Schema {
                name: schema.clone(),
                collapsed,
            });
            if collapsed {
                continue;
            }

            for category in &schema_group.categories {
                let category_key = format!("{schema}:{}", category.label);
                let expanded = self
                    .category_expansion
                    .get(&category_key)
                    .copied()
                    .unwrap_or(category.label == "Tables");
                rows.push(ExplorerRow::Category {
                    schema: schema.clone(),
                    label: category.label,
                    count: category.tables.len(),
                    expanded,
                });
                if expanded {
                    rows.extend(category.tables.iter().enumerate().map(|(index, table)| {
                        ExplorerRow::Table {
                            schema: SharedString::new(schema),
                            name: SharedString::new(&table.name),
                            row_id: SharedString::new(format!(
                                "table-row-{schema}-{}-{index}",
                                category.label
                            )),
                            tooltip_label: SharedString::new(format!("{schema}.{}", table.name)),
                        }
                    }));
                }
            }
        }
        self.visible_rows = std::rc::Rc::new(rows);
    }

    #[allow(clippy::too_many_arguments)]
    pub fn render(
        &self,
        cx: &mut Context<DatabaseWorkspace>,
        table_search: &Entity<InputState>,
        schema_filter: Option<&str>,
        type_filter: Option<TableRelationType>,
        table_loading: bool,
        table_offset: usize,
        table_has_next: bool,
        selected_table: Option<&(String, String)>,
        compact_layout: bool,
    ) -> impl IntoElement {
        let workspace = cx.entity();
        let filter_count =
            usize::from(schema_filter.is_some()) + usize::from(type_filter.is_some());
        v_flex()
            .w(px(276.))
            .h_full()
            .min_w(px(232.))
            .border_r_1()
            .border_color(cx.theme().border)
            .bg(cx.theme().background)
            .child(
                v_flex()
                    .gap_3()
                    .px_3()
                    .pt_5()
                    .pb_2()
                    .child(
                        h_flex()
                            .justify_between()
                            .items_center()
                            .child(
                                v_flex().gap_1().child(
                                    div()
                                        .text_base()
                                        .font_weight(FontWeight::BOLD)
                                        .child("Database Objects"),
                                ),
                            )
                            .child(
                                Button::new("refresh-database-objects")
                                    .ghost()
                                    .xsmall()
                                    .icon(Icon::new(gpui_kit::assets::IconName::RefreshCw))
                                    .tooltip("Refresh database objects")
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        this.refresh_database_objects(cx)
                                    })),
                            )
                            .child(
                                Button::new(if compact_layout {
                                    "show-table-workspace"
                                } else {
                                    "collapse-database-objects"
                                })
                                .ghost()
                                .xsmall()
                                .icon(Icon::new(if compact_layout {
                                    gpui_kit::assets::IconName::PanelRightClose
                                } else {
                                    gpui_kit::assets::IconName::PanelLeftClose
                                }))
                                .tooltip(if compact_layout {
                                    "Return to workspace"
                                } else {
                                    "Hide database objects"
                                })
                                .on_click(
                                    cx.listener(|this, _, _, cx| this.toggle_table_sidebar(cx)),
                                ),
                            ),
                    )
                    .child(
                        h_flex().child(
                            Input::new(table_search)
                                .flex_1()
                                .prefix(Icon::new(IconName::Search))
                                .suffix(
                                    h_flex()
                                        .items_center()
                                        .gap_1()
                                        .child(
                                            Button::new("apply-table-filter")
                                                .ghost()
                                                .xsmall()
                                                .icon(IconName::Search)
                                                .tooltip("Apply object search")
                                                .on_click(cx.listener(|this, _, _, cx| {
                                                    this.apply_table_filter(cx)
                                                })),
                                        )
                                        .child(filter_menu(
                                            filter_count,
                                            self.schemas.clone(),
                                            workspace.clone(),
                                        )),
                                ),
                        ),
                    )
                    .child(render_filter_chips(
                        schema_filter,
                        type_filter,
                        workspace.clone(),
                    )),
            )
            .child(if table_loading {
                div()
                    .flex_1()
                    .min_h(px(0.))
                    .overflow_y_scrollbar()
                    .px_2()
                    .child(render_loading(cx))
                    .into_any_element()
            } else {
                let content = div()
                    .flex_1()
                    .min_h(px(0.))
                    .px_2()
                    .child(self.render_grouped_tables(cx, workspace.clone(), selected_table));
                if self.table_count == 0 {
                    content.into_any_element()
                } else {
                    content.vertical_scrollbar(&self.scroll).into_any_element()
                }
            })
            .child(
                h_flex()
                    .justify_between()
                    .items_center()
                    .px_4()
                    .py_3()
                    .border_t_1()
                    .border_color(cx.theme().border)
                    .child(
                        div()
                            .text_xs()
                            .text_color(cx.theme().muted_foreground)
                            .child(if self.table_count == 0 {
                                "No matching tables".to_owned()
                            } else {
                                format!("{}–{}", table_offset + 1, table_offset + self.table_count)
                            }),
                    )
                    .child(
                        h_flex()
                            .gap_1()
                            .child(
                                Button::new("previous-table-page")
                                    .ghost()
                                    .xsmall()
                                    .label("Prev")
                                    .disabled(table_loading || table_offset == 0)
                                    .on_click(
                                        cx.listener(|this, _, _, cx| this.previous_table_page(cx)),
                                    ),
                            )
                            .child(
                                Button::new("next-table-page")
                                    .ghost()
                                    .xsmall()
                                    .label("Next")
                                    .disabled(table_loading || !table_has_next)
                                    .on_click(
                                        cx.listener(|this, _, _, cx| this.next_table_page(cx)),
                                    ),
                            ),
                    ),
            )
    }

    fn render_grouped_tables(
        &self,
        cx: &mut Context<DatabaseWorkspace>,
        workspace: Entity<DatabaseWorkspace>,
        selected_table: Option<&(String, String)>,
    ) -> gpui_kit::AnyElement {
        if self.table_count == 0 {
            return v_flex()
                .items_center()
                .gap_2()
                .py_12()
                .child(IconName::Folder)
                .child(div().text_sm().child("No tables match these filters."))
                .child(
                    div()
                        .text_xs()
                        .text_color(cx.theme().muted_foreground)
                        .child("Try clearing a filter or searching by name."),
                )
                .into_any_element();
        }
        let rows = self.visible_rows.clone();
        let workspace_for_rows = workspace.clone();
        let selected_table = selected_table.cloned();
        let scroll = &self.scroll;
        uniform_list(
            "database-object-rows",
            rows.len(),
            cx.processor(move |_, range: std::ops::Range<usize>, _window, cx| {
                range
                    .filter_map(|index| {
                        rows.get(index).map(|row| {
                            render_explorer_row(
                                row,
                                selected_table.as_ref(),
                                workspace_for_rows.clone(),
                                cx,
                            )
                        })
                    })
                    .collect::<Vec<_>>()
            }),
        )
        .size_full()
        .track_scroll(scroll)
        .into_any_element()
    }
}

fn render_explorer_row(
    row: &ExplorerRow,
    selected_table: Option<&(String, String)>,
    workspace: Entity<DatabaseWorkspace>,
    cx: &mut Context<DatabaseWorkspace>,
) -> gpui_kit::AnyElement {
    let item = match row {
        ExplorerRow::Schema { name, collapsed } => {
            let schema_for_toggle = name.clone();
            let workspace_for_toggle = workspace;
            ListItem::new(format!("schema-group-{name}"))
                .w_full()
                .h(px(28.))
                .rounded_md()
                .text_sm()
                .child(
                    h_flex()
                        .w_full()
                        .items_center()
                        .gap_2()
                        .child(Icon::new(if *collapsed {
                            IconName::ChevronRight
                        } else {
                            IconName::ChevronDown
                        }))
                        .child(Icon::new(gpui_kit::assets::IconName::Database))
                        .child(name.clone()),
                )
                .on_click(move |_, _, cx| {
                    workspace_for_toggle.update(cx, |this, cx| {
                        this.toggle_table_schema(&schema_for_toggle, cx)
                    })
                })
        }
        ExplorerRow::Category {
            schema,
            label,
            count,
            expanded,
        } => {
            let category_for_toggle = (*label).to_owned();
            let schema_for_toggle = schema.clone();
            let workspace_for_toggle = workspace;
            let default_expanded = *label == "Tables";
            let category_key = format!("{schema}:{label}");
            ListItem::new(format!("object-category-{category_key}"))
                .w_full()
                .h(px(28.))
                .rounded_md()
                .text_sm()
                .child(
                    h_flex()
                        .w_full()
                        .items_center()
                        .gap_2()
                        .pl_3()
                        .child(Icon::new(if *expanded {
                            IconName::ChevronDown
                        } else {
                            IconName::ChevronRight
                        }))
                        .child(Icon::new(gpui_kit::assets::IconName::Table))
                        .child(*label)
                        .child(
                            div()
                                .px_1()
                                .rounded_sm()
                                .bg(cx.theme().secondary)
                                .text_xs()
                                .child(count.to_string()),
                        ),
                )
                .on_click(move |_, _, cx| {
                    workspace_for_toggle.update(cx, |this, cx| {
                        this.toggle_table_category(
                            &schema_for_toggle,
                            &category_for_toggle,
                            default_expanded,
                            cx,
                        )
                    })
                })
        }
        ExplorerRow::Table {
            schema,
            name,
            row_id,
            tooltip_label,
        } => {
            let schema_for_open = schema.clone();
            let name_for_open = name.clone();
            let tooltip_label = tooltip_label.clone();
            let is_selected = selected_table.is_some_and(|(selected_schema, selected_name)| {
                selected_schema.as_str() == schema.as_str()
                    && selected_name.as_str() == name.as_str()
            });
            ListItem::new(row_id.clone())
                .w_full()
                .h(px(28.))
                .selected(is_selected)
                .rounded_md()
                .text_sm()
                .text_color(if is_selected {
                    cx.theme().sidebar_accent_foreground
                } else {
                    cx.theme().foreground
                })
                .child(
                    h_flex()
                        .w_full()
                        .items_center()
                        .gap_2()
                        .pl_12()
                        .child(Icon::new(gpui_kit::assets::IconName::Table))
                        .child(name.clone()),
                )
                .tooltip(move |window, cx| {
                    gpui_kit::component::tooltip::Tooltip::new(tooltip_label.clone())
                        .build(window, cx)
                })
                .on_click(move |_, window, cx| {
                    workspace.update(cx, |this, cx| {
                        this.preview_table(
                            schema_for_open.as_str(),
                            name_for_open.as_str(),
                            window,
                            cx,
                        )
                    })
                })
        }
    };
    h_flex()
        .w_full()
        .h(px(28.))
        .items_center()
        .child(item)
        .into_any_element()
}

fn group_tables(tables: Vec<TableSummary>) -> Vec<SchemaTableGroup> {
    let mut groups: BTreeMap<String, BTreeMap<&'static str, Vec<TableSummary>>> = BTreeMap::new();
    for table in tables {
        let category = match table.relation_type {
            TableRelationType::Table | TableRelationType::PartitionedTable => "Tables",
            TableRelationType::View | TableRelationType::MaterializedView => "Views",
            TableRelationType::ForeignTable => "Foreign tables",
        };
        groups
            .entry(table.schema.clone())
            .or_default()
            .entry(category)
            .or_default()
            .push(table);
    }
    groups
        .into_iter()
        .map(|(schema, categories)| SchemaTableGroup {
            schema,
            categories: categories
                .into_iter()
                .map(|(label, tables)| TableCategory { label, tables })
                .collect(),
        })
        .collect()
}

fn render_loading(cx: &mut Context<DatabaseWorkspace>) -> impl IntoElement {
    v_flex().gap_2().px_2().py_3().children((0..6).map(|_| {
        div()
            .h(px(28.))
            .w_full()
            .rounded_md()
            .bg(cx.theme().secondary)
    }))
}

fn filter_menu(
    filter_count: usize,
    schemas: std::rc::Rc<Vec<String>>,
    workspace: Entity<DatabaseWorkspace>,
) -> impl IntoElement {
    Button::new("table-filter-menu")
        .ghost()
        .xsmall()
        .icon(Icon::new(gpui_kit::assets::IconName::ListFilter))
        .tooltip(if filter_count == 0 {
            "Filter by schema or type"
        } else {
            "Schema or type filter active"
        })
        .dropdown_menu({
            move |menu, window, cx| {
                let menu = menu.item(PopupMenuItem::new("All schemas").on_click({
                    let workspace = workspace.clone();
                    move |_, _, cx| {
                        workspace.update(cx, |workspace, cx| {
                            workspace.set_table_schema_filter(None, cx)
                        })
                    }
                }));
                let schema_groups = schemas.iter().cloned().fold(
                    BTreeMap::<char, Vec<String>>::new(),
                    |mut groups, schema| {
                        let letter = schema
                            .chars()
                            .next()
                            .map(|letter| letter.to_ascii_uppercase())
                            .unwrap_or('#');
                        groups.entry(letter).or_default().push(schema);
                        groups
                    },
                );
                let menu = schema_groups
                    .into_iter()
                    .fold(menu, |menu, (letter, schemas)| {
                        let workspace = workspace.clone();
                        menu.submenu(
                            format!("Schemas: {letter}"),
                            window,
                            cx,
                            move |mut submenu, window, cx| {
                                if schemas.len() <= SCHEMA_MENU_PAGE_SIZE {
                                    return schemas.iter().cloned().fold(
                                        submenu,
                                        |submenu, schema| {
                                            submenu
                                                .item(schema_filter_item(schema, workspace.clone()))
                                        },
                                    );
                                }

                                for (page_index, page) in
                                    schemas.chunks(SCHEMA_MENU_PAGE_SIZE).enumerate()
                                {
                                    let page_start = page_index * SCHEMA_MENU_PAGE_SIZE + 1;
                                    let page_end = page_start + page.len() - 1;
                                    let page_schemas = page.to_vec();
                                    let page_workspace = workspace.clone();
                                    submenu = submenu.submenu(
                                        format!("{letter} · {page_start}–{page_end}"),
                                        window,
                                        cx,
                                        move |page_menu, _, _| {
                                            page_schemas.iter().cloned().fold(
                                                page_menu,
                                                |page_menu, schema| {
                                                    page_menu.item(schema_filter_item(
                                                        schema,
                                                        page_workspace.clone(),
                                                    ))
                                                },
                                            )
                                        },
                                    );
                                }
                                submenu
                            },
                        )
                    });
                menu.item(PopupMenuItem::new("All types").on_click({
                    let workspace = workspace.clone();
                    move |_, _, cx| {
                        workspace.update(cx, |workspace, cx| {
                            workspace.set_table_type_filter(None, cx)
                        })
                    }
                }))
                .item(PopupMenuItem::new("Type: Tables").on_click({
                    let workspace = workspace.clone();
                    move |_, _, cx| {
                        workspace.update(cx, |workspace, cx| {
                            workspace.set_table_type_filter(Some(TableRelationType::Table), cx)
                        })
                    }
                }))
                .item(PopupMenuItem::new("Type: Views").on_click({
                    let workspace = workspace.clone();
                    move |_, _, cx| {
                        workspace.update(cx, |workspace, cx| {
                            workspace.set_table_type_filter(Some(TableRelationType::View), cx)
                        })
                    }
                }))
                .item(PopupMenuItem::new("Type: Partitioned tables").on_click({
                    let workspace = workspace.clone();
                    move |_, _, cx| {
                        workspace.update(cx, |workspace, cx| {
                            workspace.set_table_type_filter(
                                Some(TableRelationType::PartitionedTable),
                                cx,
                            )
                        })
                    }
                }))
                .item(PopupMenuItem::new("Type: Materialized views").on_click({
                    let workspace = workspace.clone();
                    move |_, _, cx| {
                        workspace.update(cx, |workspace, cx| {
                            workspace.set_table_type_filter(
                                Some(TableRelationType::MaterializedView),
                                cx,
                            )
                        })
                    }
                }))
                .item(PopupMenuItem::new("Type: Foreign tables").on_click({
                    let workspace = workspace.clone();
                    move |_, _, cx| {
                        workspace.update(cx, |workspace, cx| {
                            workspace
                                .set_table_type_filter(Some(TableRelationType::ForeignTable), cx)
                        })
                    }
                }))
            }
        })
}

const SCHEMA_MENU_PAGE_SIZE: usize = 32;

fn schema_filter_item(schema: String, workspace: Entity<DatabaseWorkspace>) -> PopupMenuItem {
    PopupMenuItem::new(schema.clone()).on_click(move |_, _, cx| {
        workspace.update(cx, |workspace, cx| {
            workspace.set_table_schema_filter(Some(schema.clone()), cx)
        })
    })
}

fn render_filter_chips(
    schema: Option<&str>,
    relation_type: Option<TableRelationType>,
    workspace: Entity<DatabaseWorkspace>,
) -> gpui_kit::AnyElement {
    if schema.is_none() && relation_type.is_none() {
        return div().into_any_element();
    }
    h_flex()
        .gap_1()
        .flex_wrap()
        .children(
            [
                schema.map(|schema| {
                    let workspace = workspace.clone();
                    Button::new("schema-filter-chip")
                        .outline()
                        .xsmall()
                        .label(format!("Schema: {schema} ×"))
                        .on_click(move |_, _, cx| {
                            workspace.update(cx, |workspace, cx| {
                                workspace.set_table_schema_filter(None, cx)
                            })
                        })
                        .into_any_element()
                }),
                relation_type.map(|relation_type| {
                    let workspace = workspace.clone();
                    Button::new("type-filter-chip")
                        .outline()
                        .xsmall()
                        .label(format!("Type: {} ×", relation_type.label()))
                        .on_click(move |_, _, cx| {
                            workspace.update(cx, |workspace, cx| {
                                workspace.set_table_type_filter(None, cx)
                            })
                        })
                        .into_any_element()
                }),
            ]
            .into_iter()
            .flatten(),
        )
        .child(if schema.is_some() || relation_type.is_some() {
            Button::new("clear-table-filters")
                .ghost()
                .xsmall()
                .label("Clear")
                .on_click(move |_, _, cx| {
                    workspace.update(cx, |workspace, cx| {
                        workspace.set_table_schema_filter(None, cx);
                        workspace.set_table_type_filter(None, cx);
                    })
                })
                .into_any_element()
        } else {
            div().into_any_element()
        })
        .into_any_element()
}

#[cfg(test)]
#[path = "../../tests/unit/ui/object_explorer.rs"]
mod tests;
