use crate::ui::DatabaseWorkspace;
use gpui_kit::base::v_flex;
use gpui_kit::component::tab::{Tab, TabBar};
use gpui_kit::{Context, FontWeight, IntoElement, ParentElement as _, Styled as _, div};

pub fn render(active_tab: usize, cx: &mut Context<DatabaseWorkspace>) -> impl IntoElement {
    v_flex()
        .flex_1()
        .h_full()
        .child(
            TabBar::new("workspace-tabs")
                .selected_index(active_tab)
                .on_click(cx.listener(|this, index, _, cx| {
                    this.select_tab(*index, cx);
                }))
                .child(Tab::new().label("Welcome"))
                .child(Tab::new().label("SQL query")),
        )
        .child(
            v_flex()
                .flex_1()
                .items_center()
                .justify_center()
                .gap_2()
                .child("▧")
                .child(div().text_lg().font_weight(FontWeight::SEMIBOLD).child(
                    if active_tab == 0 {
                        "Select a database object"
                    } else {
                        "SQL editor coming next"
                    },
                ))
                .child(div().text_sm().child(if active_tab == 0 {
                    "Choose a table, view, or schema from the explorer."
                } else {
                    "The query editor will use the same workspace tab model."
                })),
        )
}
