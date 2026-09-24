use gpui_kit::component::TitleBar;
use gpui_kit::{AppContext as _, application};

use crate::infrastructure::MockDatabaseProvider;
use crate::ui::DatabaseWorkspace;

pub fn run() {
    let app = application().with_assets(gpui_kit::assets::Assets);
    app.run(move |cx| {
        gpui_kit::init(cx);
        cx.spawn(async move |cx| {
            cx.open_window(TitleBar::window_options(), |window, cx| {
                let view =
                    cx.new(|cx| DatabaseWorkspace::new(MockDatabaseProvider::new(), window, cx));
                cx.new(|cx| gpui_kit::component::Root::new(view, window, cx))
            })
            .expect("failed to open tableX window");
        })
        .detach();
    });
}
