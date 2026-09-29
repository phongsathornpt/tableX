use super::DatabaseWorkspace;
use crate::domain::connection::ConnectionId;
use crate::domain::database_object::TableCursor;
use crate::infrastructure::postgres::TableListRequest;
use crate::infrastructure::{DatabaseError, PostgresInspection};
use crate::ui::{Notice, ObjectExplorer};
use gpui_kit::{AppContext as _, Context};

pub(crate) type InspectionResult = Result<PostgresInspection, DatabaseError>;
pub(crate) type TableListResult = Result<crate::domain::database_object::TablePage, DatabaseError>;

pub(crate) fn connect(
    workspace: &mut DatabaseWorkspace,
    connection_id: &ConnectionId,
    cx: &mut Context<DatabaseWorkspace>,
) {
    let Some(profile) = workspace.connection_profiles.get(connection_id).cloned() else {
        workspace.connection_status = crate::domain::connection::ConnectionStatus::Error(
            "Connection details are missing".into(),
        );
        workspace.notice = Some(Notice::error(
            "Connection unavailable",
            "The saved connection details are missing.",
        ));
        cx.notify();
        return;
    };

    workspace.workspace.selected_connection = Some(connection_id.clone());
    workspace.connection_generation = workspace.connection_generation.wrapping_add(1);
    let connection_generation = workspace.connection_generation;
    workspace.connection_status = crate::domain::connection::ConnectionStatus::Connecting;
    workspace.notice = Some(Notice::info(format!("Connecting to {}...", profile.name)));
    workspace.pending_delete = None;
    workspace.object_explorer = None;
    workspace.selected_table = None;
    workspace.active_database = None;
    workspace.available_databases = Vec::new();
    workspace.database_switch_loading = false;
    workspace.table_sidebar_visible = true;
    workspace.table_page_offset = 0;
    workspace.table_page_has_next = false;
    workspace.table_page_cursors = vec![None];
    workspace.table_page_generation = workspace.table_page_generation.wrapping_add(1);
    let table_page_generation = workspace.table_page_generation;
    workspace.table_page_loading = true;
    workspace.query_result = None;
    workspace.result_column_widths = Default::default();
    workspace.table_preview_loading = false;
    workspace.table_preview_error = None;
    workspace.refresh_table_data_filter_cache(cx);
    workspace.query_generation = workspace.query_generation.wrapping_add(1);
    workspace.query_running = false;
    workspace.write_confirmation_sql = None;
    cx.notify();

    workspace.postgres_provider.invalidate_metadata_session();
    let provider = workspace.postgres_provider.clone();
    let connection_id = connection_id.clone();
    let credential_store = workspace.credential_store;
    let request = table_list_request(workspace, cx, 0);
    let task = cx.background_spawn(async move {
        let mut profile = profile;
        if profile.password.is_none() {
            profile.password = credential_store.load(&profile.id)?;
        }
        provider.inspect(profile, request)
    });
    cx.spawn(async move |this, cx| {
        let result = task.await;
        this.update(cx, |workspace, cx| {
            finish_connection(
                workspace,
                &connection_id,
                connection_generation,
                table_page_generation,
                result,
                cx,
            );
        })
        .ok();
    })
    .detach();
}

pub(crate) fn finish_connection(
    workspace: &mut DatabaseWorkspace,
    connection_id: &ConnectionId,
    connection_generation: u64,
    table_page_generation: u64,
    result: InspectionResult,
    cx: &mut Context<DatabaseWorkspace>,
) {
    if workspace.workspace.selected_connection.as_ref() != Some(connection_id)
        || workspace.connection_generation != connection_generation
    {
        return;
    }

    match result {
        Ok(inspection) => {
            workspace.connection_status = crate::domain::connection::ConnectionStatus::Connected;
            workspace.database_switch_loading = false;
            let success_notice = Notice::success(format!(
                "Connected to PostgreSQL {} as {}",
                inspection.server.version, inspection.server.user
            ));
            workspace.notice = apply_inspection(workspace, inspection, table_page_generation, cx)
                .map(table_page_failure_notice)
                .or(Some(success_notice));
        }
        Err(error) => {
            workspace.table_page_loading = false;
            workspace.database_switch_loading = false;
            workspace.connection_status =
                crate::domain::connection::ConnectionStatus::Error(error.message.clone());
            workspace.server_version = None;
            workspace.active_database = None;
            workspace.available_databases = Vec::new();
            workspace.notice = Some(
                Notice::error(
                    "Connection failed",
                    "Could not establish a PostgreSQL connection.",
                )
                .with_detail(error.message),
            );
        }
    }
    cx.notify();
}

