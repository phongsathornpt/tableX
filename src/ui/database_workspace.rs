// Transitional feature boundary. The implementation is kept in the existing
// file while responsibilities are moved into this directory incrementally.
use gpui_kit::base::{h_flex, v_flex};
use gpui_kit::component::{
    ActiveTheme as _, Root, TitleBar,
    input::{InputState, TextareaState},
    menu::PopupMenuItem,
};
use gpui_kit::{
    AppContext as _, Context, Focusable as _, IntoElement, ParentElement as _, Render, Styled as _,
    UniformListScrollHandle, Window, px,
};
use std::collections::{HashMap, HashSet};
use std::rc::Rc;
use std::sync::Arc;
#[cfg(feature = "perf-overlay")]
use std::sync::atomic::AtomicBool;
use std::sync::atomic::{AtomicU64, Ordering};

use crate::domain::{
    connection::{ConnectionId, ConnectionStatus, ConnectionSummary},
    database_object::{TableCursor, TableRelationType},
    query::{TableColumnFilter, TableDataCursor, TableDataCursorDirection, TableFilterOperator},
    workspace::WorkspaceState,
};
use crate::infrastructure::postgres::model::{PostgresConnectionProfile, PostgresSslMode};
use crate::infrastructure::{ConnectionStore, CredentialStore, PostgresProvider, QueryResult};
use crate::ui::{Notice, ObjectExplorer, homepage};

mod connection_editor;
mod connections;
mod metadata;
mod navigation;
mod query;
pub(crate) use connection_editor::ConnectionEditor;
pub(crate) use query::is_inline_edit_type;

pub(crate) const MAX_ENUM_MENU_OPTIONS: usize = 100;

#[cfg(feature = "perf-overlay")]
static STARTUP_RENDER_TIMING_REPORTED: AtomicBool = AtomicBool::new(false);

pub struct DatabaseWorkspace {
    workspace: WorkspaceState,
    connections: Vec<ConnectionSummary>,
    connection_store: ConnectionStore,
    credential_store: CredentialStore,
    postgres_provider: PostgresProvider,
    connection_profiles: HashMap<ConnectionId, PostgresConnectionProfile>,
    connection_generation: u64,
    connection_status: ConnectionStatus,
    server_version: Option<String>,
    notice: Option<Notice>,
    connection_editor: Option<ConnectionEditor>,
    object_explorer: Option<ObjectExplorer>,
    pending_delete: Option<ConnectionId>,
    query_input: gpui_kit::Entity<TextareaState>,
    query_dock_tab: QueryDockTab,
    query_result: Option<Arc<QueryResult>>,
    result_column_widths: Rc<Vec<f32>>,
    query_running: bool,
    query_generation: u64,
    connection_test_running: bool,
    connection_test_generation: u64,
    write_confirmation_sql: Option<String>,
    table_search: gpui_kit::Entity<InputState>,
    global_search: gpui_kit::Entity<InputState>,
    table_schema_filter: Option<String>,
    table_type_filter: Option<TableRelationType>,
    table_page_loading: bool,
    table_page_generation: u64,
    table_page_offset: usize,
    table_page_limit: usize,
    table_page_has_next: bool,
    table_page_cursors: Vec<Option<TableCursor>>,
    selected_table: Option<(String, String)>,
    table_data_offset: usize,
    table_data_limit: usize,
    table_data_has_next: bool,
    table_data_sort: Option<(String, bool)>,
    table_data_filter_input: gpui_kit::Entity<InputState>,
    table_data_filter: Option<String>,
    table_data_filter_column: Option<String>,
    table_data_empty_filter: Option<(String, bool)>,
    table_column_filters: Vec<TableColumnFilter>,
    table_filter_input: gpui_kit::Entity<InputState>,
    table_filter_editor_column: Option<String>,
    table_filter_editor_operator: TableFilterOperator,
    active_cell_edit: Option<ActiveCellEdit>,
    cell_update_reload_pending: bool,
    filtered_table_data_rows: Option<Rc<Vec<usize>>>,
    table_data_filter_pending: bool,
    filter_cache_generation: Arc<AtomicU64>,
    hidden_table_data_columns: HashSet<String>,
    table_result_scroll: UniformListScrollHandle,
    table_result_horizontal_scroll: gpui_kit::ScrollHandle,
    table_sidebar_visible: bool,
}

