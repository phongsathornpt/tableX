use gpui_kit::base::{Selectable as _, h_flex};
use gpui_kit::component::{
    ActiveTheme as _, Disableable as _, Icon, IconName, Sizable as _,
    button::{Button, ButtonVariants as _},
    input::{Input, InputState},
    menu::{DropdownMenu as _, PopupMenuItem},
};
use gpui_kit::{Context, FontWeight, IntoElement, ParentElement as _, Styled as _, div, px};
use std::collections::BTreeMap;

use crate::domain::connection::ConnectionSummary;
use crate::ui::DatabaseWorkspace;

#[allow(clippy::too_many_arguments)]
pub(crate) fn render(
    cx: &mut Context<DatabaseWorkspace>,
    selected_connection: Option<&ConnectionSummary>,
    connections: &[ConnectionSummary],
    connected: bool,
    server_version: Option<&str>,
    active_database: Option<&str>,
    available_databases: &[String],
    database_switch_loading: bool,
    global_search: &gpui_kit::Entity<InputState>,
    sidebar_visible: bool,
    sql_console_expanded: bool,
) -> impl IntoElement {
    let workspace = cx.entity();
    let conn_workspace = workspace.clone();
    let left_zone = if sidebar_visible {
        h_flex()
            .w(px(260.))
            .h_full()
            .items_center()
            .gap_2()
            .px_2()
            .bg(cx.theme().sidebar)
            .border_r_1()
            .border_color(cx.theme().sidebar_border)
            .child(div().w(px(72.)))
            .child(
                Button::new("toggle-sidebar-button")
                    .ghost()
                    .xsmall()
                    .icon(Icon::new(gpui_kit::assets::IconName::PanelLeftClose))
                    .tooltip("Hide Sidebar (⌘B)")
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.toggle_table_sidebar(cx);
                    })),
            )
            .child(
                div()
                    .font_weight(FontWeight::BOLD)
                    .text_sm()
                    .text_color(cx.theme().foreground)
                    .child("tableX"),
            )
    } else {
        h_flex()
            .h_full()
            .items_center()
            .gap_2()
            .px_2()
            .child(div().w(px(72.)))
            .child(
                Button::new("toggle-sidebar-button")
                    .ghost()
                    .xsmall()
                    .icon(Icon::new(gpui_kit::assets::IconName::PanelLeft))
                    .tooltip("Show Sidebar (⌘B)")
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.toggle_table_sidebar(cx);
                    })),
            )
            .child(
                div()
                    .font_weight(FontWeight::BOLD)
                    .text_sm()
                    .text_color(cx.theme().foreground)
                    .child("tableX"),
            )
    };

    h_flex()
        .w_full()
        .h(px(38.))
        .items_center()
        .bg(cx.theme().background)
        .border_b_1()
        .border_color(cx.theme().title_bar_border)
        .child(left_zone)
        .child(
            h_flex()
                .flex_1()
                .h_full()
                .items_center()
                .px_2()
                .child(div().flex_1())
                // Center zone: macOS Path / Breadcrumb control
                .child(
                    h_flex()
                        .items_center()
                        .gap_1()
                        .px_2()
                        .py_0p5()
                        .rounded_md()
                        .bg(cx.theme().secondary)
                        .border_1()
                        .border_color(cx.theme().border)
                        .child(
                            Button::new("connection-selector")
                                .ghost()
                                .xsmall()
                                .icon(Icon::new(gpui_kit::assets::IconName::Database))
                                .label(
                                    selected_connection
                                        .map(|connection| connection.name.clone())
                                        .unwrap_or_else(|| "No connection".to_owned()),
                                )
                                .dropdown_caret(true)
                                .dropdown_menu({
                                    let connections = connections.to_owned();
                                    move |menu, _, _| {
                                        connections.clone().into_iter().fold(
                                            menu,
                                            |menu, connection| {
                                                let workspace = conn_workspace.clone();
                                                let connection_id = connection.id.clone();
                                                menu.item(
                                                    PopupMenuItem::new(connection.name).on_click(
                                                        move |_, _, cx| {
                                                            workspace.update(cx, |workspace, cx| {
                                                                workspace.connect_connection(
                                                                    &connection_id,
                                                                    cx,
                                                                )
                                                            })
                                                        },
                                                    ),
                                                )
                                            },
                                        )
                                    }
                                }),
                        )
                        .child(
                            div()
                                .text_xs()
                                .text_color(cx.theme().muted_foreground)
                                .child("›"),
                        )
                        .child(database_selector(
                            workspace.clone(),
                            connected,
                            active_database,
                            available_databases,
                            database_switch_loading,
                        )),
                )
                .child(div().flex_1())
                // Right zone: Search pill, Status badge, Quick Actions
                .child(
                    h_flex()
                        .items_center()
                        .gap_2()
                        .child(
                            h_flex()
                                .w(px(220.))
                                .items_center()
                                .gap_1()
                                .child(Input::new(global_search).xsmall().flex_1())
                                .child(
                                    Button::new("global-table-search")
                                        .ghost()
                                        .xsmall()
                                        .icon(IconName::Search)
                                        .tooltip("Search table names")
                                        .on_click(cx.listener(|this, _, window, cx| {
                                            this.apply_global_table_search(window, cx);
                                        })),
                                ),
                        )
                        .child(
                            h_flex()
                                .items_center()
                                .gap_1p5()
                                .px_2()
                                .py_0p5()
                                .rounded_full()
                                .bg(cx.theme().accent)
                                .border_1()
                                .border_color(cx.theme().border)
                                .child(div().w(px(6.)).h(px(6.)).rounded_full().bg(if connected {
                                    cx.theme().success
                                } else {
                                    cx.theme().muted_foreground
                                }))
                                .child(
                                    div()
                                        .text_xs()
                                        .text_color(cx.theme().muted_foreground)
                                        .child(if connected {
                                            server_version
                                                .map(|v| format!("Connected · PG {v}"))
                                                .unwrap_or_else(|| "Connected".to_owned())
                                        } else {
                                            "Disconnected".to_owned()
                                        }),
                                ),
                        )
                        .child(
                            Button::new("toolbar-toggle-sql")
                                .ghost()
                                .xsmall()
                                .icon(Icon::new(gpui_kit::assets::IconName::Terminal))
                                .selected(sql_console_expanded)
                                .tooltip(if sql_console_expanded {
                                    "Hide SQL Console"
                                } else {
                                    "Show SQL Console"
                                })
                                .on_click(cx.listener(|this, _, window, cx| {
                                    this.toggle_sql_console(window, cx);
                                })),
                        )
                        .child(
                            Button::new("toolbar-refresh-objects")
                                .ghost()
                                .xsmall()
                                .icon(Icon::new(gpui_kit::assets::IconName::RefreshCw))
                                .tooltip("Refresh database objects")
                                .on_click(cx.listener(|this, _, _, cx| {
                                    this.refresh_database_objects(cx);
                                })),
                        ),
                ),
        )
}