pub(crate) fn refresh_table_list(
    workspace: &mut DatabaseWorkspace,
    cx: &mut Context<DatabaseWorkspace>,
) {
    let Some(connection_id) = workspace.workspace.selected_connection.clone() else {
        return;
    };
    let Some(profile) = workspace.active_profile(&connection_id) else {
        return;
    };

    workspace.table_page_generation = workspace.table_page_generation.wrapping_add(1);
    let generation = workspace.table_page_generation;
    workspace.table_page_loading = true;
    let request = table_list_request(workspace, cx, workspace.table_page_offset);
    cx.notify();

    let credential_store = workspace.credential_store;
    let provider = workspace.postgres_provider.clone();
    let task = cx.background_spawn(async move {
        let mut profile = profile;
        if profile.password.is_none() {
            profile.password = credential_store.load(&profile.id)?;
        }
        provider.list_tables(profile, request)
    });
    cx.spawn(async move |this, cx| {
        let result = task.await;
        this.update(cx, |workspace, cx| {
            finish_table_list(workspace, &connection_id, generation, result, cx);
        })
        .ok();
    })
    .detach();
}

pub(crate) fn finish_table_list(
    workspace: &mut DatabaseWorkspace,
    connection_id: &ConnectionId,
    generation: u64,
    result: TableListResult,
    cx: &mut Context<DatabaseWorkspace>,
) {
    if workspace.workspace.selected_connection.as_ref() != Some(connection_id)
        || workspace.table_page_generation != generation
    {
        return;
    }
    workspace.table_page_loading = false;
    match result {
        Ok(page) => {
            workspace.table_page_offset = page.offset;
            workspace.table_page_limit = page.limit;
            workspace.table_page_has_next = page.has_next;
            remember_table_page_cursor(workspace, &page);
            if let Some(explorer) = &mut workspace.object_explorer {
                explorer.set_tables(page.tables, cx);
            }
        }
        Err(error) => {
            workspace.table_page_has_next = false;
            workspace.notice = Some(
                Notice::error("Table list failed", "Could not load the table explorer.")
                    .with_detail(error.message),
            );
        }
    }
    cx.notify();
}

pub(crate) fn refresh(workspace: &mut DatabaseWorkspace, cx: &mut Context<DatabaseWorkspace>) {
    let Some(connection_id) = workspace.workspace.selected_connection.clone() else {
        workspace.notice = Some(Notice::warning(
            "No database connected",
            "Select a connection before refreshing metadata.",
        ));
        cx.notify();
        return;
    };
    let Some(profile) = workspace.active_profile(&connection_id) else {
        workspace.notice = Some(Notice::error(
            "Connection unavailable",
            "The saved connection details are missing.",
        ));
        cx.notify();
        return;
    };

    workspace.connection_generation = workspace.connection_generation.wrapping_add(1);
    let connection_generation = workspace.connection_generation;
    workspace.table_page_generation = workspace.table_page_generation.wrapping_add(1);
    let table_page_generation = workspace.table_page_generation;
    workspace.table_page_loading = true;
    let request = table_list_request(workspace, cx, workspace.table_page_offset);
    workspace.notice = Some(Notice::info("Refreshing database metadata..."));
    cx.notify();

    let credential_store = workspace.credential_store;
    let provider = workspace.postgres_provider.clone();
    let task = cx.background_spawn(async move {
        let mut profile = profile;
        if profile.password.is_none() {
            profile.password = credential_store.load(&profile.id)?;
        }
        provider.inspect(profile, request)
    });
    cx.spawn(async move |this, cx| {
        let result = task.await;
        this.update(cx, |workspace, cx| {
            finish_refresh(
                workspace,
                &connection_id,
                connection_generation,
                table_page_generation,
                result,
                cx,
            );
        })
        .ok();
    })
    .detach();
}

