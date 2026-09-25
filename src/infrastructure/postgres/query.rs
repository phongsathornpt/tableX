use super::PostgresProvider;
#[cfg(test)]
use super::connect::read_only_connection_config;
use super::connect::{connect_with_timeout, mutation_connection_config, rustls_connector};
use super::error::{format_connection_error, format_tls_connection_error};
use super::metadata::{MetadataSession, MetadataSessionCache};
use super::model::{PostgresConnectionProfile, PostgresSslMode};
use super::runtime;
use super::sql::quote_identifier;
#[cfg(test)]
use super::value::cell_to_string;
use super::value::{cell_to_string_with_null, enum_values as postgres_enum_values};
use crate::domain::query::{
    CellUpdateRequest, EditableTable, MutationResult, QueryResult, TableColumnFilter,
    TableDataCursor, TableDataCursorDirection, TableFilterOperator, TablePreviewPageRequest,
};
use crate::infrastructure::error::DatabaseError;
use futures_util::TryStreamExt as _;
use std::future::Future;
#[cfg(test)]
use std::time::{Duration, Instant};
use tokio_postgres::{Client, NoTls};

const MAX_QUERY_BYTES: usize = 100_000;
pub(crate) const MAX_QUERY_ROWS: usize = 500;
const MAX_QUERY_CELL_BYTES: usize = 1024 * 1024;
const MAX_QUERY_RESULT_BYTES: usize = 32 * 1024 * 1024;
const MAX_TABLE_FILTERS: usize = 20;
const MAX_TABLE_FILTER_VALUE_BYTES: usize = 4096;
const MAX_CELL_UPDATE_VALUE_BYTES: usize = 1024 * 1024;
const TABLE_PRIMARY_KEY_QUERY: &str = "SELECT a.attname
         FROM pg_index i
         JOIN pg_attribute a ON a.attrelid = i.indrelid AND a.attnum = ANY(i.indkey)
         WHERE i.indrelid = to_regclass($1) AND i.indisprimary
         ORDER BY array_position(i.indkey, a.attnum)";

#[derive(Default)]
struct DecodedRows {
    rows: Vec<Vec<String>>,
    null_cells: Vec<Vec<bool>>,
    truncated_cells: Vec<Vec<bool>>,
}

struct DecodedRow {
    values: Vec<String>,
    null_cells: Vec<bool>,
    truncated_cells: Vec<bool>,
}

impl DecodedRows {
    fn len(&self) -> usize {
        self.rows.len()
    }

    fn truncate(&mut self, len: usize) {
        self.rows.truncate(len);
        self.null_cells.truncate(len);
        self.truncated_cells.truncate(len);
    }

    fn reverse(&mut self) {
        self.rows.reverse();
        self.null_cells.reverse();
        self.truncated_cells.reverse();
    }
}

pub(crate) fn is_mutating_query(sql: &str) -> bool {
    let statement = sql.trim().trim_end_matches(';').trim();
    matches!(
        statement
            .split_whitespace()
            .next()
            .unwrap_or_default()
            .to_ascii_lowercase()
            .as_str(),
        "insert" | "update" | "delete" | "merge"
    )
}

pub(crate) fn validate_read(sql: &str) -> Result<(), DatabaseError> {
    let trimmed = sql.trim();
    if trimmed.is_empty() {
        return Err(DatabaseError::new("Enter a SQL query first"));
    }
    if trimmed.len() > MAX_QUERY_BYTES {
        return Err(DatabaseError::new("Query is limited to 100,000 bytes"));
    }
    let statement = trimmed.trim_end_matches(';').trim_end();
    if statement.contains(';') {
        return Err(DatabaseError::new(
            "Only one SQL statement can be run at a time",
        ));
    }
    let keyword = statement.split_whitespace().next().unwrap_or_default();
    if !matches!(
        keyword.to_ascii_lowercase().as_str(),
        "select" | "with" | "show" | "values" | "explain"
    ) {
        return Err(DatabaseError::new(
            "Only read queries are supported here: SELECT, WITH, SHOW, VALUES, or EXPLAIN",
        ));
    }
    Ok(())
}

pub(crate) fn validate_mutation(sql: &str) -> Result<(), DatabaseError> {
    let trimmed = sql.trim();
    if trimmed.is_empty() {
        return Err(DatabaseError::new("Enter a SQL statement first"));
    }
    if trimmed.len() > MAX_QUERY_BYTES {
        return Err(DatabaseError::new(
            "SQL statement is limited to 100,000 bytes",
        ));
    }
    let statement = trimmed.trim_end_matches(';').trim_end();
    if statement.contains(';') {
        return Err(DatabaseError::new(
            "Only one SQL statement can be run at a time",
        ));
    }
    if !is_mutating_query(statement) {
        return Err(DatabaseError::new(
            "Write execution supports INSERT, UPDATE, DELETE, and MERGE statements",
        ));
    }
    Ok(())
}

pub(crate) fn execute_read(
    provider: &PostgresProvider,
    profile: PostgresConnectionProfile,
    sql: String,
) -> Result<QueryResult, DatabaseError> {
    validate_read(&sql)?;
    runtime::run(move || execute_query_on_runtime(&provider.metadata_sessions, &profile, &sql))
}

pub(crate) fn execute_mutation(
    _provider: &PostgresProvider,
    profile: PostgresConnectionProfile,
    sql: String,
) -> Result<MutationResult, DatabaseError> {
    validate_mutation(&sql)?;
    runtime::run(move || execute_mutation_on_runtime(&profile, &sql))
}

pub(crate) fn update_table_cell(
    _provider: &PostgresProvider,
    profile: PostgresConnectionProfile,
    request: CellUpdateRequest,
) -> Result<MutationResult, DatabaseError> {
    if request.primary_key_columns.is_empty()
        || request.primary_key_columns.len() != request.primary_key_values.len()
    {
        return Err(DatabaseError::new(
            "Inline editing requires a complete primary-key value for this row",
        ));
    }
    if request.primary_key_columns.len() > 64 {
        return Err(DatabaseError::new(
            "Inline editing supports at most 64 primary-key columns",
        ));
    }
    if request
        .value
        .as_ref()
        .is_some_and(|value| value.len() > MAX_CELL_UPDATE_VALUE_BYTES)
    {
        return Err(DatabaseError::new("Edited values are limited to 1 MiB"));
    }
    if request
        .primary_key_columns
        .iter()
        .any(|column| column == &request.column)
    {
        return Err(DatabaseError::new(
            "Primary-key columns cannot be changed inline",
        ));
    }
    runtime::run(move || update_table_cell_on_runtime(&profile, request))
}