struct ActiveCellEdit {
    row_index: usize,
    column_index: usize,
    input: gpui_kit::Entity<InputState>,
    set_null: bool,
    enum_values: Option<Vec<String>>,
    enum_value: Option<String>,
}

#[derive(Clone)]
pub(crate) struct ActiveCellEditView {
    pub(crate) row_index: usize,
    pub(crate) column_index: usize,
    pub(crate) input: gpui_kit::Entity<InputState>,
    pub(crate) set_null: bool,
    pub(crate) enum_values: Option<Vec<String>>,
    pub(crate) enum_value: Option<String>,
}

impl DatabaseWorkspace {
    pub(crate) fn query_result(&self) -> Option<&QueryResult> {
        self.query_result.as_deref()
    }

    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        Self::new_with_stores(window, cx, ConnectionStore::new(), CredentialStore::new())
    }

    fn new_with_stores(
        window: &mut Window,
        cx: &mut Context<Self>,
        connection_store: ConnectionStore,
        credential_store: CredentialStore,
    ) -> Self {
        let (connections, storage_notice) = match connection_store.load() {
            Ok(persisted_connections) if !persisted_connections.is_empty() => {
                (persisted_connections, None)
            }
            Ok(_) => (Vec::new(), None),
            Err(error) => (
                Vec::new(),
                Some(
                    Notice::warning(
                        "Could not load saved connections",
                        "Saved connections could not be restored.",
                    )
                    .with_detail(error.message),
                ),
            ),
        };
        let connection = connections.first().cloned();
        let selected_connection = connection.map(|connection| connection.id);
        let connection_profiles = connections
            .iter()
            .map(|connection| {
                let mut profile = PostgresConnectionProfile::new(
                    connection.id.clone(),
                    connection.name.clone(),
                    connection.host.clone(),
                    connection.database.clone(),
                    connection.user.clone(),
                );
                profile.port = connection.port;
                profile.ssl = connection.ssl.into();
                profile.reject_unauthorized = connection.reject_unauthorized;
                profile.ca_certificate_path = connection.ca_certificate_path.clone();
                (connection.id.clone(), profile)
            })
            .collect();

        Self {
            workspace: WorkspaceState::new(selected_connection),
            connections,
            connection_store,
            credential_store,
            postgres_provider: PostgresProvider::new(),
            connection_profiles,
            connection_generation: 0,
            connection_status: ConnectionStatus::Disconnected,
            server_version: None,
            notice: storage_notice,
            connection_editor: None,
            object_explorer: None,
            pending_delete: None,
            query_input: cx.new(|cx| TextareaState::new(window, cx).default_value("SELECT 1;")),
            query_dock_tab: QueryDockTab::Query,
            query_result: None,
            result_column_widths: Rc::new(Vec::new()),
            query_running: false,
            query_generation: 0,
            connection_test_running: false,
            connection_test_generation: 0,
            write_confirmation_sql: None,
            table_search: cx.new(|cx| InputState::new(window, cx).placeholder("Search objects...")),
            global_search: cx
                .new(|cx| InputState::new(window, cx).placeholder("Search tables by name")),
            table_schema_filter: None,
            table_type_filter: None,
            table_page_loading: false,
            table_page_generation: 0,
            table_page_offset: 0,
            table_page_limit: 100,
            table_page_has_next: false,
            table_page_cursors: vec![None],
            selected_table: None,
            table_data_offset: 0,
            table_data_limit: 25,
            table_data_has_next: false,
            table_data_sort: None,
            table_data_filter_input: cx
                .new(|cx| InputState::new(window, cx).placeholder("Filter loaded rows")),
            table_data_filter: None,
            table_data_filter_column: None,
            table_column_filters: Vec::new(),
            table_filter_input: cx
                .new(|cx| InputState::new(window, cx).placeholder("Filter value")),
            table_filter_editor_column: None,
            table_filter_editor_operator: TableFilterOperator::Contains,
            active_cell_edit: None,
            cell_update_reload_pending: false,
            table_data_empty_filter: None,
            filtered_table_data_rows: None,
            table_data_filter_pending: false,
            filter_cache_generation: Arc::new(AtomicU64::new(0)),
            hidden_table_data_columns: HashSet::new(),
            table_result_scroll: UniformListScrollHandle::new(),
            table_result_horizontal_scroll: gpui_kit::ScrollHandle::new(),
            table_sidebar_visible: true,
        }
    }

    pub(crate) fn open_connection_editor(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        connections::open_editor(self, window, cx);
    }

    pub(crate) fn edit_connection(
        &mut self,
        connection_id: &ConnectionId,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        connections::edit_editor(self, connection_id, window, cx);
    }

    pub(crate) fn close_connection_editor(&mut self, cx: &mut Context<Self>) {
        connections::close_editor(self, cx);
    }

    fn set_ssl_mode(&mut self, mode: PostgresSslMode, cx: &mut Context<Self>) {
        connections::set_ssl_mode(self, mode, cx);
    }

    fn test_connection(&mut self, cx: &mut Context<Self>) {
        connections::test_connection(self, cx);
    }

    pub(crate) fn save_connection(&mut self, cx: &mut Context<Self>) {
        connections::save(self, cx);
    }

    pub(crate) fn connect_connection(
        &mut self,
        connection_id: &ConnectionId,
        cx: &mut Context<Self>,
    ) {
        metadata::connect(self, connection_id, cx);
    }

    pub(crate) fn execute_query(&mut self, cx: &mut Context<Self>) {
        query::execute_read_query(self, cx);
    }

    pub(crate) fn execute_write_query(&mut self, cx: &mut Context<Self>) {
        query::execute_write_query(self, cx);
    }

    pub(crate) fn format_query(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let formatted = self.query_input.read(cx).value().trim().to_owned();
        self.query_input.update(cx, |state, cx| {
            state.set_value(formatted, window, cx);
        });
        cx.notify();
    }

    pub(crate) fn set_query_dock_tab(
        &mut self,
        tab: QueryDockTab,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.query_dock_tab = tab;
        if tab == QueryDockTab::Query {
            self.query_input.read(cx).focus_handle(cx).focus(window, cx);
        }
        cx.notify();
    }

    pub(crate) fn edit_selected_connection(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(connection_id) = self.workspace.selected_connection.clone() {
            self.edit_connection(&connection_id, window, cx);
        }
    }

    pub(crate) fn preview_table(
        &mut self,
        schema: &str,
        table: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if window.inner_window_bounds().get_bounds().size.width < px(760.) {
            self.table_sidebar_visible = false;
        }
        self.table_data_offset = 0;
        self.table_data_limit = 25;
        self.table_data_has_next = false;
        query::preview_table_page(self, schema, table, window, cx);
    }

    pub(crate) fn refresh_selected_table(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some((schema, table)) = self.selected_table.clone() {
            query::preview_table_page(self, &schema, &table, window, cx);
        }
    }

    pub(crate) fn previous_table_data_page(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let page_cursor = self.table_data_cursor(TableDataCursorDirection::Before);
        self.table_data_offset = self.table_data_offset.saturating_sub(self.table_data_limit);
        if let Some((schema, table)) = self.selected_table.clone() {
            query::preview_table_page_with_cursor(self, &schema, &table, window, cx, page_cursor);
        }
    }

    pub(crate) fn next_table_data_page(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.table_data_has_next {
            return;
        }
        let page_cursor = self.table_data_cursor(TableDataCursorDirection::After);
        self.table_data_offset = self.table_data_offset.saturating_add(self.table_data_limit);
        if let Some((schema, table)) = self.selected_table.clone() {
            query::preview_table_page_with_cursor(self, &schema, &table, window, cx, page_cursor);
        }
    }

    fn table_data_cursor(&self, direction: TableDataCursorDirection) -> Option<TableDataCursor> {
        let (schema, table) = self.selected_table.as_ref()?;
        let (sort_column, _) = self.table_data_sort.as_ref()?;
        let result = self.query_result()?;
        let editable = result.editable.as_ref()?;
        if editable.schema != *schema
            || editable.table != *table
            || editable.primary_key_columns.first().map(String::as_str)
                != Some(sort_column.as_str())
        {
            return None;
        }
        let row = match direction {
            TableDataCursorDirection::After => result.rows.last(),
            TableDataCursorDirection::Before => result.rows.first(),
        }?;
        let values = editable
            .primary_key_columns
            .iter()
            .map(|key| {
                let column_index = result.columns.iter().position(|column| column == key)?;
                row.get(column_index).cloned()
            })
            .collect::<Option<Vec<_>>>()?;
        Some(TableDataCursor { values, direction })
    }

    pub(crate) fn set_table_data_limit(
        &mut self,
        limit: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.table_data_limit = limit.clamp(1, 100);
        self.table_data_offset = 0;
        if let Some((schema, table)) = self.selected_table.clone() {
            query::preview_table_page(self, &schema, &table, window, cx);
        }
    }

    pub(crate) fn set_table_data_sort(
        &mut self,
        column: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let descending = match self.table_data_sort.as_ref() {
            Some((current, descending)) if current == &column => !descending,
            _ => false,
        };
        self.table_data_sort = Some((column, descending));
        self.table_data_offset = 0;
        if let Some((schema, table)) = self.selected_table.clone() {
            query::preview_table_page(self, &schema, &table, window, cx);
        }
    }

    pub(crate) fn apply_table_data_filter(&mut self, cx: &mut Context<Self>) {
        let filter = self
            .table_data_filter_input
            .read(cx)
            .value()
            .trim()
            .to_owned();
        self.table_data_filter = (!filter.is_empty()).then_some(filter);
        self.refresh_table_data_filter_cache(cx);
        cx.notify();
    }

    pub(crate) fn set_table_data_filter_column(
        &mut self,
        column: Option<String>,
        cx: &mut Context<Self>,
    ) {
        self.table_data_filter_column = column;
        self.refresh_table_data_filter_cache(cx);
        cx.notify();
    }

    #[cfg(test)]
    pub(crate) fn table_column_filters(&self) -> &[TableColumnFilter] {
        &self.table_column_filters
    }

    pub(crate) fn active_cell_edit(&self) -> Option<ActiveCellEditView> {
        self.active_cell_edit
            .as_ref()
            .map(|edit| ActiveCellEditView {
                row_index: edit.row_index,
                column_index: edit.column_index,
                input: edit.input.clone(),
                set_null: edit.set_null,
                enum_values: edit.enum_values.clone(),
                enum_value: edit.enum_value.clone(),
            })
    }

    #[cfg(test)]
    pub(crate) fn table_filter_editor(
        &self,
    ) -> (
        Option<String>,
        TableFilterOperator,
        gpui_kit::Entity<InputState>,
    ) {
        (
            self.table_filter_editor_column.clone(),
            self.table_filter_editor_operator,
            self.table_filter_input.clone(),
        )
    }

    pub(crate) fn begin_table_cell_edit(
        &mut self,
        row_index: usize,
        column_index: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        query::begin_table_cell_edit(self, row_index, column_index, window, cx);
    }

    pub(crate) fn save_table_cell_edit(&mut self, cx: &mut Context<Self>) {
        query::save_table_cell_edit(self, cx);
    }

    pub(crate) fn cancel_table_cell_edit(&mut self, cx: &mut Context<Self>) {
        self.active_cell_edit = None;
        cx.notify();
    }

    pub(crate) fn set_cell_edit_null(&mut self, set_null: bool, cx: &mut Context<Self>) {
        if let Some(edit) = &mut self.active_cell_edit {
            edit.set_null = set_null;
            cx.notify();
        }
    }

    pub(crate) fn select_table_cell_enum_value(
        &mut self,
        value: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        query::select_table_cell_enum_value(self, value, window, cx);
    }

    pub(crate) fn set_table_filter_input(
        &mut self,
        value: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.table_filter_input.update(cx, |input, cx| {
            input.set_value(value, window, cx);
        });
    }

    pub(crate) fn open_table_filter_editor(
        &mut self,
        column: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let existing = self
            .table_column_filters
            .iter()
            .find(|filter| filter.column == column);
        let default_operator = self
            .query_result
            .as_ref()
            .and_then(|result| {
                result
                    .columns
                    .iter()
                    .position(|name| name == &column)
                    .and_then(|index| result.column_types.get(index))
            })
            .is_some_and(|type_name| {
                matches!(
                    type_name.to_ascii_lowercase().as_str(),
                    "text" | "varchar" | "bpchar" | "name" | "citext"
                )
            });
        self.table_filter_editor_operator =
            existing
                .map(|filter| filter.operator)
                .unwrap_or(if default_operator {
                    TableFilterOperator::Contains
                } else {
                    TableFilterOperator::Equals
                });
        let value = existing
            .and_then(|filter| filter.value.clone())
            .unwrap_or_default();
        self.table_filter_editor_column = Some(column);
        self.table_filter_input.update(cx, |input, cx| {
            input.set_value(value, window, cx);
        });
        cx.notify();
    }

    pub(crate) fn set_table_filter_editor_operator(
        &mut self,
        operator: TableFilterOperator,
        cx: &mut Context<Self>,
    ) {
        self.table_filter_editor_operator = operator;
        cx.notify();
    }

    pub(crate) fn apply_table_column_filter(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(column) = self.table_filter_editor_column.clone() else {
            return;
        };
        let operator = self.table_filter_editor_operator;
        let value = match operator {
            TableFilterOperator::IsNull | TableFilterOperator::IsNotNull => None,
            _ => Some(self.table_filter_input.read(cx).value().to_string()),
        };
        self.table_column_filters
            .retain(|filter| filter.column != column);
        self.table_column_filters.push(TableColumnFilter {
            column,
            operator,
            value,
        });
        self.table_data_offset = 0;
        self.active_cell_edit = None;
        self.refresh_table_data_filter_cache(cx);
        if let Some((schema, table)) = self.selected_table.clone() {
            query::preview_table_page(self, &schema, &table, window, cx);
        } else {
            cx.notify();
        }
    }

    pub(crate) fn clear_table_column_filter(
        &mut self,
        column: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.table_column_filters
            .retain(|filter| filter.column != column);
        self.table_data_offset = 0;
        self.active_cell_edit = None;
        if let Some((schema, table)) = self.selected_table.clone() {
            query::preview_table_page(self, &schema, &table, window, cx);
        } else {
            cx.notify();
        }
    }

    pub(crate) fn set_table_data_empty_filter(
        &mut self,
        filter: Option<(String, bool)>,
        cx: &mut Context<Self>,
    ) {
        self.table_data_empty_filter = filter;
        self.refresh_table_data_filter_cache(cx);
        cx.notify();
    }

    pub(crate) fn refresh_table_data_filter_cache(&mut self, cx: &mut Context<Self>) {
        let generation = self
            .filter_cache_generation
            .fetch_add(1, Ordering::AcqRel)
            .wrapping_add(1);
        let filters_active =
            self.table_data_filter.is_some() || self.table_data_empty_filter.is_some();
        let Some(result) = self.query_result.clone().filter(|_| filters_active) else {
            self.filtered_table_data_rows = None;
            self.table_data_filter_pending = false;
            return;
        };

        let filter = self.table_data_filter.clone();
        let filter_column = self.table_data_filter_column.clone();
        let empty_filter = self.table_data_empty_filter.clone();
        let generation_guard = self.filter_cache_generation.clone();
        self.filtered_table_data_rows = None;
        self.table_data_filter_pending = true;

        let task = cx.background_spawn(async move {
            homepage::query::matching_row_indices_with_cancellation(
                &result,
                filter.as_deref(),
                filter_column.as_deref(),
                empty_filter.as_ref(),
                usize::MAX,
                || generation_guard.load(Ordering::Acquire) == generation,
            )
        });
        cx.spawn(async move |this, cx| {
            let filtered_rows = task.await;
            this.update(cx, |workspace, cx| {
                if workspace.filter_cache_generation.load(Ordering::Acquire) != generation {
                    return;
                }
                workspace.filtered_table_data_rows = filtered_rows.map(Rc::new);
                workspace.table_data_filter_pending = false;
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    pub(crate) fn toggle_table_data_column(
        &mut self,
        column: &str,
        columns: &[String],
        cx: &mut Context<Self>,
    ) {
        let is_hidden = self.hidden_table_data_columns.contains(column);
        let visible_count = columns
            .iter()
            .filter(|name| !self.hidden_table_data_columns.contains(*name))
            .count();
        if !is_hidden && visible_count <= 1 {
            return;
        }
        if is_hidden {
            self.hidden_table_data_columns.remove(column);
        } else {
            self.hidden_table_data_columns.insert(column.to_owned());
        }
        cx.notify();
    }

    pub(crate) fn clear_table_data_filter(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.table_data_filter_input.update(cx, |state, cx| {
            state.set_value(String::new(), window, cx);
        });
        self.table_data_filter = None;
        self.refresh_table_data_filter_cache(cx);
        cx.notify();
    }

    pub(crate) fn prepare_query(
        &mut self,
        sql: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.query_input.update(cx, |state, cx| {
            state.set_value(sql, window, cx);
        });
        self.write_confirmation_sql = None;
        self.notice = Some(Notice::warning(
            "Review generated SQL",
            "Use Run write twice to confirm this data change.",
        ));
        cx.notify();
    }

    pub(crate) fn request_delete_connection(
        &mut self,
        connection_id: &ConnectionId,
        cx: &mut Context<Self>,
    ) {
        connections::request_delete(self, connection_id, cx);
    }

    pub(crate) fn refresh_database_objects(&mut self, cx: &mut Context<Self>) {
        metadata::refresh(self, cx);
    }

    pub(crate) fn apply_table_filter(&mut self, cx: &mut Context<DatabaseWorkspace>) {
        self.table_page_offset = 0;
        self.table_page_cursors = vec![None];
        metadata::refresh_table_list(self, cx);
    }

    pub(crate) fn apply_global_table_search(
        &mut self,
        window: &mut Window,
        cx: &mut Context<DatabaseWorkspace>,
    ) {
        let value = self.global_search.read(cx).value().to_string();
        self.table_search.update(cx, |state, cx| {
            state.set_value(value, window, cx);
        });
        self.apply_table_filter(cx);
    }

    pub(crate) fn toggle_table_schema(&mut self, schema: &str, cx: &mut Context<Self>) {
        if let Some(explorer) = &mut self.object_explorer {
            explorer.toggle_schema(schema, cx);
        }
    }

    pub(crate) fn toggle_table_category(
        &mut self,
        schema: &str,
        category: &str,
        default_expanded: bool,
        cx: &mut Context<Self>,
    ) {
        if let Some(explorer) = &mut self.object_explorer {
            explorer.toggle_category(schema, category, default_expanded, cx);
        }
    }

    pub(crate) fn toggle_table_sidebar(&mut self, cx: &mut Context<Self>) {
        self.table_sidebar_visible = !self.table_sidebar_visible;
        cx.notify();
    }

    pub(crate) fn set_table_schema_filter(
        &mut self,
        schema: Option<String>,
        cx: &mut Context<Self>,
    ) {
        self.table_schema_filter = schema;
        self.table_page_offset = 0;
        self.table_page_cursors = vec![None];
        metadata::refresh_table_list(self, cx);
    }

    pub(crate) fn set_table_type_filter(
        &mut self,
        relation_type: Option<TableRelationType>,
        cx: &mut Context<Self>,
    ) {
        self.table_type_filter = relation_type;
        self.table_page_offset = 0;
        self.table_page_cursors = vec![None];
        metadata::refresh_table_list(self, cx);
    }

    pub(crate) fn previous_table_page(&mut self, cx: &mut Context<Self>) {
        self.table_page_offset = self.table_page_offset.saturating_sub(self.table_page_limit);
        metadata::refresh_table_list(self, cx);
    }

    pub(crate) fn next_table_page(&mut self, cx: &mut Context<Self>) {
        if self.table_page_has_next {
            self.table_page_offset = self.table_page_offset.saturating_add(self.table_page_limit);
            metadata::refresh_table_list(self, cx);
        }
    }

    pub(crate) fn dismiss_notice(&mut self, cx: &mut Context<Self>) {
        self.notice = None;
        cx.notify();
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum QueryDockTab {
    #[default]
    Query,
    Results,
}

fn ssl_menu_item(
    label: &'static str,
    description: &'static str,
    mode: PostgresSslMode,
    selected: PostgresSslMode,
    workspace: gpui_kit::Entity<DatabaseWorkspace>,
) -> PopupMenuItem {
    PopupMenuItem::new(format!("{label}  ·  {description}"))
        .checked(mode == selected)
        .on_click(move |_, _, cx| {
            workspace.update(cx, |workspace, cx| {
                workspace.set_ssl_mode(mode, cx);
            });
        })
}

fn ssl_label(mode: PostgresSslMode) -> String {
    match mode {
        PostgresSslMode::Disable => "disable",
        PostgresSslMode::Prefer => "prefer",
        PostgresSslMode::Require => "require",
    }
    .into()
}

fn ssl_description(mode: PostgresSslMode) -> &'static str {
    match mode {
        PostgresSslMode::Disable => "Plaintext connection; use only on a trusted network",
        PostgresSslMode::Prefer => {
            "TLS when available; plaintext fallback only when the server declines TLS"
        }
        PostgresSslMode::Require => "Trusted TLS connection required",
    }
}

impl Render for DatabaseWorkspace {
    fn render(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> impl gpui_kit::IntoElement {
        #[cfg(feature = "perf-overlay")]
        let render_started = std::time::Instant::now();
        let connected = self.workspace.selected_connection.is_some()
            && matches!(self.connection_status, ConnectionStatus::Connected);
        let compact_layout = window.inner_window_bounds().get_bounds().size.width < px(980.);
        let selected_connection = self.workspace.selected_connection.as_ref().and_then(|id| {
            self.connections
                .iter()
                .find(|connection| &connection.id == id)
        });

        let element = v_flex()
            .size_full()
            .bg(cx.theme().background)
            .text_color(cx.theme().foreground)
            .child(TitleBar::new().child(homepage::render_titlebar(
                cx,
                selected_connection,
                &self.connections,
                connected,
                self.server_version.as_deref(),
                &self.global_search,
            )))
            .child(if let Some(explorer) = &self.object_explorer {
                let sidebar = explorer
                    .render(
                        cx,
                        &self.table_search,
                        self.table_schema_filter.as_deref(),
                        self.table_type_filter,
                        self.table_page_loading,
                        self.table_page_offset,
                        self.table_page_has_next,
                        self.selected_table.as_ref(),
                        compact_layout,
                    )
                    .into_any_element();
                let workspace = homepage::render_workspace(
                    cx,
                    self.notice.as_ref(),
                    self.selected_table.as_ref(),
                    &self.query_input,
                    self.query_result.as_deref(),
                    self.result_column_widths.clone(),
                    self.query_running,
                    self.write_confirmation_sql.is_some(),
                    self.table_data_offset,
                    self.table_data_limit,
                    self.table_data_has_next,
                    self.table_data_sort.as_ref(),
                    &self.table_data_filter_input,
                    self.table_data_filter.as_deref(),
                    self.table_data_filter_column.clone(),
                    self.table_data_empty_filter.clone(),
                    self.filtered_table_data_rows.clone(),
                    self.table_data_filter_pending,
                    self.hidden_table_data_columns.clone(),
                    &self.table_result_scroll,
                    &self.table_result_horizontal_scroll,
                    compact_layout,
                    self.query_dock_tab,
                    &self.table_column_filters,
                    self.active_cell_edit(),
                    Some((
                        self.table_filter_editor_column.clone(),
                        self.table_filter_editor_operator,
                        self.table_filter_input.clone(),
                    )),
                )
                .into_any_element();
                if compact_layout {
                    if self.table_sidebar_visible {
                        h_flex().size_full().child(sidebar).into_any_element()
                    } else {
                        h_flex().size_full().child(workspace).into_any_element()
                    }
                } else {
                    let navigation = navigation::render(
                        cx,
                        connected,
                        selected_connection.map(|connection| connection.name.as_str()),
                        self.table_sidebar_visible,
                        self.query_dock_tab,
                    )
                    .into_any_element();
                    let mut layout = h_flex().size_full().child(navigation);
                    if self.table_sidebar_visible {
                        layout = layout.child(sidebar);
                    }
                    layout.child(workspace).into_any_element()
                }
            } else {
                homepage::render(
                    cx,
                    homepage::HomepageView {
                        connections: &self.connections,
                        selected_connection,
                        connection_editor: self.connection_editor.as_ref(),
                        connection_test_running: self.connection_test_running,
                        pending_delete: self.pending_delete.as_ref(),
                        notice: self.notice.as_ref(),
                        connected,
                        server_version: self.server_version.as_deref(),
                        query_input: &self.query_input,
                        query_result: self.query_result.as_deref(),
                        result_column_widths: self.result_column_widths.clone(),
                        query_running: self.query_running,
                        write_confirmation_pending: self.write_confirmation_sql.is_some(),
                    },
                )
                .into_any_element()
            })
            .children(Root::render_dialog_layer(window, cx))
            .children(Root::render_sheet_layer(window, cx))
            .children(Root::render_notification_layer(window, cx));
        #[cfg(feature = "perf-overlay")]
        if std::env::var_os("TABLEX_DEBUG_FRAME_OVERLAY").is_some()
            && !STARTUP_RENDER_TIMING_REPORTED.swap(true, Ordering::Relaxed)
        {
            eprintln!(
                "[tableX perf] DatabaseWorkspace::render element build: {:.2} ms",
                render_started.elapsed().as_secs_f64() * 1000.0
            );
        }
        element
    }
}

#[cfg(test)]
#[path = "../../tests/unit/ui/database_workspace.rs"]
mod tests;
