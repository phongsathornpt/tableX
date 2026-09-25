use gpui_kit::base::{h_flex, v_flex};
use gpui_kit::component::{
    ActiveTheme as _, IconName, Sizable as _,
    button::{Button, ButtonVariants as _},
};
use gpui_kit::{Context, IntoElement, ParentElement as _, Styled as _, div};

use crate::ui::DatabaseWorkspace;

pub(crate) fn actions(cx: &mut Context<DatabaseWorkspace>) -> impl IntoElement {
    h_flex()
        .w_full()
        .gap_3()
        .child(
            v_flex()
                .flex_1()
                .items_center()
                .gap_2()
                .child(
                    Button::new("browse-database")
                        .primary()
                        .large()
                        .icon(IconName::Folder)
                        .label("Browse a database")
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.show_homepage_message(
                                "Connect to a database first, then use the object explorer",
                                cx,
                            );
                        })),
                )
                .child(
                    div()
                        .text_sm()
                        .text_color(cx.theme().muted_foreground)
                        .child("Explore tables, views, functions, and more"),
                ),
        )
        .child(
            v_flex()
                .flex_1()
                .items_center()
                .gap_2()
                .child(
                    Button::new("new-query")
                        .outline()
                        .large()
                        .icon(IconName::FileText)
                        .label("New query")
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.show_homepage_message(
                                "Connect to a database before opening a query",
                                cx,
                            );
                        })),
                )
                .child(
                    div()
                        .text_sm()
                        .text_color(cx.theme().muted_foreground)
                        .child("Open a new SQL editor"),
                ),
        )
}

pub(crate) fn recent(cx: &mut Context<DatabaseWorkspace>) -> impl IntoElement {
    let muted = cx.theme().muted_foreground;

    v_flex()
        .w_full()
        .gap_3()
        .pt_4()
        .border_t_1()
        .border_color(cx.theme().border)
        .child(
            div()
                .font_weight(gpui_kit::FontWeight::SEMIBOLD)
                .child("Recent activity"),
        )
        .child(
            v_flex().w_full().items_center().gap_2().py_6().child(
                div().text_sm().text_color(muted).child(
                    "Recent queries and tables will appear after you connect and open data.",
                ),
            ),
        )
}
