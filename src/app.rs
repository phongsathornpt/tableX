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
            theme.background = rgb(0x1e1e1e).into();
            theme.foreground = rgb(0xf5f5f7).into();
            theme.secondary = rgb(0x252528).into();
            theme.secondary_foreground = rgb(0xd1d1d6).into();
            theme.border = rgb(0x38383a).into();
            theme.input = rgb(0x2a2a2d).into();
            theme.muted_foreground = rgb(0x8e8e93).into();
            theme.primary = rgb(0x0a84ff).into();
            theme.primary_foreground = rgb(0xffffff).into();
            theme.primary_hover = rgb(0x409cff).into();
            theme.primary_active = rgb(0x0062cc).into();
            theme.button_primary = rgb(0x0a84ff).into();
            theme.button_primary_foreground = rgb(0xffffff).into();
            theme.button_primary_hover = rgb(0x409cff).into();
            theme.accent = rgb(0x2c2c2e).into();
            theme.accent_foreground = rgb(0xf5f5f7).into();
            theme.list.active_highlight = true;
            theme.list_head = rgb(0x2c2c2e).into();
            theme.list_even = rgb(0x222225).into();
            theme.list_hover = rgb(0x2a2a2d).into();
            theme.list_active = rgb(0x0a84ff).into();
            theme.list_active_border = rgb(0x0a84ff).into();
            theme.sidebar = rgb(0x18181a).into();
            theme.sidebar_foreground = rgb(0xd1d1d6).into();
            theme.sidebar_accent = rgb(0x0a84ff).into();
            theme.sidebar_accent_foreground = rgb(0xffffff).into();
            theme.sidebar_border = rgb(0x2e2e32).into();
            theme.table = rgb(0x1e1e1e).into();
            theme.table_head = rgb(0x28282b).into();
            theme.table_head_foreground = rgb(0xf5f5f7).into();
            theme.table_even = rgb(0x222225).into();
            theme.table_hover = rgb(0x2c2c30).into();
            theme.table_row_border = rgb(0x2a2a2d).into();
            theme.title_bar = rgb(0x1a1a1c).into();
            theme.title_bar_border = rgb(0x2e2e32).into();
            theme.status_bar = rgb(0x1a1a1c).into();
            theme.status_bar_border = rgb(0x2e2e32).into();
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
