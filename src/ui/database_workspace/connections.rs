use super::{ConnectionEditor, DatabaseWorkspace};
use crate::domain::connection::ConnectionSummary;
use crate::infrastructure::postgres::model::PostgresConnectionProfile;
use crate::infrastructure::postgres::model::{PostgresServerInfo, PostgresSslMode};
use crate::infrastructure::{DatabaseError, PostgresProvider};
use crate::ui::Notice;
use gpui_kit::{AppContext as _, Context, Window};
use std::time::{SystemTime, UNIX_EPOCH};

use crate::domain::connection::ConnectionId;

pub(crate) fn open_editor(
    workspace: &mut DatabaseWorkspace,
    window: &mut Window,
    cx: &mut Context<DatabaseWorkspace>,
) {
    workspace.connection_editor = Some(ConnectionEditor::new(window, cx, None));
    workspace.pending_delete = None;
    workspace.notice = None;
    cx.notify();
}

pub(crate) fn edit_editor(
    workspace: &mut DatabaseWorkspace,
    connection_id: &ConnectionId,
    window: &mut Window,
    cx: &mut Context<DatabaseWorkspace>,
) {
    let Some(profile) = workspace.connection_profiles.get(connection_id).cloned() else {
        workspace.notice = Some(Notice::error(
            "Connection unavailable",
            "The saved connection details are missing.",
        ));
        cx.notify();
        return;
    };
    workspace.connection_editor = Some(ConnectionEditor::new(window, cx, Some(&profile)));
    workspace.pending_delete = None;
    workspace.notice = None;
    cx.notify();
}

pub(crate) fn close_editor(workspace: &mut DatabaseWorkspace, cx: &mut Context<DatabaseWorkspace>) {
    workspace.connection_test_generation = workspace.connection_test_generation.wrapping_add(1);
    workspace.connection_test_running = false;
    workspace.connection_editor = None;
    workspace.pending_delete = None;
    cx.notify();
}

pub(crate) fn set_ssl_mode(
    workspace: &mut DatabaseWorkspace,
    mode: PostgresSslMode,
    cx: &mut Context<DatabaseWorkspace>,
) {
    if let Some(editor) = &mut workspace.connection_editor {
        editor.ssl = mode;
        cx.notify();
    }
}

pub(crate) fn finish_connection_test(
    workspace: &mut DatabaseWorkspace,
    generation: u64,
    result: Result<PostgresServerInfo, DatabaseError>,
    cx: &mut Context<DatabaseWorkspace>,
) {
    if workspace.connection_test_generation != generation {
        return;
    }
    workspace.connection_test_running = false;
    workspace.notice = Some(match result {
        Ok(server) => Notice::success(format!(
            "PostgreSQL {} is ready for {}",
            server.version, server.user
        )),
        Err(error) => Notice::error(
            "Connection test failed",
            "Could not connect to this PostgreSQL server.",
        )
        .with_detail(error.message),
    });
    cx.notify();
}

pub(crate) fn test_connection(
    workspace: &mut DatabaseWorkspace,
    cx: &mut Context<DatabaseWorkspace>,
) {
    let Some(editor) = workspace.connection_editor.as_ref() else {
        return;
    };
    let id = editor
        .editing_id
        .clone()
        .unwrap_or_else(|| "connection-test".into());
    let load_saved_password = editor.editing_id.is_some();
    let profile = match editor.profile(cx, id) {
        Ok(profile) => profile,
        Err(message) => {
            workspace.notice = Some(Notice::error("Invalid connection details", message));
            cx.notify();
            return;
        }
    };

    workspace.connection_test_generation = workspace.connection_test_generation.wrapping_add(1);
    let generation = workspace.connection_test_generation;
    workspace.connection_test_running = true;
    workspace.notice = Some(Notice::info("Testing PostgreSQL connection..."));
    cx.notify();

    let credential_store = workspace.credential_store;
    let task = cx.background_spawn(async move {
        let mut profile = profile;
        if load_saved_password && profile.password.is_none() {
            profile.password = credential_store.load(&profile.id)?;
        }
        PostgresProvider::new().test_connection(profile)
    });
    cx.spawn(async move |this, cx| {
        let result = task.await;
        this.update(cx, |workspace, cx| {
            finish_connection_test(workspace, generation, result, cx);
        })
        .ok();
    })
    .detach();
}