fn database_selector(
    workspace: gpui_kit::Entity<DatabaseWorkspace>,
    connected: bool,
    active_database: Option<&str>,
    available_databases: &[String],
    database_switch_loading: bool,
) -> impl IntoElement {
    if !connected {
        return div().child(
            Button::new("database-selector")
                .ghost()
                .xsmall()
                .icon(Icon::new(gpui_kit::assets::IconName::Database))
                .label("No database")
                .disabled(true),
        );
    }

    let current_db = active_database.unwrap_or("No database");
    let label = if database_switch_loading {
        format!("Switching to {current_db}...")
    } else {
        current_db.to_owned()
    };

    let available_databases = available_databases.to_vec();
    let active_db_str = active_database.map(str::to_owned);

    div().child(
        Button::new("database-selector")
            .ghost()
            .xsmall()
            .icon(Icon::new(gpui_kit::assets::IconName::Database))
            .label(label)
            .disabled(database_switch_loading)
            .dropdown_caret(true)
            .dropdown_menu(move |menu, window, cx| {
                let workspace_refresh = workspace.clone();
                let mut menu = menu.item(
                    PopupMenuItem::new("Refresh databases")
                        .icon(Icon::new(gpui_kit::assets::IconName::RefreshCw))
                        .on_click(move |_, _, cx| {
                            workspace_refresh.update(cx, |workspace, cx| {
                                workspace.refresh_databases(cx);
                            });
                        }),
                );

                if available_databases.is_empty() {
                    if let Some(active) = &active_db_str {
                        menu = menu.item(
                            PopupMenuItem::new(active.clone())
                                .checked(true)
                                .disabled(true),
                        );
                    }
                    return menu;
                }

                if available_databases.len() <= 20 {
                    for db in &available_databases {
                        let db_name = db.clone();
                        let is_active = active_db_str.as_deref() == Some(db.as_str());
                        let workspace = workspace.clone();
                        menu = menu.item(
                            PopupMenuItem::new(db_name.clone())
                                .checked(is_active)
                                .on_click(move |_, _, cx| {
                                    workspace.update(cx, |workspace, cx| {
                                        workspace.switch_database(&db_name, cx);
                                    });
                                }),
                        );
                    }
                } else {
                    let mut groups: BTreeMap<char, Vec<String>> = BTreeMap::new();
                    for db in &available_databases {
                        let first_char = db
                            .chars()
                            .next()
                            .map(|c| c.to_ascii_uppercase())
                            .unwrap_or('#');
                        groups.entry(first_char).or_default().push(db.clone());
                    }
                    for (letter, group_dbs) in groups {
                        let workspace = workspace.clone();
                        let active_db_str = active_db_str.clone();
                        menu = menu.submenu(
                            format!("Databases: {letter}"),
                            window,
                            cx,
                            move |sub, _, _| {
                                group_dbs.iter().fold(sub, |sub, db| {
                                    let db_name = db.clone();
                                    let is_active = active_db_str.as_deref() == Some(db.as_str());
                                    let workspace = workspace.clone();
                                    sub.item(
                                        PopupMenuItem::new(db_name.clone())
                                            .checked(is_active)
                                            .on_click(move |_, _, cx| {
                                                workspace.update(cx, |workspace, cx| {
                                                    workspace.switch_database(&db_name, cx);
                                                });
                                            }),
                                    )
                                })
                            },
                        );
                    }
                }

                menu
            }),
    )
}
