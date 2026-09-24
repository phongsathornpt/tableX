use gpui_kit::component::status_bar::StatusBar;
use gpui_kit::{IntoElement, ParentElement as _};

pub fn render(selected_connection: &str) -> impl IntoElement {
    StatusBar::new()
        .left("●")
        .child(format!("Connected to {selected_connection}"))
        .right("PostgreSQL")
}