pub(crate) fn save(workspace: &mut DatabaseWorkspace, cx: &mut Context<DatabaseWorkspace>) {
    let Some(editor) = workspace.connection_editor.as_ref() else {
        return;
    };

    let value = |state: &gpui_kit::Entity<gpui_kit::component::input::InputState>| {
        state.read(cx).value().to_string()
    };
    let name = value(&editor.name).trim().to_string();
    let host = value(&editor.host).trim().to_string();
    let port_text = value(&editor.port);
    let database = value(&editor.database).trim().to_string();
    let user = value(&editor.user).trim().to_string();
    let password = value(&editor.password);
    let ca_certificate_path = value(&editor.ca_certificate_path);
    let password_for_store = password.clone();
    let editing_id = editor.editing_id.clone();

    let port = match port_text.trim().parse::<u16>() {
        Ok(port) if port > 0 => port,
        _ => {
            workspace.notice = Some(Notice::error(
                "Invalid port",
                "Port must be a number between 1 and 65535.",
            ));
            cx.notify();
            return;
        }
    };

    if name.is_empty() || host.is_empty() || database.is_empty() || user.is_empty() {
        workspace.notice = Some(Notice::error(
            "Missing connection details",
            "Name, host, database, and user are required.",
        ));
        cx.notify();
        return;
    }

    let ssl = editor.ssl;
    let id = editing_id.unwrap_or_else(|| {
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|duration| duration.as_nanos())
            .unwrap_or_default();
        let mut candidate = format!("postgres-{timestamp}");
        let mut suffix = 1;
        while workspace.connection_profiles.contains_key(&candidate) {
            candidate = format!("postgres-{timestamp}-{suffix}");
            suffix += 1;
        }
        candidate
    });
    let profile = PostgresConnectionProfile {
        id: id.clone(),
        name: name.clone(),
        host: host.clone(),
        port,
        database: database.clone(),
        user: user.clone(),
        password: (!password.is_empty()).then_some(password),
        ssl,
        reject_unauthorized: editor.reject_unauthorized,
        ca_certificate_path: (!ca_certificate_path.trim().is_empty())
            .then_some(ca_certificate_path.trim().to_owned()),
    };
    workspace.postgres_provider.invalidate_metadata_session();
    workspace.connection_profiles.insert(id.clone(), profile);
    let summary = ConnectionSummary {
        id: id.clone(),
        name,
        database,
        host,
        port,
        user,
        ssl: ssl.into(),
        reject_unauthorized: editor.reject_unauthorized,
        ca_certificate_path: (!ca_certificate_path.trim().is_empty())
            .then_some(ca_certificate_path.trim().to_owned()),
    };
    if let Some(existing) = workspace
        .connections
        .iter_mut()
        .find(|connection| connection.id == id)
    {
        *existing = summary;
    } else {
        workspace.connections.push(summary);
    }
    let persistence_error = workspace
        .connection_store
        .save(&workspace.connections)
        .err();
    workspace.workspace.selected_connection = Some(id.clone());
    workspace.pending_delete = None;
    workspace.connection_generation = workspace.connection_generation.wrapping_add(1);
    workspace.connection_status = crate::domain::connection::ConnectionStatus::Disconnected;
    workspace.server_version = None;
    workspace.connection_editor = None;
    if let Some(error) = persistence_error {
        workspace.notice = Some(
            Notice::warning(
                "Connection saved only for this session",
                "The connection file could not be updated.",
            )
            .with_detail(error.message),
        );
        cx.notify();
        return;
    }
    if password_for_store.is_empty() {
        workspace.notice = Some(Notice::success(
            "Connection saved. Test it before browsing.",
        ));
        cx.notify();
        return;
    }
    workspace.notice = Some(Notice::info("Connection saved. Securing password..."));
    let credential_store = workspace.credential_store;
    let credential_id = id;
    let credential_id_for_task = credential_id.clone();
    let task = cx.background_spawn(async move {
        credential_store.save(&credential_id_for_task, &password_for_store)
    });
    cx.spawn(async move |this, cx| {
        let result = task.await;
        this.update(cx, |workspace, cx| {
            finish_credential_save(workspace, &credential_id, result, cx);
        })
        .ok();
    })
    .detach();
    cx.notify();
}

