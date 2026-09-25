use gpui_kit::base::Selectable as _;
use gpui_kit::base::{h_flex, v_flex};
use gpui_kit::component::{
    ActiveTheme as _, Icon, Sizable as _,
    button::{Button, ButtonVariants as _},
};
use gpui_kit::{Context, IntoElement, ParentElement as _, Styled as _, div, px};

use super::{DatabaseWorkspace, QueryDockTab};

pub(crate) fn render(
    cx: &mut Context<DatabaseWorkspace>,
    connected: bool,
    _connection_name: Option<&str>,
    sidebar_visible: bool,
    query_dock_tab: QueryDockTab,
) -> impl IntoElement {
    let workspace = cx.entity();
    let query_workspace = workspace.clone();
    let results_workspace = workspace.clone();
    let settings_workspace = workspace.clone();
    v_flex()
        .w(px(66.))
        .h_full()
        .items_center()
        .justify_between()
        .py_3()
        .border_r_1()
        .border_color(cx.theme().border)
        .bg(cx.theme().sidebar)
        .child(
            v_flex()
                .w_full()
                .items_center()
                .gap_2()
                .child(nav_button(
                    "database-nav",
                    Icon::new(gpui_kit::assets::IconName::Database),
                    "Database objects",
                    sidebar_visible,
                    cx.listener(|this, _, _, cx| this.toggle_table_sidebar(cx)),
                ))
                .child(nav_button(
                    "table-nav",
                    Icon::new(gpui_kit::assets::IconName::Table),
                    "Table results",
                    query_dock_tab == QueryDockTab::Results,
                    move |_, window, cx| {
                        results_workspace.update(cx, |this, cx| {
                            this.set_query_dock_tab(QueryDockTab::Results, window, cx)
                        });
                    },
                ))
                .child(nav_button(
                    "query-nav",
                    Icon::new(gpui_kit::assets::IconName::Play),
                    "SQL query",
                    query_dock_tab == QueryDockTab::Query,
                    move |_, window, cx| {
                        query_workspace.update(cx, |this, cx| {
                            this.set_query_dock_tab(QueryDockTab::Query, window, cx)
                        });
                    },
                )),
        )
        .child(
            v_flex()
                .w_full()
                .items_center()
                .gap_2()
                .child(nav_button(
                    "connection-settings-nav",
                    Icon::new(gpui_kit::assets::IconName::Settings2),
                    "Connection settings",
                    false,
                    move |_, window, cx| {
                        settings_workspace
                            .update(cx, |this, cx| this.edit_selected_connection(window, cx));
                    },
                ))
                .child(
                    h_flex().w_full().justify_center().child(
                        div()
                            .text_xs()
                            .text_color(if connected {
                                cx.theme().primary
                            } else {
                                cx.theme().muted_foreground
                            })
                            .child("●"),
                    ),
                ),
        )
}

fn nav_button(
    id: &'static str,
    icon: Icon,
    label: &'static str,
    selected: bool,
    on_click: impl Fn(&gpui_kit::ClickEvent, &mut gpui_kit::Window, &mut gpui_kit::App) + 'static,
) -> impl IntoElement {
    Button::new(id)
        .ghost()
        .small()
        .icon(icon)
        .w(px(46.))
        .h(px(42.))
        .justify_center()
        .selected(selected)
        .tooltip(label)
        .on_click(on_click)
}