fn update_table_cell_on_runtime(
    profile: &PostgresConnectionProfile,
    request: CellUpdateRequest,
) -> Result<MutationResult, DatabaseError> {
    let runtime = runtime::handle()?;
    runtime.block_on(async {
        let config = mutation_connection_config(profile);
        match profile.ssl {
            PostgresSslMode::Disable => {
                let (client, connection) =
                    connect_with_timeout(config.connect(NoTls), profile, |error| {
                        DatabaseError::new(format_connection_error(profile, error))
                    })
                    .await?;
                update_table_cell_client(client, connection, request).await
            }
            PostgresSslMode::Require => {
                let tls = rustls_connector(profile)?;
                let (client, connection) =
                    connect_with_timeout(config.connect(tls), profile, |error| {
                        format_tls_connection_error(profile, error, PostgresSslMode::Require)
                    })
                    .await?;
                update_table_cell_client(client, connection, request).await
            }
            PostgresSslMode::Prefer => {
                let tls = rustls_connector(profile)?;
                let (client, connection) =
                    connect_with_timeout(config.connect(tls), profile, |error| {
                        format_tls_connection_error(profile, error, PostgresSslMode::Prefer)
                    })
                    .await?;
                update_table_cell_client(client, connection, request).await
            }
        }
    })
}

async fn update_table_cell_client<C>(
    mut client: Client,
    connection: C,
    request: CellUpdateRequest,
) -> Result<MutationResult, DatabaseError>
where
    C: Future<Output = Result<(), tokio_postgres::Error>> + Send + 'static,
{
    tokio::spawn(async move {
        if let Err(error) = connection.await {
            eprintln!("PostgreSQL cell-update connection closed: {error}");
        }
    });

    let relation = format!(
        "{}.{}",
        quote_identifier(&request.schema),
        quote_identifier(&request.table)
    );
    let describe_sql = format!("SELECT * FROM {relation} LIMIT 0");
    let describe = client.prepare(&describe_sql).await.map_err(|error| {
        DatabaseError::new(format!("could not inspect the target table: {error}"))
    })?;
    let column_type = |name: &str| {
        describe
            .columns()
            .iter()
            .find(|column| column.name() == name)
            .map(|column| qualified_type_name(column.type_()))
    };
    let value_type = column_type(&request.column).ok_or_else(|| {
        DatabaseError::new("The edited column is no longer present; refresh the table")
    })?;
    let key_types = request
        .primary_key_columns
        .iter()
        .map(|key_column| {
            column_type(key_column)
                .map(|type_name| (key_column.clone(), type_name))
                .ok_or_else(|| {
                    DatabaseError::new("The row key is no longer present; refresh the table")
                })
        })
        .collect::<Result<Vec<_>, _>>()?;
    let sql = build_cell_update_sql(&relation, &request, &value_type, &key_types);
    let transaction = client
        .transaction()
        .await
        .map_err(|error| DatabaseError::new(format!("could not begin cell update: {error}")))?;
    let statement = transaction
        .prepare(&sql)
        .await
        .map_err(|error| DatabaseError::new(format!("could not prepare cell update: {error}")))?;
    let mut parameters: Vec<&(dyn tokio_postgres::types::ToSql + Sync)> =
        Vec::with_capacity(request.primary_key_values.len() + 2);
    parameters.push(&request.value);
    parameters.extend(
        request
            .primary_key_values
            .iter()
            .map(|value| value as &(dyn tokio_postgres::types::ToSql + Sync)),
    );
    parameters.push(&request.expected_value);
    let affected_rows = transaction
        .execute(&statement, &parameters)
        .await
        .map_err(|error| {
            DatabaseError::new(format!("cell update failed and was rolled back: {error}"))
        })?;
    if affected_rows != 1 {
        return Err(DatabaseError::new(if affected_rows == 0 {
            "The row changed or no longer exists. Refresh the table and try again."
        } else {
            "The row key matched more than one row. The update was rolled back."
        }));
    }
    transaction
        .commit()
        .await
        .map_err(|error| DatabaseError::new(format!("could not commit cell update: {error}")))?;
    Ok(MutationResult { affected_rows })
}

fn build_cell_update_sql(
    relation: &str,
    request: &CellUpdateRequest,
    value_type: &str,
    key_types: &[(String, String)],
) -> String {
    let predicates = key_types
        .iter()
        .enumerate()
        .map(|(index, (column, type_name))| {
            format!(
                "{} = ((${}::text)::{type_name})",
                quote_identifier(column),
                index + 2
            )
        })
        .collect::<Vec<_>>();
    let expected_parameter = key_types.len() + 2;
    format!(
        "UPDATE {relation} SET {} = (($1::text)::{value_type}) WHERE {} AND {} IS NOT DISTINCT FROM ((${}::text)::{value_type})",
        quote_identifier(&request.column),
        predicates.join(" AND "),
        quote_identifier(&request.column),
        expected_parameter,
    )
}

pub(crate) fn execute_mutation_on_runtime(
    profile: &PostgresConnectionProfile,
    sql: &str,
) -> Result<MutationResult, DatabaseError> {
    let runtime = runtime::handle()?;
    runtime.block_on(async {
        let config = mutation_connection_config(profile);
        match profile.ssl {
            PostgresSslMode::Disable => {
                let (client, connection) =
                    connect_with_timeout(config.connect(NoTls), profile, |error| {
                        DatabaseError::new(format_connection_error(profile, error))
                    })
                    .await?;
                execute_mutation_client(client, connection, sql).await
            }
            PostgresSslMode::Require => {
                let tls = rustls_connector(profile)?;
                let (client, connection) =
                    connect_with_timeout(config.connect(tls), profile, |error| {
                        format_tls_connection_error(profile, error, PostgresSslMode::Require)
                    })
                    .await?;
                execute_mutation_client(client, connection, sql).await
            }
            PostgresSslMode::Prefer => {
                let tls = rustls_connector(profile)?;
                let (client, connection) =
                    connect_with_timeout(config.connect(tls), profile, |error| {
                        format_tls_connection_error(profile, error, PostgresSslMode::Prefer)
                    })
                    .await?;
                execute_mutation_client(client, connection, sql).await
            }
        }
    })
}

