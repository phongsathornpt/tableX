use gpui_kit::component::{Theme, ThemeMode, TitleBar};
use gpui_kit::{AppContext as _, application, rgb};

use crate::ui::DatabaseWorkspace;

pub fn run() {
    #[cfg(feature = "perf-overlay")]
    let report_startup_frames = std::env::var_os("TABLEX_DEBUG_FRAME_OVERLAY").is_some();
    let app = application().with_assets(gpui_kit::assets::AllAssets);
    app.run(move |cx| {
        gpui_kit::init(cx);
        Theme::change(ThemeMode::Dark, None, cx);
        {
            let theme = Theme::global_mut(cx);
            theme.background = rgb(0x0e1727).into();
            theme.foreground = rgb(0xdce6f3).into();
            theme.secondary = rgb(0x162338).into();
            theme.secondary_foreground = rgb(0xc3d0e1).into();
            theme.border = rgb(0x293950).into();
            theme.input = rgb(0x30415b).into();
            theme.muted_foreground = rgb(0x91a2ba).into();
            theme.primary = rgb(0x19d5c7).into();
            theme.primary_foreground = rgb(0x06262b).into();
            theme.primary_hover = rgb(0x36e1d4).into();
            theme.primary_active = rgb(0x10b8ad).into();
            theme.button_primary = rgb(0x19d5c7).into();
            theme.button_primary_foreground = rgb(0x06262b).into();
            theme.button_primary_hover = rgb(0x36e1d4).into();
            theme.accent = rgb(0x1b2a40).into();
            theme.accent_foreground = rgb(0xdce6f3).into();
            theme.list.active_highlight = true;
            theme.list_head = rgb(0x26364d).into();
            theme.list_even = rgb(0x17243a).into();
            theme.list_hover = rgb(0x1b2b42).into();
            theme.list_active = rgb(0x153748).into();
            theme.list_active_border = rgb(0x19d5c7).into();
            theme.sidebar = rgb(0x111c2d).into();
            theme.sidebar_foreground = rgb(0xcbd7e8).into();
            theme.sidebar_accent = rgb(0x173348).into();
            theme.sidebar_accent_foreground = rgb(0x47e0d5).into();
            theme.sidebar_border = rgb(0x26364d).into();
            theme.table = rgb(0x121e31).into();
            theme.table_head = rgb(0x26364d).into();
            theme.table_head_foreground = rgb(0xdce6f3).into();
            theme.table_even = rgb(0x17243a).into();
            theme.table_hover = rgb(0x1c2b41).into();
            theme.table_row_border = rgb(0x27374d).into();
            theme.title_bar = rgb(0x0c1422).into();
            theme.title_bar_border = rgb(0x293950).into();
            theme.status_bar = rgb(0x101a2a).into();
            theme.status_bar_border = rgb(0x293950).into();
        }
        Theme::sync_base(cx);
        cx.spawn(async move |cx| {
            cx.open_window(TitleBar::window_options(), move |window, cx| {
                #[cfg(feature = "perf-overlay")]
                if report_startup_frames {
                    window.set_debug_frame_overlay_mode(gpui_kit::DebugFrameOverlayMode::Full);
                }
                #[cfg(feature = "perf-overlay")]
                let workspace_started = std::time::Instant::now();
                let view = cx.new(|cx| DatabaseWorkspace::new(window, cx));
                #[cfg(feature = "perf-overlay")]
                if report_startup_frames {
                    eprintln!(
                        "[tableX perf] DatabaseWorkspace::new: {:.2} ms",
                        workspace_started.elapsed().as_secs_f64() * 1000.0
                    );
                }
                #[cfg(feature = "perf-overlay")]
                let root_started = std::time::Instant::now();
                let root = cx.new(|cx| gpui_kit::component::Root::new(view, window, cx));
                #[cfg(feature = "perf-overlay")]
                if report_startup_frames {
                    eprintln!(
                        "[tableX perf] Root::new: {:.2} ms",
                        root_started.elapsed().as_secs_f64() * 1000.0
                    );
                    window.on_next_frame(report_first_frame_metrics);
                }
                root
            })
            .expect("failed to open tableX window");
        })
        .detach();
    });
}

#[cfg(feature = "perf-overlay")]
fn report_first_frame_metrics(window: &mut gpui_kit::Window, _cx: &mut gpui_kit::App) {
    let snapshot = window.frame_duration_snapshot();
    let draw = &snapshot.draw_duration_histogram;
    let dirty_to_present = &snapshot.dirty_to_present_histogram;
    if draw.is_empty() || dirty_to_present.is_empty() {
        window.on_next_frame(report_first_frame_metrics);
        return;
    }

    let as_ms = |nanos: u64| nanos as f64 / 1_000_000.0;
    eprintln!(
        "[tableX perf] startup-window samples: draw n={} p50={:.2} p90={:.2} p99={:.2} max={:.2} ms; dirty-to-present n={} p50={:.2} p90={:.2} p99={:.2} max={:.2} ms",
        draw.len(),
        as_ms(draw.value_at_quantile(0.5)),
        as_ms(draw.value_at_quantile(0.9)),
        as_ms(draw.value_at_quantile(0.99)),
        as_ms(draw.max()),
        dirty_to_present.len(),
        as_ms(dirty_to_present.value_at_quantile(0.5)),
        as_ms(dirty_to_present.value_at_quantile(0.9)),
        as_ms(dirty_to_present.value_at_quantile(0.99)),
        as_ms(dirty_to_present.max()),
    );
}
