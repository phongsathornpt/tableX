use gpui_kit::base::{h_flex, v_flex};
use gpui_kit::component::{
    ActiveTheme as _, IconName, Sizable as _,
    button::{Button, ButtonVariants as _},
};
use gpui_kit::{Context, FontWeight, IntoElement, ParentElement as _, Styled as _, div};

use crate::domain::connection::ConnectionSummary;
use crate::ui::DatabaseWorkspace;

pub(crate) fn render(
    cx: &mut Context<DatabaseWorkspace>,
    connections: &[ConnectionSummary],
    selected_connection: Option<&ConnectionSummary>,
    connected: bool,
    pending_delete: Option<&String>,
) -> impl IntoElement {
    let mut list = v_flex().w_full().gap_3();

    for connection in connections {
        list = list.child(render_card(
            cx,
            connection,
            selected_connection.is_some_and(|selected| selected.id == connection.id),
            connected,
            pending_delete.is_some_and(|id| id == &connection.id),
        ));
    }

    if connections.is_empty() {
        list = list.child(
            v_flex()
                .w_full()
                .items_center()
                .gap_2()
                .px_4()
                .py_8()
                .rounded_md()
                .border_1()
                .border_color(cx.theme().border)
                .child(
                    div()
                        .font_weight(FontWeight::SEMIBOLD)
                        .child("No connections yet"),
                )
                .child(
                    div()
                        .text_sm()
                        .text_color(cx.theme().muted_foreground)
                        .child("Add a PostgreSQL connection to get started."),
                ),
        );
    }

    list.child(
        h_flex()
            .w_full()
            .justify_between()
            .items_center()
            .pt_2()
            .child(
                div()
                    .text_sm()
                    .text_color(cx.theme().muted_foreground)
                    .child(format!(
                        "{} saved connection{}",
                        connections.len(),
                        if connections.len() == 1 { "" } else { "s" }
                    )),
            )
            .child(
                Button::new("add-connection")
                    .primary()
                    .small()
                    .icon(IconName::Plus)
                    .label("Add connection")
                    .on_click(cx.listener(|this, _, window, cx| {
                        this.open_connection_editor(window, cx);
                    })),
            ),
    )
}

fn render_card(
    cx: &mut Context<DatabaseWorkspace>,
    connection: &ConnectionSummary,
    selected: bool,
    connected: bool,
    delete_pending: bool,
) -> impl IntoElement {
    let connection_id = connection.id.clone();
    let edit_connection_id = connection.id.clone();
    let delete_connection_id = connection.id.clone();
    let status_color = if selected && connected {
        cx.theme().success
    } else {
        cx.theme().muted_foreground
    };

    h_flex()
        .w_full()
        .px_4()
        .py_4()
        .gap_3()
        .rounded_md()
        .border_1()
        .border_color(cx.theme().border)
        .bg(if selected {
            cx.theme().secondary
        } else {
            cx.theme().background
        })
        .child(div().size_3().rounded_full().bg(status_color))
        .child(
            v_flex()
                .flex_1()
                .gap_1()
                .child(
                    h_flex()
                        .gap_2()
                        .child(
                            div()
                                .font_weight(FontWeight::SEMIBOLD)
                                .child(connection.name.clone()),
                        )
                        .child(div().text_sm().text_color(status_color).child(
                            if selected && connected {
                                "Connected"
                            } else if selected {
                                "Current"
                            } else {
                                "Saved"
                            },
                        )),
                )
                .child(
                    div()
                        .text_sm()
                        .text_color(cx.theme().muted_foreground)
                        .child(format!(
                            "{}  •  {}:{}",
                            connection.database, connection.host, connection.port
                        )),
                ),
        )
        .child(
            Button::new(format!("edit-{}", connection.id))
                .ghost()
                .small()
                .label("Edit")
                .on_click(cx.listener(move |this, _, window, cx| {
                    this.edit_connection(&edit_connection_id, window, cx);
                })),
        )
        .child(
            Button::new(format!("delete-{}", connection.id))
                .ghost()
                .small()
                .danger()
                .label(if delete_pending {
                    "Confirm delete"
                } else {
                    "Delete"
                })
                .on_click(cx.listener(move |this, _, _, cx| {
                    this.request_delete_connection(&delete_connection_id, cx);
                })),
        )
        .child(
            Button::new(format!("connect-{}", connection.id))
                .outline()
                .small()
                .label(if selected && connected {
                    "Connected"
                } else {
                    "Connect"
                })
                .on_click(cx.listener(move |this, _, _, cx| {
                    this.connect_connection(&connection_id, cx);
                })),
        )
}