async fn execute_mutation_client<C>(
    mut client: Client,
    connection: C,
    sql: &str,
) -> Result<MutationResult, DatabaseError>
where
    C: Future<Output = Result<(), tokio_postgres::Error>> + Send + 'static,
{
    tokio::spawn(async move {
        if let Err(error) = connection.await {
            eprintln!("PostgreSQL mutation connection closed: {error}");
        }
    });

    let transaction = client
        .transaction()
        .await
        .map_err(|error| DatabaseError::new(format!("could not begin transaction: {error}")))?;
    let affected_rows = transaction.execute(sql, &[]).await.map_err(|error| {
        DatabaseError::new(format!("mutation failed and was rolled back: {error}"))
    })?;
    transaction
        .commit()
        .await
        .map_err(|error| DatabaseError::new(format!("could not commit mutation: {error}")))?;
    Ok(MutationResult { affected_rows })
}

pub(crate) fn preview_table_page(
    provider: &PostgresProvider,
    profile: PostgresConnectionProfile,
    schema: String,
    table: String,
    request: TablePreviewPageRequest,
) -> Result<QueryResult, DatabaseError> {
    let TablePreviewPageRequest {
        limit,
        offset,
        sort,
        primary_key_columns,
        cursor,
        filters,
    } = request;
    let limit = limit.clamp(1, 100);
    let offset_for_query = i64::try_from(offset)
        .map_err(|_| DatabaseError::new("table preview offset exceeds PostgreSQL limits"))?;
    let select_sql = preview_select_sql(&schema, &table, sort.as_ref(), &primary_key_columns);
    let relation = format!("{}.{}", quote_identifier(&schema), quote_identifier(&table));
    validate_read(&select_sql)?;
    let profile_for_task = profile;
    let sessions = &provider.metadata_sessions;
    runtime::run(move || {
        preview_table_page_on_runtime(
            sessions,
            &profile_for_task,
            &select_sql,
            &relation,
            schema,
            table,
            limit,
            offset_for_query,
            primary_key_columns,
            sort,
            cursor,
            filters,
        )
    })
}

fn preview_select_sql(
    schema: &str,
    table: &str,
    sort: Option<&(String, bool)>,
    primary_key_columns: &[String],
) -> String {
    let relation = format!("{}.{}", quote_identifier(schema), quote_identifier(table));
    let order_sql = sort.map_or_else(String::new, |(column, descending)| {
        let direction = if *descending { "DESC" } else { "ASC" };
        let order_columns = if primary_key_columns.first() == Some(column) {
            primary_key_columns
        } else {
            std::slice::from_ref(column)
        };
        let columns = order_columns
            .iter()
            .map(|column| format!("{} {direction}", quote_identifier(column)))
            .collect::<Vec<_>>()
            .join(", ");
        format!(" ORDER BY {columns}")
    });
    format!("SELECT * FROM {relation}{order_sql} LIMIT $1::bigint OFFSET $2::bigint")
}

#[allow(clippy::too_many_arguments)]
fn preview_table_page_on_runtime(
    sessions: &MetadataSessionCache,
    profile: &PostgresConnectionProfile,
    sql: &str,
    relation: &str,
    schema: String,
    table: String,
    limit: usize,
    offset: i64,
    primary_key_columns: Vec<String>,
    sort: Option<(String, bool)>,
    page_cursor: Option<TableDataCursor>,
    filters: Vec<TableColumnFilter>,
) -> Result<QueryResult, DatabaseError> {
    let runtime = runtime::handle()?;
    runtime.block_on(async {
        for attempt in 0..=1 {
            let session = sessions.get_or_connect(profile).await?;
            let (result, is_closed) = {
                let client = session.client.lock().await;
                let result = execute_table_preview_client(
                    &session,
                    &client,
                    sql,
                    relation,
                    schema.clone(),
                    table.clone(),
                    limit,
                    offset,
                    &primary_key_columns,
                    sort.as_ref(),
                    page_cursor.as_ref(),
                    &filters,
                )
                .await;
                (result, client.is_closed())
            };
            if is_closed {
                sessions.invalidate_if_current(&session).await;
                if attempt == 0 {
                    continue;
                }
            }
            return result;
        }
        unreachable!("the bounded preview reconnect loop always returns")
    })
}

pub(crate) fn execute_query_on_runtime(
    sessions: &MetadataSessionCache,
    profile: &PostgresConnectionProfile,
    sql: &str,
) -> Result<QueryResult, DatabaseError> {
    let runtime = runtime::handle()?;
    runtime.block_on(async {
        for attempt in 0..=1 {
            let session = sessions.get_or_connect(profile).await?;
            let (result, is_closed) = {
                let mut client = session.client.lock().await;
                let result = execute_query_client(&session, &mut client, sql).await;
                (result, client.is_closed())
            };
            if is_closed {
                sessions.invalidate_if_current(&session).await;
                if attempt == 0 {
                    continue;
                }
            }
            return result;
        }
        unreachable!("the bounded query reconnect loop always returns")
    })
}

async fn execute_query_client(
    session: &MetadataSession,
    client: &mut Client,
    sql: &str,
) -> Result<QueryResult, DatabaseError> {
    let statement = session.read_statement(client, sql).await?;
    let transaction = client.transaction().await.map_err(|error| {
        DatabaseError::new(format!(
            "could not begin read-only query transaction: {error}"
        ))
    })?;
    let portal = transaction
        .bind(&statement, &[])
        .await
        .map_err(|error| DatabaseError::new(format!("could not bind query portal: {error}")))?;
    let mut row_stream = Box::pin(
        transaction
            .query_portal_raw(&portal, (MAX_QUERY_ROWS + 1) as i32)
            .await
            .map_err(|error| DatabaseError::new(format!("query failed: {error}")))?,
    );
    let columns = statement
        .columns()
        .iter()
        .map(|column| column.name().to_owned())
        .collect::<Vec<_>>();
    let column_types = statement
        .columns()
        .iter()
        .map(|column| column.type_().name().to_owned())
        .collect::<Vec<_>>();
    let column_enum_values = statement
        .columns()
        .iter()
        .map(|column| postgres_enum_values(column.type_()).map(<[String]>::to_vec))
        .collect::<Vec<_>>();
    let decoded_rows = async {
        let mut rows = Vec::with_capacity(MAX_QUERY_ROWS.min(128));
        let mut null_cells = Vec::with_capacity(MAX_QUERY_ROWS.min(128));
        let mut truncated_cells = Vec::with_capacity(MAX_QUERY_ROWS.min(128));
        let mut total_bytes = 0usize;
        let mut truncated = false;
        while let Some(row) = row_stream
            .try_next()
            .await
            .map_err(|error| DatabaseError::new(format!("query failed: {error}")))?
        {
            if rows.len() == MAX_QUERY_ROWS {
                truncated = true;
                continue;
            }

            let decoded = decode_row(&row, columns.len(), &mut total_bytes)?;
            rows.push(decoded.values);
            null_cells.push(decoded.null_cells);
            truncated_cells.push(decoded.truncated_cells);
        }
        Ok::<_, DatabaseError>((rows, null_cells, truncated_cells, truncated))
    }
    .await;
    drop(row_stream);

    transaction.rollback().await.map_err(|error| {
        DatabaseError::new(format!(
            "could not close read-only query transaction: {error}"
        ))
    })?;
    let (rows, null_cells, truncated_cells, truncated) = decoded_rows?;

    Ok(QueryResult {
        columns,
        column_types,
        column_enum_values,
        offset: 0,
        null_cells,
        truncated_cells,
        limit: MAX_QUERY_ROWS,
        has_next: truncated,
        rows,
        truncated,
        editable: None,
    })
}