fn finish_credential_save(
    workspace: &mut DatabaseWorkspace,
    connection_id: &ConnectionId,
    result: Result<(), DatabaseError>,
    cx: &mut Context<DatabaseWorkspace>,
) {
    if workspace.workspace.selected_connection.as_ref() != Some(connection_id) {
        return;
    }
    if workspace
        .notice
        .as_ref()
        .is_none_or(|notice| notice.message != "Connection saved. Securing password...")
    {
        return;
    }
    workspace.notice = Some(match result {
        Ok(()) => Notice::success("Connection saved. Password secured in the OS credential store."),
        Err(error) => Notice::warning(
            "Connection saved, but password needs attention",
            "The connection is available, but its password could not be secured.",
        )
        .with_detail(error.message),
    });
    cx.notify();
}

pub(crate) fn request_delete(
    workspace: &mut DatabaseWorkspace,
    connection_id: &ConnectionId,
    cx: &mut Context<DatabaseWorkspace>,
) {
    if workspace.pending_delete.as_ref() != Some(connection_id) {
        workspace.pending_delete = Some(connection_id.clone());
        workspace.notice = Some(Notice::warning(
            "Confirm connection removal",
            "Click Confirm delete again to remove this saved connection.",
        ));
        cx.notify();
        return;
    }

    workspace
        .connections
        .retain(|connection| &connection.id != connection_id);
    workspace.connection_profiles.remove(connection_id);
    workspace.pending_delete = None;
    workspace.object_explorer = None;
    if workspace.workspace.selected_connection.as_ref() == Some(connection_id) {
        workspace.postgres_provider.invalidate_metadata_session();
    }
    let persistence_error = workspace
        .connection_store
        .save(&workspace.connections)
        .err();

    if workspace.workspace.selected_connection.as_ref() == Some(connection_id) {
        workspace.workspace.selected_connection = workspace
            .connections
            .first()
            .map(|connection| connection.id.clone());
        workspace.connection_status = crate::domain::connection::ConnectionStatus::Disconnected;
        workspace.server_version = None;
    }
    if let Some(error) = persistence_error {
        workspace.notice = Some(
            Notice::warning(
                "Connection removed for this session",
                "The connection file could not be updated.",
            )
            .with_detail(error.message),
        );
        cx.notify();
        return;
    }
    workspace.notice = Some(Notice::info(
        "Connection deleted. Removing saved credential...",
    ));
    let credential_store = workspace.credential_store;
    let credential_id = connection_id.clone();
    let credential_id_for_task = credential_id.clone();
    let task = cx.background_spawn(async move { credential_store.delete(&credential_id_for_task) });
    cx.spawn(async move |this, cx| {
        let result = task.await;
        this.update(cx, |workspace, cx| {
            finish_credential_delete(workspace, result, cx);
        })
        .ok();
    })
    .detach();
    cx.notify();
}

fn finish_credential_delete(
    workspace: &mut DatabaseWorkspace,
    result: Result<(), DatabaseError>,
    cx: &mut Context<DatabaseWorkspace>,
) {
    if workspace
        .notice
        .as_ref()
        .is_none_or(|notice| notice.message != "Connection deleted. Removing saved credential...")
    {
        return;
    }
    workspace.notice = Some(match result {
        Ok(()) => Notice::success("Connection deleted"),
        Err(error) => Notice::warning(
            "Connection deleted, but credential cleanup needs attention",
            "The saved password may still exist in the OS credential store.",
        )
        .with_detail(error.message),
    });
    cx.notify();
}
