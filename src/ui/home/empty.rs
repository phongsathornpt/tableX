use gpui_kit::base::v_flex;
use gpui_kit::component::ActiveTheme as _;
use gpui_kit::{Context, IntoElement, ParentElement as _, Styled as _, div};

use crate::ui::DatabaseWorkspace;

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
