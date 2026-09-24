use gpui_kit::base::v_flex;
use gpui_kit::component::{ActiveTheme as _, Root, TitleBar};
use gpui_kit::{Context, ParentElement as _, Render, Styled as _, Window};

use super::homepage;
use crate::domain::{connection::ConnectionStatus, workspace::WorkspaceState};
use crate::infrastructure::{DatabaseProvider, MockDatabaseProvider};

pub struct DatabaseWorkspace {
    workspace: WorkspaceState,
    connection_status: ConnectionStatus,
    homepage_message: String,
}

impl DatabaseWorkspace {
    pub fn new(provider: MockDatabaseProvider, _: &mut Window, _cx: &mut Context<Self>) -> Self {
        let connection = provider.connections().into_iter().next();
        let selected_connection = connection.map(|connection| connection.id);

        Self {
            workspace: WorkspaceState::new(selected_connection),
            connection_status: ConnectionStatus::Connected,
            homepage_message: String::new(),
        }
    }

    pub(crate) fn show_homepage_message(&mut self, message: &str, cx: &mut Context<Self>) {
        self.homepage_message = message.to_string();
        cx.notify();
    }
}

impl Render for DatabaseWorkspace {
    fn render(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> impl gpui_kit::IntoElement {
        let connected = self.workspace.selected_connection.is_some()
            && matches!(self.connection_status, ConnectionStatus::Connected);

        v_flex()
            .size_full()
            .bg(cx.theme().background)
            .text_color(cx.theme().foreground)
            .child(TitleBar::new().child(homepage::render_titlebar(cx)))
            .child(homepage::render(cx, &self.homepage_message, connected))
            .children(Root::render_dialog_layer(window, cx))
            .children(Root::render_sheet_layer(window, cx))
            .children(Root::render_notification_layer(window, cx))
    }
}
