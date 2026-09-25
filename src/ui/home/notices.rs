use gpui_kit::base::v_flex;
use gpui_kit::component::{ActiveTheme as _, alert::Alert};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{Context, IntoElement, ParentElement as _, Styled as _, div, px};

use crate::ui::{DatabaseWorkspace, Notice, NoticeLevel};

pub(crate) fn render(cx: &mut Context<DatabaseWorkspace>, notice: &Notice) -> impl IntoElement {
    let alert = match notice.level {
        NoticeLevel::Info => Alert::info("workspace-toast", notice.message.clone()),
        NoticeLevel::Success => Alert::success("workspace-toast", notice.message.clone()),
        NoticeLevel::Warning => Alert::warning("workspace-toast", notice.message.clone()),
        NoticeLevel::Error => Alert::error("workspace-toast", notice.message.clone()),
    }
    .title(notice.title.clone())
    .on_close(cx.listener(|this, _, _, cx| this.dismiss_notice(cx)));

    v_flex()
        .absolute()
        .top(px(16.))
        .right(px(20.))
        .w(px(460.))
        .gap_2()
        .child(alert)
        .when_some(notice.detail.clone(), |this, detail| {
            this.child(
                div()
                    .px_4()
                    .text_xs()
                    .text_color(cx.theme().muted_foreground)
                    .child(detail),
            )
        })
}
