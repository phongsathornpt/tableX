use super::connection::ConnectionId;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WorkspaceState {
    pub active_tab: usize,
    pub selected_connection: Option<ConnectionId>,
    pub selected_object: Option<String>,
}

impl WorkspaceState {
    pub fn new(selected_connection: Option<ConnectionId>) -> Self {
        Self {
            active_tab: 0,
            selected_connection,
            selected_object: None,
        }
    }
}
