use super::DatabaseWorkspace;
use crate::domain::connection::ConnectionId;
use crate::domain::query::{
    CellUpdateRequest, MutationResult, QueryResult, TableDataCursor, TablePreviewPageRequest,
};
use crate::infrastructure::DatabaseError;
use crate::infrastructure::PostgresProvider;
use crate::infrastructure::postgres::sql::quote_identifier;
use crate::ui::Notice;
use gpui_kit::{AppContext as _, Context, Focusable as _, Window, component::input::InputState};
use std::sync::Arc;

pub(crate) fn execute_read_query(
    workspace: &mut DatabaseWorkspace,
    cx: &mut Context<DatabaseWorkspace>,
) {
    let Some(connection_id) = workspace.workspace.selected_connection.clone() else {
        workspace.notice = Some(Notice::warning(
            "No database connected",
            "Connect to a database before running a query.",
        ));
        cx.notify();
        return;
    };
    let Some(profile) = workspace.connection_profiles.get(&connection_id).cloned() else {
        workspace.notice = Some(Notice::error(
            "Connection unavailable",
            "The saved connection details are missing.",
        ));
        cx.notify();
        return;
    };
    if workspace.query_running {
        return;
    }

    let sql = workspace.query_input.read(cx).value().to_string();
    workspace.write_confirmation_sql = None;
    workspace.query_generation = workspace.query_generation.wrapping_add(1);
    let query_generation = workspace.query_generation;
    workspace.query_running = true;
    workspace.notice = Some(Notice::info("Running read-only query..."));
    cx.notify();

    let credential_store = workspace.credential_store;
    let provider = workspace.postgres_provider.clone();
    let task = cx.background_spawn(async move {
        let mut profile = profile;
        if profile.password.is_none() {
            profile.password = credential_store.load(&profile.id)?;
        }
        provider.execute_read_query(profile, sql)
    });
    cx.spawn(async move |this, cx| {
        let result = task.await;
        this.update(cx, |workspace, cx| {
            finish_query(workspace, &connection_id, query_generation, result, cx);
        })
        .ok();
    })
    .detach();
}

pub(crate) fn execute_write_query(
    workspace: &mut DatabaseWorkspace,
    cx: &mut Context<DatabaseWorkspace>,
) {
    let Some(connection_id) = workspace.workspace.selected_connection.clone() else {
        workspace.notice = Some(Notice::warning(
            "No database connected",
            "Connect to a database before running a write.",
        ));
        cx.notify();
        return;
    };
    let Some(profile) = workspace.connection_profiles.get(&connection_id).cloned() else {
        workspace.notice = Some(Notice::error(
            "Connection unavailable",
            "The saved connection details are missing.",
        ));
        cx.notify();
        return;
    };
    if workspace.query_running {
        return;
    }

    let sql = workspace.query_input.read(cx).value().to_string();
    if !PostgresProvider::is_mutating_query(&sql)
        || workspace.write_confirmation_sql.as_deref() != Some(sql.trim())
    {
        workspace.write_confirmation_sql = Some(sql.trim().to_owned());
        workspace.notice = Some(Notice::warning(
            "Review before writing",
            "Click Run write again to confirm this data change.",
        ));
        cx.notify();
        return;
    }
    workspace.write_confirmation_sql = None;
    workspace.query_generation = workspace.query_generation.wrapping_add(1);
    let query_generation = workspace.query_generation;
    workspace.query_running = true;
    workspace.notice = Some(Notice::info("Running write in a transaction..."));
    cx.notify();

    let credential_store = workspace.credential_store;
    let provider = workspace.postgres_provider.clone();
    let task = cx.background_spawn(async move {
        let mut profile = profile;
        if profile.password.is_none() {
            profile.password = credential_store.load(&profile.id)?;
        }
        provider.execute_mutation(profile, sql)
    });
    cx.spawn(async move |this, cx| {
        let result = task.await;
        this.update(cx, |workspace, cx| {
            finish_mutation(workspace, &connection_id, query_generation, result, cx);
        })
        .ok();
    })
    .detach();
}

