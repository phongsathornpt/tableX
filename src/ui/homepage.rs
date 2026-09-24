use gpui_kit::base::{h_flex, v_flex};
use gpui_kit::component::{
    ActiveTheme as _, IconName, Sizable as _,
    button::{Button, ButtonVariants as _},
    status_bar::StatusBar,
};
use gpui_kit::{Context, FontWeight, IntoElement, ParentElement as _, Styled as _, div, px};

use crate::ui::DatabaseWorkspace;

pub fn render(
    cx: &mut Context<DatabaseWorkspace>,
    homepage_message: &str,
    connected: bool,
) -> impl IntoElement {
    let status = if !connected {
        "●  Not connected".to_string()
    } else if homepage_message.is_empty() {
        "●  Connected".to_string()
    } else {
        homepage_message.to_string()
    };

    v_flex()
        .size_full()
        .bg(cx.theme().background)
        .text_color(cx.theme().foreground)
        .child(
            v_flex().flex_1().items_center().justify_center().child(
                v_flex()
                    .w(px(760.))
                    .gap_6()
                    .child(render_connection_summary(cx))
                    .child(
                        v_flex()
                            .items_center()
                            .gap_2()
                            .child(
                                div()
                                    .text_3xl()
                                    .font_weight(FontWeight::BOLD)
                                    .text_color(cx.theme().primary)
                                    .child("tableX"),
                            )
                            .child(
                                div()
                                    .text_lg()
                                    .text_color(cx.theme().muted_foreground)
                                    .child("A clearer way to work with PostgreSQL"),
                            ),
                    )
                    .child(render_actions(cx))
                    .child(render_recent(cx)),
            ),
        )
        .child(StatusBar::new().left(status).right("PostgreSQL 15.4"))
}

pub(crate) fn render_titlebar(cx: &mut Context<DatabaseWorkspace>) -> impl IntoElement {
    h_flex()
        .flex_1()
        .items_center()
        .gap_4()
        .child(
            div()
                .w(px(120.))
                .font_weight(FontWeight::BOLD)
                .text_lg()
                .text_color(cx.theme().primary)
                .child("tableX"),
        )
        .child(
            Button::new("connection-selector")
                .outline()
                .small()
                .icon(IconName::Folder)
                .label("Production")
                .dropdown_caret(true)
                .on_click(|_, _, _| println!("connection selector requested")),
        )
        .child(
            Button::new("command-search")
                .ghost()
                .small()
                .flex_1()
                .icon(IconName::Search)
                .label("Search tables, run a command...   ⌘ K")
                .on_click(|_, _, _| println!("command search requested")),
        )
        .child(
            Button::new("settings")
                .ghost()
                .small()
                .icon(IconName::Settings)
                .tooltip("Settings")
                .on_click(|_, _, _| println!("settings requested")),
        )
}

fn render_connection_summary(cx: &mut Context<DatabaseWorkspace>) -> impl IntoElement {
    h_flex()
        .w_full()
        .px_4()
        .py_3()
        .gap_3()
        .rounded_md()
        .border_1()
        .border_color(cx.theme().border)
        .bg(cx.theme().secondary)
        .child(div().size_3().rounded_full().bg(cx.theme().success))
        .child(
            v_flex()
                .flex_1()
                .gap_1()
                .child(
                    h_flex()
                        .gap_2()
                        .child("Connected to ")
                        .child(div().font_weight(FontWeight::SEMIBOLD).child("Production")),
                )
                .child(
                    div()
                        .text_sm()
                        .text_color(cx.theme().muted_foreground)
                        .child("postgres@db.internal:5432  •  PostgreSQL 15.4  •  main"),
                ),
        )
        .child(IconName::ChevronDown)
}

fn render_actions(cx: &mut Context<DatabaseWorkspace>) -> impl IntoElement {
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
                            this.show_homepage_message("Database browser requested", cx);
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
                            this.show_homepage_message("New SQL query requested", cx);
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

fn render_recent(cx: &mut Context<DatabaseWorkspace>) -> impl IntoElement {
    let border = cx.theme().border;
    let muted = cx.theme().muted_foreground;

    v_flex()
        .w_full()
        .gap_3()
        .pt_4()
        .border_t_1()
        .border_color(border)
        .child(
            h_flex()
                .justify_between()
                .child(div().font_weight(FontWeight::SEMIBOLD).child("Open recent"))
                .child(
                    Button::new("view-recent")
                        .ghost()
                        .small()
                        .label("View all")
                        .icon(IconName::ArrowRight)
                        .on_click(|_, _, _| println!("recent items requested")),
                ),
        )
        .child(render_recent_item(
            "sales_analysis.sql",
            "Queries  ›  sales_analysis.sql",
            "2 hours ago",
            border,
            muted,
        ))
        .child(render_recent_item(
            "public.customers",
            "Tables  ›  public.customers",
            "5 hours ago",
            border,
            muted,
        ))
        .child(render_recent_item(
            "churn_report.sql",
            "Queries  ›  churn_report.sql",
            "1 day ago",
            border,
            muted,
        ))
        .child(render_recent_item(
            "analytics.events",
            "Tables  ›  analytics.events",
            "2 days ago",
            border,
            muted,
        ))
}

fn render_recent_item(
    name: &str,
    detail: &str,
    time: &str,
    border: gpui_kit::Hsla,
    muted: gpui_kit::Hsla,
) -> impl IntoElement {
    h_flex()
        .w_full()
        .px_3()
        .py_3()
        .gap_3()
        .border_b_1()
        .border_color(border)
        .child(IconName::Folder)
        .child(
            v_flex()
                .flex_1()
                .gap_1()
                .child(
                    div()
                        .font_weight(FontWeight::SEMIBOLD)
                        .child(name.to_string()),
                )
                .child(div().text_sm().text_color(muted).child(detail.to_string())),
        )
        .child(div().text_sm().text_color(muted).child(time.to_string()))
}