pub(crate) fn finish_refresh(
    workspace: &mut DatabaseWorkspace,
    connection_id: &ConnectionId,
    connection_generation: u64,
    table_page_generation: u64,
    result: InspectionResult,
    cx: &mut Context<DatabaseWorkspace>,
) {
    if workspace.workspace.selected_connection.as_ref() != Some(connection_id)
        || workspace.connection_generation != connection_generation
    {
        return;
    }
    match result {
        Ok(inspection) => {
            workspace.notice = apply_inspection(workspace, inspection, table_page_generation, cx)
                .map(table_page_failure_notice)
                .or_else(|| Some(Notice::success("Database metadata refreshed")));
        }
        Err(error) => {
            workspace.table_page_loading = false;
            workspace.notice = Some(
                Notice::error(
                    "Metadata refresh failed",
                    "Could not refresh database objects.",
                )
                .with_detail(error.message),
            );
        }
    }
    cx.notify();
}

fn table_list_request(
    workspace: &DatabaseWorkspace,
    cx: &Context<DatabaseWorkspace>,
    offset: usize,
) -> TableListRequest {
    TableListRequest {
        search: workspace.table_search.read(cx).value().to_string(),
        schema: workspace.table_schema_filter.clone(),
        relation_type: workspace.table_type_filter,
        limit: workspace.table_page_limit,
        offset,
        after: table_cursor_for_offset(
            &workspace.table_page_cursors,
            offset,
            workspace.table_page_limit,
        ),
    }
}

fn table_cursor_for_offset(
    cursors: &[Option<TableCursor>],
    offset: usize,
    limit: usize,
) -> Option<TableCursor> {
    cursors.get(offset / limit.max(1)).cloned().flatten()
}

fn remember_table_page_cursor(
    workspace: &mut DatabaseWorkspace,
    page: &crate::domain::database_object::TablePage,
) {
    let page_index = page.offset / page.limit.max(1);
    remember_page_cursor(
        &mut workspace.table_page_cursors,
        page_index,
        page.has_next,
        page.next_cursor.clone(),
    );
}

fn remember_page_cursor(
    cursors: &mut Vec<Option<TableCursor>>,
    page_index: usize,
    has_next: bool,
    next_cursor: Option<TableCursor>,
) {
    cursors.truncate(page_index + 1);
    if has_next && let Some(cursor) = next_cursor {
        cursors.push(Some(cursor));
    }
}

fn apply_inspection(
    workspace: &mut DatabaseWorkspace,
    inspection: PostgresInspection,
    table_page_generation: u64,
    cx: &mut Context<DatabaseWorkspace>,
) -> Option<DatabaseError> {
    workspace.server_version = Some(inspection.server.version.to_string());
    workspace.active_database = Some(inspection.server.database);
    if !inspection.databases.is_empty() {
        workspace.available_databases = inspection.databases;
    }
    if let Some(explorer) = &mut workspace.object_explorer {
        explorer.set_schemas(inspection.schemas);
    } else {
        workspace.object_explorer = Some(ObjectExplorer::new(inspection.schemas));
    }
    if workspace.table_page_generation != table_page_generation {
        return None;
    }
    workspace.table_page_loading = false;
    match inspection.table_page {
        Some(Ok(page)) => {
            workspace.table_page_offset = page.offset;
            workspace.table_page_limit = page.limit;
            workspace.table_page_has_next = page.has_next;
            remember_table_page_cursor(workspace, &page);
            if let Some(explorer) = &mut workspace.object_explorer {
                explorer.set_tables(page.tables, cx);
            }
            None
        }
        Some(Err(error)) => {
            workspace.table_page_has_next = false;
            Some(error)
        }
        None => None,
    }
}