pub(crate) fn begin_table_cell_edit(
    workspace: &mut DatabaseWorkspace,
    row_index: usize,
    column_index: usize,
    window: &mut Window,
    cx: &mut Context<DatabaseWorkspace>,
) {
    if workspace.query_running {
        return;
    }
    let Some(result) = workspace.query_result.as_ref() else {
        return;
    };
    let Some(table) = result.editable.as_ref() else {
        return;
    };
    let Some(column) = result.columns.get(column_index) else {
        return;
    };
    if table.primary_key_columns.is_empty()
        || table.primary_key_columns.iter().any(|key| key == column)
        || !is_inline_edit_type(result.column_types.get(column_index).map(String::as_str))
    {
        return;
    }
    let Some(value) = result
        .rows
        .get(row_index)
        .and_then(|row| row.get(column_index))
    else {
        return;
    };
    let is_null = result
        .null_cells
        .get(row_index)
        .and_then(|row| row.get(column_index))
        .copied()
        .unwrap_or(false);
    let editor =
        cx.new(|cx| InputState::new(window, cx).default_value(if is_null { "" } else { value }));
    editor.read(cx).focus_handle(cx).focus(window, cx);
    workspace.active_cell_edit = Some(super::ActiveCellEdit {
        row_index,
        column_index,
        input: editor,
        set_null: is_null,
    });
    cx.notify();
}

pub(crate) fn is_inline_edit_type(type_name: Option<&str>) -> bool {
    type_name.is_some_and(|type_name| {
        matches!(
            type_name.to_ascii_lowercase().as_str(),
            "text"
                | "varchar"
                | "bpchar"
                | "name"
                | "bool"
                | "int2"
                | "int4"
                | "int8"
                | "float4"
                | "float8"
                | "numeric"
                | "date"
                | "time"
                | "timetz"
                | "timestamp"
                | "timestamptz"
                | "uuid"
                | "bytea"
        )
    })
}

pub(crate) fn save_table_cell_edit(
    workspace: &mut DatabaseWorkspace,
    cx: &mut Context<DatabaseWorkspace>,
) {
    let Some(edit) = workspace.active_cell_edit.as_ref() else {
        return;
    };
    if workspace.query_running {
        return;
    }
    let Some(connection_id) = workspace.workspace.selected_connection.clone() else {
        workspace.notice = Some(Notice::warning(
            "No database connected",
            "Connect to a database before saving this cell.",
        ));
        cx.notify();
        return;
    };
    let Some(profile) = workspace.connection_profiles.get(&connection_id).cloned() else {
        workspace.notice = Some(Notice::error(
            "Connection unavailable",
            "The saved connection details are missing.",
        ));
        cx.notify();
        return;
    };
    let Some(result) = workspace.query_result.as_ref() else {
        return;
    };
    let Some(table) = result.editable.as_ref() else {
        return;
    };
    let Some(column) = result.columns.get(edit.column_index).cloned() else {
        return;
    };
    let Some(row) = result.rows.get(edit.row_index) else {
        return;
    };
    let mut primary_key_values = Vec::with_capacity(table.primary_key_columns.len());
    for key_column in &table.primary_key_columns {
        let Some(key_index) = result.columns.iter().position(|name| name == key_column) else {
            workspace.notice = Some(Notice::error(
                "Cannot identify this row",
                "A primary-key column is missing from the preview. Refresh the table.",
            ));
            cx.notify();
            return;
        };
        if result
            .null_cells
            .get(edit.row_index)
            .and_then(|nulls| nulls.get(key_index))
            .copied()
            .unwrap_or(false)
        {
            workspace.notice = Some(Notice::error(
                "Cannot identify this row",
                "A primary-key value is NULL. Refresh the table before editing.",
            ));
            cx.notify();
            return;
        }
        let Some(value) = row.get(key_index) else {
            return;
        };
        primary_key_values.push(value.clone());
    }
    let old_is_null = result
        .null_cells
        .get(edit.row_index)
        .and_then(|nulls| nulls.get(edit.column_index))
        .copied()
        .unwrap_or(false);
    let old_value = row.get(edit.column_index).cloned().unwrap_or_default();
    let expected_value = (!old_is_null).then(|| old_value.clone());
    let new_value = if edit.set_null {
        None
    } else {
        Some(edit.input.read(cx).value().to_string())
    };
    if old_is_null == new_value.is_none()
        && new_value.as_deref().is_none_or(|value| value == old_value)
    {
        workspace.active_cell_edit = None;
        cx.notify();
        return;
    }
    let request = CellUpdateRequest {
        schema: table.schema.clone(),
        table: table.table.clone(),
        column,
        primary_key_columns: table.primary_key_columns.clone(),
        primary_key_values,
        value: new_value,
        expected_value,
    };
    let credential_store = workspace.credential_store;
    let provider = workspace.postgres_provider.clone();
    workspace.query_generation = workspace.query_generation.wrapping_add(1);
    let query_generation = workspace.query_generation;
    workspace.query_running = true;
    workspace.notice = Some(Notice::info("Saving cell in a transaction..."));
    cx.notify();

    let task = cx.background_spawn(async move {
        let mut profile = profile;
        if profile.password.is_none() {
            profile.password = credential_store.load(&profile.id)?;
        }
        provider.update_table_cell(profile, request)
    });
    cx.spawn(async move |this, cx| {
        let result = task.await;
        this.update(cx, |workspace, cx| {
            finish_cell_update(workspace, &connection_id, query_generation, result, cx);
        })
        .ok();
    })
    .detach();
}