#[allow(clippy::too_many_arguments)]
async fn execute_table_preview_client(
    session: &MetadataSession,
    client: &Client,
    sql: &str,
    relation: &str,
    schema: String,
    table: String,
    limit: usize,
    offset: i64,
    primary_key_columns_hint: &[String],
    sort: Option<&(String, bool)>,
    page_cursor: Option<&TableDataCursor>,
    filters: &[TableColumnFilter],
) -> Result<QueryResult, DatabaseError> {
    let key_tiebreaker_was_requested = sort.is_some_and(|(column, _)| {
        primary_key_columns_hint.first() == Some(column) && primary_key_columns_hint.len() > 1
    });
    let preview_statement = async {
        match session.read_statement(client, sql).await {
            Ok(statement) => Ok(statement),
            Err(_) if key_tiebreaker_was_requested => {
                let fallback_sql = preview_select_sql(&schema, &table, sort, &[]);
                session.read_statement(client, &fallback_sql).await
            }
            Err(error) => Err(error),
        }
    };
    let (base_statement, primary_key_statement) = futures_util::try_join!(
        preview_statement,
        session.read_statement(client, TABLE_PRIMARY_KEY_QUERY),
    )?;
    let column_type_specs = base_statement
        .columns()
        .iter()
        .map(|column| {
            (
                column.name().to_owned(),
                qualified_type_name(column.type_()),
                is_text_filter_type(column.type_().name()),
            )
        })
        .collect::<Vec<_>>();
    let (filter_clause, filter_values) = build_filter_predicates(filters, &column_type_specs)?;
    let statement = if filters.is_empty() {
        base_statement
    } else {
        let filtered_sql = build_filtered_offset_sql(
            relation,
            sort,
            primary_key_columns_hint,
            &filter_clause,
            filter_values.len(),
        );
        session.read_statement(client, &filtered_sql).await?
    };
    let columns = statement
        .columns()
        .iter()
        .map(|column| column.name().to_owned())
        .collect::<Vec<_>>();
    let column_types = statement
        .columns()
        .iter()
        .map(|column| column.type_().name().to_owned())
        .collect::<Vec<_>>();
    let column_enum_values = statement
        .columns()
        .iter()
        .map(|column| postgres_enum_values(column.type_()).map(<[String]>::to_vec))
        .collect::<Vec<_>>();
    // The normal preview query includes LIMIT limit+1 (limit is capped at 100).
    // Stream and decode rows as they arrive so a page of wide rows does not keep
    // both every raw PostgreSQL row and every decoded display value in memory.
    // The independent primary-key lookup can share a network round trip with
    // the bounded table-page read on tokio-postgres' pipelined connection.
    let primary_key_parameters: [&(dyn tokio_postgres::types::ToSql + Sync); 1] = [&relation];
    let cursor_candidate = page_cursor
        .zip(sort)
        .filter(|_| offset > 0)
        .filter(|(_, (sort_column, _))| primary_key_columns_hint.first() == Some(sort_column))
        .filter(|(cursor, _)| cursor.values.len() == primary_key_columns_hint.len());
    let keyset_statement = if let Some((cursor, (_, descending))) = cursor_candidate {
        let key_columns = primary_key_columns_hint
            .iter()
            .map(|name| {
                statement
                    .columns()
                    .iter()
                    .find(|column| column.name() == name)
                    .map(|column| (name.as_str(), qualified_type_name(column.type_())))
            })
            .collect::<Option<Vec<_>>>();
        if let Some(key_columns) = key_columns {
            let keyset_sql = build_keyset_sql_with_filters(
                relation,
                &key_columns,
                *descending,
                cursor.direction,
                &filter_clause,
                filter_values.len(),
            );
            session.read_statement(client, &keyset_sql).await.ok()
        } else {
            None
        }
    } else {
        None
    };
    let requested_page_future = async {
        if let (Some(statement), Some((cursor, _))) = (&keyset_statement, cursor_candidate) {
            read_preview_rows_after_filtered(
                client,
                statement,
                columns.len(),
                limit,
                &filter_values,
                &cursor.values,
            )
            .await
        } else if filters.is_empty() {
            read_preview_rows(client, &statement, columns.len(), limit, offset).await
        } else {
            read_preview_rows_filtered(
                client,
                &statement,
                columns.len(),
                limit,
                offset,
                &filter_values,
            )
            .await
        }
    };
    let (requested_rows, primary_key_result) = futures_util::join!(
        requested_page_future,
        client.query(&primary_key_statement, &primary_key_parameters),
    );
    let primary_key_columns = primary_key_result
        .ok()
        .map(|rows| {
            rows.into_iter()
                .filter_map(|row| row.try_get::<_, String>(0).ok())
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    #[cfg(test)]
    let requested_page_succeeded = requested_rows.is_ok();
    let (mut rows, used_keyset) = if keyset_statement.is_none() {
        (requested_rows?, false)
    } else if primary_key_columns.as_slice() == primary_key_columns_hint {
        match requested_rows {
            Ok(rows) => (rows, true),
            Err(_) => (
                if filters.is_empty() {
                    read_preview_rows(client, &statement, columns.len(), limit, offset).await?
                } else {
                    read_preview_rows_filtered(
                        client,
                        &statement,
                        columns.len(),
                        limit,
                        offset,
                        &filter_values,
                    )
                    .await?
                },
                false,
            ),
        }
    } else {
        (
            if filters.is_empty() {
                read_preview_rows(client, &statement, columns.len(), limit, offset).await?
            } else {
                read_preview_rows_filtered(
                    client,
                    &statement,
                    columns.len(),
                    limit,
                    offset,
                    &filter_values,
                )
                .await?
            },
            false,
        )
    };
    let needs_composite_pk_order = !used_keyset
        && primary_key_columns.len() > 1
        && sort.is_some_and(|(sort_column, _)| primary_key_columns.first() == Some(sort_column))
        && primary_key_columns_hint != primary_key_columns;
    if needs_composite_pk_order {
        let stable_sql = if filters.is_empty() {
            preview_select_sql(&schema, &table, sort, &primary_key_columns)
        } else {
            build_filtered_offset_sql(
                relation,
                sort,
                &primary_key_columns,
                &filter_clause,
                filter_values.len(),
            )
        };
        let stable_statement = session.read_statement(client, &stable_sql).await?;
        rows = if filters.is_empty() {
            read_preview_rows(client, &stable_statement, columns.len(), limit, offset).await?
        } else {
            read_preview_rows_filtered(
                client,
                &stable_statement,
                columns.len(),
                limit,
                offset,
                &filter_values,
            )
            .await?
        };
    }
    #[cfg(test)]
    if std::env::var_os("TABLEX_TEST_PG_PREVIEW_TRACE").is_some() {
        eprintln!(
            "[tableX preview trace] keyset_prepared={}, live_pk_matches_hint={}, requested_succeeded={}, keyset_used={used_keyset}",
            keyset_statement.is_some(),
            primary_key_columns.as_slice() == primary_key_columns_hint,
            requested_page_succeeded,
        );
    }
    let is_previous_page = used_keyset
        && page_cursor.is_some_and(|cursor| cursor.direction == TableDataCursorDirection::Before);
    let has_next = if is_previous_page {
        offset > 0
    } else {
        rows.len() > limit
    };
    rows.truncate(limit);
    if is_previous_page {
        rows.reverse();
    }
    let null_cells = rows.null_cells;
    let truncated_cells = rows.truncated_cells;
    let rows = rows.rows;
    Ok(QueryResult {
        columns,
        column_types,
        column_enum_values,
        null_cells,
        truncated_cells,
        offset: usize::try_from(offset).expect("the offset was validated before execution"),
        limit,
        has_next,
        truncated: has_next,
        rows,
        editable: Some(EditableTable {
            schema,
            table,
            primary_key_columns,
        }),
    })
}

fn qualified_type_name(column_type: &tokio_postgres::types::Type) -> String {
    let name = quote_identifier(column_type.name());
    format!("{}.{}", quote_identifier(column_type.schema()), name)
}

type PreviewColumnType = (String, String, bool);

fn is_text_filter_type(type_name: &str) -> bool {
    matches!(
        type_name.to_ascii_lowercase().as_str(),
        "text" | "varchar" | "bpchar" | "name" | "citext"
    )
}

fn build_filter_predicates(
    filters: &[TableColumnFilter],
    columns: &[PreviewColumnType],
) -> Result<(String, Vec<String>), DatabaseError> {
    if filters.len() > MAX_TABLE_FILTERS {
        return Err(DatabaseError::new(format!(
            "A table preview supports at most {MAX_TABLE_FILTERS} column filters"
        )));
    }

    let mut predicates = Vec::with_capacity(filters.len());
    let mut values = Vec::with_capacity(filters.len());
    for filter in filters {
        let Some((_, type_name, is_text)) =
            columns.iter().find(|(name, _, _)| name == &filter.column)
        else {
            return Err(DatabaseError::new(
                "A filtered column is no longer present; refresh the table",
            ));
        };
        let quoted_column = quote_identifier(&filter.column);
        match filter.operator {
            TableFilterOperator::IsNull => {
                predicates.push(format!("{quoted_column} IS NULL"));
            }
            TableFilterOperator::IsNotNull => {
                predicates.push(format!("{quoted_column} IS NOT NULL"));
            }
            operator => {
                let value = filter.value.as_deref().ok_or_else(|| {
                    DatabaseError::new("Enter a value for each active column filter")
                })?;
                if value.len() > MAX_TABLE_FILTER_VALUE_BYTES {
                    return Err(DatabaseError::new(format!(
                        "Column filter values are limited to {MAX_TABLE_FILTER_VALUE_BYTES} bytes"
                    )));
                }
                let parameter = values.len() + 1;
                match operator {
                    TableFilterOperator::Contains | TableFilterOperator::StartsWith => {
                        if !is_text {
                            return Err(DatabaseError::new(
                                "Contains and starts-with filters require a text column",
                            ));
                        }
                        let escaped = value
                            .replace('\\', "\\\\")
                            .replace('%', "\\%")
                            .replace('_', "\\_");
                        let pattern = if operator == TableFilterOperator::Contains {
                            format!("%{escaped}%")
                        } else {
                            format!("{escaped}%")
                        };
                        predicates.push(format!(
                            "{quoted_column}::text ILIKE ${parameter}::text ESCAPE '\\'"
                        ));
                        values.push(pattern);
                    }
                    TableFilterOperator::Equals
                    | TableFilterOperator::NotEquals
                    | TableFilterOperator::GreaterThan
                    | TableFilterOperator::LessThan => {
                        let comparison = match operator {
                            TableFilterOperator::Equals => "=",
                            TableFilterOperator::NotEquals => "<>",
                            TableFilterOperator::GreaterThan => ">",
                            TableFilterOperator::LessThan => "<",
                            _ => unreachable!(),
                        };
                        predicates.push(format!(
                            "{quoted_column} {comparison} ((${}::text)::{type_name})",
                            parameter
                        ));
                        values.push(value.to_owned());
                    }
                    TableFilterOperator::IsNull | TableFilterOperator::IsNotNull => {
                        unreachable!()
                    }
                }
            }
        }
    }
    Ok((predicates.join(" AND "), values))
}

fn order_by_sql(sort: Option<&(String, bool)>, primary_key_columns: &[String]) -> String {
    sort.map_or_else(String::new, |(column, descending)| {
        let direction = if *descending { "DESC" } else { "ASC" };
        let order_columns = if primary_key_columns.first() == Some(column) {
            primary_key_columns
        } else {
            std::slice::from_ref(column)
        };
        let columns = order_columns
            .iter()
            .map(|column| format!("{} {direction}", quote_identifier(column)))
            .collect::<Vec<_>>()
            .join(", ");
        format!(" ORDER BY {columns}")
    })
}

fn build_filtered_offset_sql(
    relation: &str,
    sort: Option<&(String, bool)>,
    primary_key_columns: &[String],
    filter_clause: &str,
    filter_count: usize,
) -> String {
    let limit_parameter = filter_count + 1;
    let offset_parameter = filter_count + 2;
    format!(
        "SELECT * FROM {relation} WHERE {filter_clause}{order} LIMIT ${limit_parameter}::bigint OFFSET ${offset_parameter}::bigint",
        order = order_by_sql(sort, primary_key_columns),
    )
}

#[cfg(test)]
fn build_keyset_sql(
    relation: &str,
    columns: &[(&str, String)],
    descending: bool,
    cursor_direction: TableDataCursorDirection,
) -> String {
    build_keyset_sql_with_filters(relation, columns, descending, cursor_direction, "", 0)
}

fn build_keyset_sql_with_filters(
    relation: &str,
    columns: &[(&str, String)],
    descending: bool,
    cursor_direction: TableDataCursorDirection,
    filter_clause: &str,
    filter_count: usize,
) -> String {
    let comparison = match (descending, cursor_direction) {
        (false, TableDataCursorDirection::After) | (true, TableDataCursorDirection::Before) => ">",
        (false, TableDataCursorDirection::Before) | (true, TableDataCursorDirection::After) => "<",
    };
    let query_descending = descending != (cursor_direction == TableDataCursorDirection::Before);
    let order_direction = if query_descending { "DESC" } else { "ASC" };
    let column_names = columns
        .iter()
        .map(|(name, _)| quote_identifier(name))
        .collect::<Vec<_>>();
    let parameters = columns
        .iter()
        .enumerate()
        .map(|(index, (_, column_type))| {
            format!("(${}::text)::{column_type}", filter_count + index + 1)
        })
        .collect::<Vec<_>>();
    let row_expression = |values: &[String]| {
        if values.len() == 1 {
            values[0].clone()
        } else {
            format!("({})", values.join(", "))
        }
    };
    let left = row_expression(&column_names);
    let right = row_expression(&parameters);
    let order_by = column_names
        .iter()
        .map(|name| format!("{name} {order_direction}"))
        .collect::<Vec<_>>()
        .join(", ");
    let limit_parameter = filter_count + columns.len() + 1;
    let mut predicates = Vec::with_capacity(2);
    if !filter_clause.is_empty() {
        predicates.push(filter_clause.to_owned());
    }
    predicates.push(format!("{left} {comparison} {right}"));
    format!(
        "SELECT * FROM {relation} WHERE {} ORDER BY {order_by} LIMIT ${limit_parameter}::bigint",
        predicates.join(" AND ")
    )
}

async fn read_preview_rows(
    client: &Client,
    statement: &tokio_postgres::Statement,
    column_count: usize,
    limit: usize,
    offset: i64,
) -> Result<DecodedRows, DatabaseError> {
    let page_size = (limit + 1) as i64;
    let page_parameters: [&(dyn tokio_postgres::types::ToSql + Sync); 2] = [&page_size, &offset];
    read_preview_rows_with_parameters(client, statement, column_count, limit, &page_parameters)
        .await
}

async fn read_preview_rows_filtered(
    client: &Client,
    statement: &tokio_postgres::Statement,
    column_count: usize,
    limit: usize,
    offset: i64,
    filter_values: &[String],
) -> Result<DecodedRows, DatabaseError> {
    let page_size = (limit + 1) as i64;
    let mut parameters: Vec<&(dyn tokio_postgres::types::ToSql + Sync)> = filter_values
        .iter()
        .map(|value| value as &(dyn tokio_postgres::types::ToSql + Sync))
        .collect();
    parameters.push(&page_size);
    parameters.push(&offset);
    read_preview_rows_with_parameters(client, statement, column_count, limit, &parameters).await
}

async fn read_preview_rows_after_filtered(
    client: &Client,
    statement: &tokio_postgres::Statement,
    column_count: usize,
    limit: usize,
    filter_values: &[String],
    cursor: &[String],
) -> Result<DecodedRows, DatabaseError> {
    let page_size = (limit + 1) as i64;
    let mut parameters: Vec<&(dyn tokio_postgres::types::ToSql + Sync)> = filter_values
        .iter()
        .map(|value| value as &(dyn tokio_postgres::types::ToSql + Sync))
        .collect();
    parameters.extend(
        cursor
            .iter()
            .map(|value| value as &(dyn tokio_postgres::types::ToSql + Sync)),
    );
    parameters.push(&page_size);
    read_preview_rows_with_parameters(client, statement, column_count, limit, &parameters).await
}

async fn read_preview_rows_with_parameters(
    client: &Client,
    statement: &tokio_postgres::Statement,
    column_count: usize,
    limit: usize,
    parameters: &[&(dyn tokio_postgres::types::ToSql + Sync)],
) -> Result<DecodedRows, DatabaseError> {
    let mut row_stream = Box::pin(
        client
            .query_raw(statement, parameters.iter().copied())
            .await
            .map_err(|error| DatabaseError::new(format!("table preview failed: {error}")))?,
    );
    let mut total_bytes = 0usize;
    let mut rows = DecodedRows {
        rows: Vec::with_capacity(limit + 1),
        null_cells: Vec::with_capacity(limit + 1),
        truncated_cells: Vec::with_capacity(limit + 1),
    };
    let mut decode_error = None;
    while let Some(row) = row_stream
        .try_next()
        .await
        .map_err(|error| DatabaseError::new(format!("table preview failed: {error}")))?
    {
        if decode_error.is_none() {
            match decode_row(&row, column_count, &mut total_bytes) {
                Ok(decoded) => {
                    rows.rows.push(decoded.values);
                    rows.null_cells.push(decoded.null_cells);
                    rows.truncated_cells.push(decoded.truncated_cells);
                }
                Err(error) => decode_error = Some(error),
            }
        }
    }
    if let Some(error) = decode_error {
        return Err(error);
    }
    Ok(rows)
}

fn decode_row(
    row: &tokio_postgres::Row,
    column_count: usize,
    total_bytes: &mut usize,
) -> Result<DecodedRow, DatabaseError> {
    let mut cells = Vec::with_capacity(column_count);
    let mut null_cells = Vec::with_capacity(column_count);
    let mut truncated_cells = Vec::with_capacity(column_count);
    for index in 0..column_count {
        let remaining = MAX_QUERY_RESULT_BYTES.saturating_sub(*total_bytes);
        let mut is_truncated = false;
        let (cell, is_null) = cell_to_string_with_null(
            row,
            index,
            MAX_QUERY_CELL_BYTES.min(remaining),
            &mut is_truncated,
        )?;
        *total_bytes += cell.len();
        cells.push(cell);
        null_cells.push(is_null);
        truncated_cells.push(is_truncated);
    }
    Ok(DecodedRow {
        values: cells,
        null_cells,
        truncated_cells,
    })
}

#[cfg(test)]
pub(crate) fn compare_decoder_timings(
    profile: PostgresConnectionProfile,
    sql: String,
) -> Result<(Duration, Duration), DatabaseError> {
    use super::value::cell_to_string_probe_chain;

    runtime::run(move || {
        let runtime = runtime::handle()?;
        runtime.block_on(async move {
            let config = read_only_connection_config(&profile);
            let (client, connection) =
                connect_with_timeout(config.connect(NoTls), &profile, |error| {
                    DatabaseError::new(format_connection_error(&profile, error))
                })
                .await?;
            tokio::spawn(async move {
                if let Err(error) = connection.await {
                    eprintln!("PostgreSQL decoder benchmark connection closed: {error}");
                }
            });
            let rows = client.query(&sql, &[]).await.map_err(|error| {
                DatabaseError::new(format!("decoder benchmark query failed: {error}"))
            })?;
            let expected_cell_count: usize = rows.iter().map(|row| row.len()).sum();
            let mut optimized_times = Vec::with_capacity(7);
            let mut probe_chain_times = Vec::with_capacity(7);

            for iteration in 0usize..7 {
                let (optimized_duration, optimized_values) = if iteration.is_multiple_of(2) {
                    decode_benchmark_rows(&rows, cell_to_string)?
                } else {
                    decode_benchmark_rows(&rows, cell_to_string_probe_chain)?
                };
                let (probe_chain_duration, probe_chain_values) = if iteration.is_multiple_of(2) {
                    decode_benchmark_rows(&rows, cell_to_string_probe_chain)?
                } else {
                    decode_benchmark_rows(&rows, cell_to_string)?
                };
                assert_eq!(optimized_values, probe_chain_values);
                assert_eq!(optimized_values.len(), expected_cell_count);

                if iteration.is_multiple_of(2) {
                    optimized_times.push(optimized_duration);
                    probe_chain_times.push(probe_chain_duration);
                } else {
                    optimized_times.push(probe_chain_duration);
                    probe_chain_times.push(optimized_duration);
                }
            }

            optimized_times.sort_unstable();
            probe_chain_times.sort_unstable();
            Ok((optimized_times[3], probe_chain_times[3]))
        })
    })
}

#[cfg(test)]
fn decode_benchmark_rows(
    rows: &[tokio_postgres::Row],
    decode_cell: fn(&tokio_postgres::Row, usize, usize) -> Result<String, DatabaseError>,
) -> Result<(Duration, Vec<String>), DatabaseError> {
    let cell_count = rows.iter().map(|row| row.len()).sum();
    let start = Instant::now();
    let mut values = Vec::with_capacity(cell_count);
    for row in rows {
        for index in 0..row.len() {
            values.push(decode_cell(row, index, MAX_QUERY_CELL_BYTES)?);
        }
    }
    let elapsed = start.elapsed();
    std::hint::black_box(&values);
    Ok((elapsed, values))
}

#[cfg(test)]
mod preview_sql_tests {
    use super::{
        MAX_TABLE_FILTER_VALUE_BYTES, build_cell_update_sql, build_filter_predicates,
        build_filtered_offset_sql, build_keyset_sql, build_keyset_sql_with_filters,
        preview_select_sql,
    };
    use crate::domain::query::{
        CellUpdateRequest, TableColumnFilter, TableDataCursorDirection, TableFilterOperator,
    };

    #[test]
    fn preview_sql_keeps_ordinary_offset_and_sort_semantics() {
        let sort = ("id".to_owned(), false);
        let sql = preview_select_sql("public", "events", Some(&sort), &[]);
        assert_eq!(
            sql,
            "SELECT * FROM \"public\".\"events\" ORDER BY \"id\" ASC LIMIT $1::bigint OFFSET $2::bigint"
        );
    }

    #[test]
    fn composite_primary_key_sort_adds_stable_tie_break_columns() {
        let sort = ("tenant_id".to_owned(), false);
        let keys = vec!["tenant_id".to_owned(), "event_id".to_owned()];
        let sql = preview_select_sql("public", "events", Some(&sort), &keys);
        assert!(sql.contains("ORDER BY \"tenant_id\" ASC, \"event_id\" ASC"));
    }

    #[test]
    fn keyset_sql_uses_exclusive_boundaries_and_reverses_previous_pages() {
        let relation = "\"public\".\"events\"";
        let ascending_after = build_keyset_sql(
            relation,
            &[("id", "\"pg_catalog\".\"int8\"".to_owned())],
            false,
            TableDataCursorDirection::After,
        );
        assert_eq!(
            ascending_after,
            "SELECT * FROM \"public\".\"events\" WHERE \"id\" > ($1::text)::\"pg_catalog\".\"int8\" ORDER BY \"id\" ASC LIMIT $2::bigint"
        );

        let descending_after = build_keyset_sql(
            relation,
            &[("id", "\"pg_catalog\".\"int8\"".to_owned())],
            true,
            TableDataCursorDirection::After,
        );
        assert!(descending_after.contains("WHERE \"id\" < ($1::text)"));
        assert!(descending_after.contains("ORDER BY \"id\" DESC"));

        let ascending_before = build_keyset_sql(
            relation,
            &[("id", "\"pg_catalog\".\"int8\"".to_owned())],
            false,
            TableDataCursorDirection::Before,
        );
        assert!(ascending_before.contains("WHERE \"id\" < ($1::text)"));
        assert!(ascending_before.contains("ORDER BY \"id\" DESC"));

        let descending_before = build_keyset_sql(
            relation,
            &[("id", "\"pg_catalog\".\"int8\"".to_owned())],
            true,
            TableDataCursorDirection::Before,
        );
        assert!(descending_before.contains("WHERE \"id\" > ($1::text)"));
        assert!(descending_before.contains("ORDER BY \"id\" ASC"));
    }

    #[test]
    fn composite_keyset_uses_typed_tuple_boundaries() {
        let sql = build_keyset_sql(
            "\"public\".\"events\"",
            &[
                ("tenant_id", "\"pg_catalog\".\"int4\"".to_owned()),
                ("event_id", "\"pg_catalog\".\"int8\"".to_owned()),
            ],
            false,
            TableDataCursorDirection::After,
        );
        assert_eq!(
            sql,
            "SELECT * FROM \"public\".\"events\" WHERE (\"tenant_id\", \"event_id\") > (($1::text)::\"pg_catalog\".\"int4\", ($2::text)::\"pg_catalog\".\"int8\") ORDER BY \"tenant_id\" ASC, \"event_id\" ASC LIMIT $3::bigint"
        );
    }

    #[test]
    fn table_filters_quote_identifiers_and_bind_escaped_values() {
        let filters = vec![
            TableColumnFilter {
                column: "display\"name".into(),
                operator: TableFilterOperator::Contains,
                value: Some("a%_\\b'".into()),
            },
            TableColumnFilter {
                column: "deleted_at".into(),
                operator: TableFilterOperator::IsNull,
                value: None,
            },
        ];
        let columns = vec![
            (
                "display\"name".into(),
                "\"pg_catalog\".\"text\"".into(),
                true,
            ),
            (
                "deleted_at".into(),
                "\"pg_catalog\".\"timestamptz\"".into(),
                false,
            ),
        ];
        let (predicate, values) = build_filter_predicates(&filters, &columns).unwrap();
        assert_eq!(
            predicate,
            r#""display""name"::text ILIKE $1::text ESCAPE '\' AND "deleted_at" IS NULL"#
        );
        assert_eq!(values, ["%a\\%\\_\\\\b'%"]);
        let sql =
            build_filtered_offset_sql("\"public\".\"people\"", None, &[], &predicate, values.len());
        assert!(sql.ends_with("LIMIT $2::bigint OFFSET $3::bigint"));
    }

    #[test]
    fn typed_comparisons_and_null_only_filters_keep_placeholder_order() {
        let filters = vec![
            TableColumnFilter {
                column: "amount".into(),
                operator: TableFilterOperator::GreaterThan,
                value: Some("12.50".into()),
            },
            TableColumnFilter {
                column: "active".into(),
                operator: TableFilterOperator::Equals,
                value: Some("true".into()),
            },
        ];
        let columns = vec![
            ("amount".into(), "\"pg_catalog\".\"numeric\"".into(), false),
            ("active".into(), "\"pg_catalog\".\"bool\"".into(), false),
        ];
        let (predicate, values) = build_filter_predicates(&filters, &columns).unwrap();
        assert_eq!(
            predicate,
            r#""amount" > (($1::text)::"pg_catalog"."numeric") AND "active" = (($2::text)::"pg_catalog"."bool")"#
        );
        assert_eq!(values, ["12.50", "true"]);

        let null_filter = [TableColumnFilter {
            column: "active".into(),
            operator: TableFilterOperator::IsNotNull,
            value: None,
        }];
        let (predicate, values) = build_filter_predicates(&null_filter, &columns).unwrap();
        assert_eq!(predicate, "\"active\" IS NOT NULL");
        assert!(values.is_empty());
        let keyset = build_keyset_sql_with_filters(
            "\"public\".\"people\"",
            &[("id", "\"pg_catalog\".\"int8\"".into())],
            false,
            TableDataCursorDirection::After,
            &predicate,
            values.len(),
        );
        assert!(keyset.contains("WHERE \"active\" IS NOT NULL AND \"id\" > ($1::text)"));
        assert!(keyset.ends_with("LIMIT $2::bigint"));
    }

    #[test]
    fn enum_equality_filter_uses_the_enum_type_cast() {
        let filter = [TableColumnFilter {
            column: "state".into(),
            operator: TableFilterOperator::Equals,
            value: Some("in review".into()),
        }];
        let columns = vec![("state".into(), "\"app\".\"workflow_state\"".into(), false)];
        let (predicate, values) = build_filter_predicates(&filter, &columns).unwrap();
        assert_eq!(
            predicate,
            r#""state" = (($1::text)::"app"."workflow_state")"#
        );
        assert_eq!(values, ["in review"]);
    }

    #[test]
    fn filters_reject_unknown_columns_unsupported_contains_and_oversized_values() {
        let columns = vec![("count".into(), "\"pg_catalog\".\"int4\"".into(), false)];
        let unknown = [TableColumnFilter {
            column: "other".into(),
            operator: TableFilterOperator::Equals,
            value: Some("1".into()),
        }];
        assert!(build_filter_predicates(&unknown, &columns).is_err());
        let unsupported = [TableColumnFilter {
            column: "count".into(),
            operator: TableFilterOperator::Contains,
            value: Some("1".into()),
        }];
        assert!(build_filter_predicates(&unsupported, &columns).is_err());
        let oversized = [TableColumnFilter {
            column: "count".into(),
            operator: TableFilterOperator::Equals,
            value: Some("x".repeat(MAX_TABLE_FILTER_VALUE_BYTES + 1)),
        }];
        assert!(build_filter_predicates(&oversized, &columns).is_err());
    }

    #[test]
    fn cell_update_sql_binds_value_and_checks_original_value_and_composite_key() {
        let request = CellUpdateRequest {
            schema: "public".into(),
            table: "items".into(),
            column: "label\"value".into(),
            primary_key_columns: vec!["tenant_id".into(), "item_id".into()],
            primary_key_values: vec!["7".into(), "5a72e91a-9b81-4f81-b6a1-68f0ca7a8b77".into()],
            value: Some("x'; DELETE FROM items; --".into()),
            expected_value: Some("old".into()),
        };
        let sql = build_cell_update_sql(
            "\"public\".\"items\"",
            &request,
            "\"pg_catalog\".\"text\"",
            &[
                ("tenant_id".into(), "\"pg_catalog\".\"int4\"".into()),
                ("item_id".into(), "\"pg_catalog\".\"uuid\"".into()),
            ],
        );
        assert_eq!(
            sql,
            "UPDATE \"public\".\"items\" SET \"label\"\"value\" = (($1::text)::\"pg_catalog\".\"text\") WHERE \"tenant_id\" = (($2::text)::\"pg_catalog\".\"int4\") AND \"item_id\" = (($3::text)::\"pg_catalog\".\"uuid\") AND \"label\"\"value\" IS NOT DISTINCT FROM (($4::text)::\"pg_catalog\".\"text\")"
        );
        assert!(!sql.contains("DELETE FROM items"));
    }

    #[test]
    fn enum_cell_update_casts_bound_values_to_the_qualified_enum_type() {
        let request = CellUpdateRequest {
            schema: "app".into(),
            table: "tickets".into(),
            column: "state".into(),
            primary_key_columns: vec!["id".into()],
            primary_key_values: vec!["8".into()],
            value: Some("in review".into()),
            expected_value: Some("draft".into()),
        };
        let sql = build_cell_update_sql(
            "\"app\".\"tickets\"",
            &request,
            "\"app\".\"workflow_state\"",
            &[("id".into(), "\"pg_catalog\".\"int4\"".into())],
        );
        assert_eq!(
            sql,
            "UPDATE \"app\".\"tickets\" SET \"state\" = (($1::text)::\"app\".\"workflow_state\") WHERE \"id\" = (($2::text)::\"pg_catalog\".\"int4\") AND \"state\" IS NOT DISTINCT FROM (($3::text)::\"app\".\"workflow_state\")"
        );
        assert!(!sql.contains("in review"));
    }
}
