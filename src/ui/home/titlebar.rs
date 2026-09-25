use gpui_kit::base::h_flex;
use gpui_kit::component::{
    ActiveTheme as _, Icon, IconName, Sizable as _,
    button::{Button, ButtonVariants as _},
    input::{Input, InputState},
    menu::{DropdownMenu as _, PopupMenuItem},
};
use gpui_kit::{Context, FontWeight, IntoElement, ParentElement as _, Styled as _, div, px};

use crate::domain::connection::ConnectionSummary;
use crate::ui::DatabaseWorkspace;

pub(crate) fn render(
    cx: &mut Context<DatabaseWorkspace>,
    selected_connection: Option<&ConnectionSummary>,
    connections: &[ConnectionSummary],
    connected: bool,
    server_version: Option<&str>,
    global_search: &gpui_kit::Entity<InputState>,
) -> impl IntoElement {
    let workspace = cx.entity();
    h_flex()
        .w_full()
        .items_center()
        .gap_4()
        .child(
            div()
                .w(px(104.))
                .font_weight(FontWeight::BOLD)
                .text_lg()
                .text_color(cx.theme().foreground)
                .child("tableX"),
        )
        .child(
            Button::new("connection-selector")
                .outline()
                .small()
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
                        connections
                            .clone()
                            .into_iter()
                            .fold(menu, |menu, connection| {
                                let workspace = workspace.clone();
                                let connection_id = connection.id.clone();
                                menu.item(PopupMenuItem::new(connection.name).on_click(
                                    move |_, _, cx| {
                                        workspace.update(cx, |workspace, cx| {
                                            workspace.connect_connection(&connection_id, cx)
                                        })
                                    },
                                ))
                            })
                    }
                }),
        )
        .child(
            h_flex()
                .items_center()
                .gap_2()
                .child(div().w(px(7.)).h(px(7.)).rounded_full().bg(if connected {
                    cx.theme().success
                } else {
                    cx.theme().muted_foreground
                }))
                .child(
                    div()
                        .text_xs()
                        .text_color(cx.theme().muted_foreground)
                        .child(if connected {
                            "Connected"
                        } else {
                            "Disconnected"
                        }),
                )
                .child(
                    div()
                        .text_xs()
                        .text_color(cx.theme().muted_foreground)
                        .child(server_version.unwrap_or("").to_owned()),
                ),
        )
        .child(div().flex_1())
        .child(
            h_flex()
                .w(px(352.))
                .items_center()
                .gap_2()
                .child(Input::new(global_search).flex_1())
                .child(
                    Button::new("global-table-search")
                        .ghost()
                        .small()
                        .icon(IconName::Search)
                        .tooltip("Search table names")
                        .on_click(cx.listener(|this, _, window, cx| {
                            this.apply_global_table_search(window, cx);
                        })),
                ),
        )
}