fn finish_cell_update(
    workspace: &mut DatabaseWorkspace,
    connection_id: &ConnectionId,
    query_generation: u64,
    result: Result<MutationResult, DatabaseError>,
    cx: &mut Context<DatabaseWorkspace>,
) {
    if workspace.workspace.selected_connection.as_ref() != Some(connection_id)
        || workspace.query_generation != query_generation
    {
        return;
    }
    workspace.query_running = false;
    match result {
        Ok(_) => {
            workspace.active_cell_edit = None;
            workspace.cell_update_reload_pending = true;
            if let Some((schema, table)) = workspace.selected_table.clone() {
                preview_table_page_without_window(workspace, &schema, &table, cx);
            }
        }
        Err(error) => {
            workspace.notice = Some(
                Notice::error(
                    "Cell update failed",
                    "The change was rolled back; the value is unchanged.",
                )
                .with_detail(error.message),
            );
            cx.notify();
        }
    }
}

pub(crate) fn preview_table_page(
    workspace: &mut DatabaseWorkspace,
    schema: &str,
    table: &str,
    window: &mut Window,
    cx: &mut Context<DatabaseWorkspace>,
) {
    preview_table_page_with_cursor(workspace, schema, table, window, cx, None);
}

pub(crate) fn preview_table_page_without_window(
    workspace: &mut DatabaseWorkspace,
    schema: &str,
    table: &str,
    cx: &mut Context<DatabaseWorkspace>,
) {
    preview_table_page_inner(workspace, schema, table, None, None, cx);
}

pub(crate) fn preview_table_page_with_cursor(
    workspace: &mut DatabaseWorkspace,
    schema: &str,
    table: &str,
    window: &mut Window,
    cx: &mut Context<DatabaseWorkspace>,
    page_cursor: Option<TableDataCursor>,
) {
    preview_table_page_inner(workspace, schema, table, Some(window), page_cursor, cx);
}

fn preview_table_page_inner(
    workspace: &mut DatabaseWorkspace,
    schema: &str,
    table: &str,
    window: Option<&mut Window>,
    page_cursor: Option<TableDataCursor>,
    cx: &mut Context<DatabaseWorkspace>,
) {
    workspace.active_cell_edit = None;
    if workspace
        .selected_table
        .as_ref()
        .is_none_or(|(old_schema, old_table)| old_schema != schema || old_table != table)
    {
        workspace.table_column_filters.clear();
        workspace.active_cell_edit = None;
        workspace.table_data_offset = 0;
    }
    workspace.selected_table = Some((schema.to_owned(), table.to_owned()));
    let primary_key_columns = workspace
        .query_result
        .as_ref()
        .and_then(|result| result.editable.as_ref())
        .filter(|editable| editable.schema == schema && editable.table == table)
        .map_or_else(Vec::new, |editable| editable.primary_key_columns.clone());
    let order_sql =
        workspace
            .table_data_sort
            .as_ref()
            .map_or_else(String::new, |(column, descending)| {
                let direction = if *descending { "DESC" } else { "ASC" };
                let order_columns = if primary_key_columns.first() == Some(column) {
                    primary_key_columns.as_slice()
                } else {
                    std::slice::from_ref(column)
                };
                format!(
                    " ORDER BY {}",
                    order_columns
                        .iter()
                        .map(|column| format!("{} {direction}", quote_identifier(column)))
                        .collect::<Vec<_>>()
                        .join(", ")
                )
            });
    let sql = format!(
        "SELECT * FROM {}.{}{} LIMIT {} OFFSET {};",
        quote_identifier(schema),
        quote_identifier(table),
        order_sql,
        workspace.table_data_limit,
        workspace.table_data_offset,
    );
    if let Some(window) = window {
        workspace.query_input.update(cx, |state, cx| {
            state.set_value(sql, window, cx);
        });
    }
    let Some(connection_id) = workspace.workspace.selected_connection.clone() else {
        workspace.notice = Some(Notice::warning(
            "No database connected",
            "Connect to a database before previewing a table.",
        ));
        cx.notify();
        return;
    };
    let Some(profile) = workspace.connection_profiles.get(&connection_id).cloned() else {
        workspace.notice = Some(Notice::error(
            "Connection unavailable",
            "The saved connection details are missing.",
        ));
        cx.notify();
        return;
    };
    if workspace.query_running {
        return;
    }
    workspace.write_confirmation_sql = None;
    workspace.query_generation = workspace.query_generation.wrapping_add(1);
    let query_generation = workspace.query_generation;
    workspace.query_running = true;
    workspace.notice = Some(Notice::info(format!("Loading {schema}.{table}...")));
    cx.notify();

    let credential_store = workspace.credential_store;
    let limit = workspace.table_data_limit;
    let offset = workspace.table_data_offset;
    let sort = workspace.table_data_sort.clone();
    let filters = workspace.table_column_filters.clone();
    let provider = workspace.postgres_provider.clone();
    let schema = schema.to_owned();
    let table = table.to_owned();
    let task = cx.background_spawn(async move {
        let mut profile = profile;
        if profile.password.is_none() {
            profile.password = credential_store.load(&profile.id)?;
        }
        provider.preview_table_page(
            profile,
            schema,
            table,
            TablePreviewPageRequest {
                limit,
                offset,
                sort,
                primary_key_columns,
                cursor: page_cursor,
                filters,
            },
        )
    });
    cx.spawn(async move |this, cx| {
        let result = task.await;
        this.update(cx, |workspace, cx| {
            finish_table_preview(workspace, &connection_id, query_generation, result, cx);
        })
        .ok();
    })
    .detach();
}