pub(crate) fn switch_database(
    workspace: &mut DatabaseWorkspace,
    target_database: &str,
    cx: &mut Context<DatabaseWorkspace>,
) {
    if workspace.active_database.as_deref() == Some(target_database) {
        return;
    }
    let Some(connection_id) = workspace.workspace.selected_connection.clone() else {
        return;
    };
    let Some(mut profile) = workspace.connection_profiles.get(&connection_id).cloned() else {
        return;
    };

    let previous_database = workspace.active_database.clone();
    let target_database = target_database.to_owned();

    profile.database = target_database.clone();

    // Session-based switch: clear active table, previews, and cell edits, but keep the current SQL query draft
    workspace.selected_table = None;
    workspace.query_result = None;
    workspace.result_column_widths = Default::default();
    workspace.table_preview_loading = false;
    workspace.table_preview_error = None;
    workspace.table_column_filters.clear();
    workspace.table_data_filter = None;
    workspace.table_data_filter_column = None;
    workspace.table_data_empty_filter = None;
    workspace.filtered_table_data_rows = None;
    workspace.active_cell_edit = None;

    workspace.database_switch_loading = true;
    workspace.table_page_loading = true;
    workspace.table_page_offset = 0;
    workspace.table_page_has_next = false;
    workspace.table_page_cursors = vec![None];
    workspace.connection_generation = workspace.connection_generation.wrapping_add(1);
    let connection_generation = workspace.connection_generation;
    workspace.table_page_generation = workspace.table_page_generation.wrapping_add(1);
    let table_page_generation = workspace.table_page_generation;

    workspace.notice = Some(Notice::info(format!(
        "Switching to database {target_database}..."
    )));
    cx.notify();

    let provider = workspace.postgres_provider.clone();
    let credential_store = workspace.credential_store;
    let request = table_list_request(workspace, cx, 0);

    let task = cx.background_spawn(async move {
        let mut profile = profile;
        if profile.password.is_none() {
            profile.password = credential_store.load(&profile.id)?;
        }
        provider.inspect(profile, request)
    });

    cx.spawn(async move |this, cx| {
        let result = task.await;
        this.update(cx, |workspace, cx| {
            finish_database_switch(
                workspace,
                &connection_id,
                connection_generation,
                table_page_generation,
                previous_database,
                target_database,
                result,
                cx,
            );
        })
        .ok();
    })
    .detach();
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn finish_database_switch(
    workspace: &mut DatabaseWorkspace,
    connection_id: &ConnectionId,
    connection_generation: u64,
    table_page_generation: u64,
    previous_database: Option<String>,
    target_database: String,
    result: InspectionResult,
    cx: &mut Context<DatabaseWorkspace>,
) {
    if workspace.workspace.selected_connection.as_ref() != Some(connection_id)
        || workspace.connection_generation != connection_generation
    {
        return;
    }

    workspace.database_switch_loading = false;
    match result {
        Ok(inspection) => {
            let success_notice = Notice::success(format!(
                "Switched to database {}",
                inspection.server.database
            ));
            workspace.notice = apply_inspection(workspace, inspection, table_page_generation, cx)
                .map(table_page_failure_notice)
                .or(Some(success_notice));
        }
        Err(error) => {
            workspace.table_page_loading = false;
            workspace.active_database = previous_database.clone();
            let prev_name = previous_database.as_deref().unwrap_or("previous database");
            workspace.notice = Some(
                Notice::error(
                    format!("Failed to switch to database {target_database}"),
                    format!("Could not connect to {target_database}. Remaining on {prev_name}."),
                )
                .with_detail(error.message),
            );
        }
    }
    cx.notify();
}

pub(crate) fn refresh_databases(
    workspace: &mut DatabaseWorkspace,
    cx: &mut Context<DatabaseWorkspace>,
) {
    let Some(connection_id) = workspace.workspace.selected_connection.clone() else {
        return;
    };
    let Some(profile) = workspace.active_profile(&connection_id) else {
        return;
    };

    let provider = workspace.postgres_provider.clone();
    let credential_store = workspace.credential_store;

    let task = cx.background_spawn(async move {
        let mut profile = profile;
        if profile.password.is_none() {
            profile.password = credential_store.load(&profile.id)?;
        }
        provider.list_databases(profile)
    });

    cx.spawn(async move |this, cx| {
        let result = task.await;
        this.update(cx, |workspace, cx| {
            match result {
                Ok(databases) => {
                    workspace.available_databases = databases;
                    workspace.notice = Some(Notice::info("Database list refreshed"));
                }
                Err(error) => {
                    workspace.notice = Some(
                        Notice::error(
                            "Failed to refresh databases",
                            "Could not query pg_database.",
                        )
                        .with_detail(error.message),
                    );
                }
            }
            cx.notify();
        })
        .ok();
    })
    .detach();
}

fn table_page_failure_notice(error: DatabaseError) -> Notice {
    Notice::error("Table list failed", "Could not load the table explorer.")
        .with_detail(error.message)
}

#[cfg(test)]
#[path = "../../../tests/unit/ui/database_workspace/metadata.rs"]
mod tests;
