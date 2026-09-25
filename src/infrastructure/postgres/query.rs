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

mod preview;

pub(crate) use preview::preview_table_page;
use preview::qualified_type_name;
#[cfg(test)]
use preview::{
    build_filter_predicates, build_filtered_offset_sql, build_keyset_sql,
    build_keyset_sql_with_filters, preview_select_sql,
};

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
#[path = "../../../tests/unit/infrastructure/postgres/query.rs"]
mod tests;