pub(crate) fn finish_mutation(
    workspace: &mut DatabaseWorkspace,
    connection_id: &ConnectionId,
    query_generation: u64,
    result: Result<MutationResult, DatabaseError>,
    cx: &mut Context<DatabaseWorkspace>,
) {
    if workspace.workspace.selected_connection.as_ref() != Some(connection_id)
        || workspace.query_generation != query_generation
    {
        return;
    }
    workspace.query_running = false;
    match result {
        Ok(result) => {
            workspace.query_result = None;
            workspace.notice = Some(Notice::success(format!(
                "Write committed: {} row(s) affected",
                result.affected_rows
            )));
        }
        Err(error) => {
            workspace.notice = Some(
                Notice::error("Write failed", "The data change was rolled back.")
                    .with_detail(error.message),
            );
        }
    }
    workspace.refresh_table_data_filter_cache(cx);
    cx.notify();
}

pub(crate) fn finish_table_preview(
    workspace: &mut DatabaseWorkspace,
    connection_id: &ConnectionId,
    query_generation: u64,
    result: Result<QueryResult, DatabaseError>,
    cx: &mut Context<DatabaseWorkspace>,
) {
    if workspace.workspace.selected_connection.as_ref() != Some(connection_id)
        || workspace.query_generation != query_generation
    {
        return;
    }
    workspace.query_running = false;
    match result {
        Ok(result) => {
            let row_count = result.rows.len();
            workspace.table_data_offset = result.offset;
            workspace.table_data_limit = result.limit;
            workspace.table_data_has_next = result.has_next;
            workspace.query_result = Some(Arc::new(result));
            workspace.notice = Some(if workspace.cell_update_reload_pending {
                workspace.cell_update_reload_pending = false;
                Notice::success("Cell updated and table refreshed")
            } else {
                Notice::success(format!("Table preview loaded: {row_count} row(s)"))
            });
        }
        Err(error) => {
            workspace.query_result = None;
            workspace.notice = Some(if workspace.cell_update_reload_pending {
                workspace.cell_update_reload_pending = false;
                Notice::warning(
                    "Cell saved, but refresh failed",
                    "The database change committed; refresh the table to verify the current row.",
                )
                .with_detail(error.message)
            } else {
                Notice::error("Table preview failed", "Could not load this table.")
                    .with_detail(error.message)
            });
        }
    }
    workspace.refresh_table_data_filter_cache(cx);
    cx.notify();
}

pub(crate) fn finish_query(
    workspace: &mut DatabaseWorkspace,
    connection_id: &ConnectionId,
    query_generation: u64,
    result: Result<QueryResult, DatabaseError>,
    cx: &mut Context<DatabaseWorkspace>,
) {
    if workspace.workspace.selected_connection.as_ref() != Some(connection_id)
        || workspace.query_generation != query_generation
    {
        return;
    }
    workspace.query_running = false;
    match result {
        Ok(result) => {
            let row_count = result.rows.len();
            workspace.query_result = Some(Arc::new(result));
            workspace.notice = Some(Notice::success(format!(
                "Query completed: {row_count} row(s) returned"
            )));
        }
        Err(error) => {
            workspace.query_result = None;
            workspace.notice = Some(
                Notice::error(
                    "Query could not be completed",
                    "The query failed or its results exceeded display limits.",
                )
                .with_detail(error.message),
            );
        }
    }
    workspace.refresh_table_data_filter_cache(cx);
    cx.notify();
}
