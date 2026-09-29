use super::connect::{
    connect_with_timeout, read_only_catalog_connection_config, read_only_connection_config,
    rustls_connector,
};
use super::error::{format_connection_error, format_postgres_error, format_tls_connection_error};
use super::model::{
    PostgresConnectionProfile, PostgresServerInfo, PostgresSslMode, PostgresVersion,
};
use super::runtime;
use super::{PostgresInspection, PostgresProvider};
use crate::domain::database_object::{TableCursor, TablePage, TableRelationType, TableSummary};
use crate::infrastructure::error::DatabaseError;
use std::collections::HashMap;
use std::collections::VecDeque;
use std::future::Future;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use tokio_postgres::{Client, NoTls, Statement};

pub(crate) const MAX_METADATA_SCHEMAS: usize = 500;
pub(crate) const MAX_METADATA_DATABASES: usize = 500;
pub(crate) const MAX_TABLE_PAGE_SIZE: usize = 500;
const MAX_CACHED_READ_STATEMENTS: usize = 16;

const TABLE_PAGE_QUERY: &str = "SELECT c.oid::text, n.nspname, c.relname, c.relkind::text
         FROM pg_class c
         JOIN pg_namespace n ON n.oid = c.relnamespace
         WHERE n.nspname <> 'information_schema'
           AND n.nspname NOT LIKE 'pg_%'
           AND ($1 = '' OR c.relname ILIKE '%' || $1 || '%')
           AND ($2::text IS NULL OR n.nspname = $2)
           AND ($3::text IS NULL OR c.relkind::text = $3)
           AND c.relkind IN ('r', 'p', 'v', 'm', 'f')
         ORDER BY n.nspname, c.relname
         LIMIT $4 OFFSET $5";
const TABLE_PAGE_FIRST_QUERY: &str = "WITH catalog_shape AS MATERIALIZED (
             SELECT CASE
                 WHEN class_stats.reltuples < 0 OR namespace_stats.reltuples <= 0 THEN 1000000::real
                 ELSE class_stats.reltuples / namespace_stats.reltuples
             END AS relations_per_schema
             FROM pg_class class_stats
             CROSS JOIN pg_class namespace_stats
             WHERE class_stats.oid = 'pg_catalog.pg_class'::regclass
               AND namespace_stats.oid = 'pg_catalog.pg_namespace'::regclass
         ), sparse_page AS MATERIALIZED (
             SELECT c.oid, n.nspname, c.relname, c.relkind
             FROM catalog_shape shape
             CROSS JOIN pg_class c
             JOIN pg_namespace n ON n.oid = c.relnamespace
             WHERE shape.relations_per_schema < 8
               AND n.nspname <> 'information_schema'
               AND n.nspname NOT LIKE 'pg_%'
               AND ($1 = '' OR c.relname ILIKE '%' || $1 || '%')
               AND ($2::text IS NULL OR c.relkind::text = $2)
               AND c.relkind IN ('r', 'p', 'v', 'm', 'f')
             ORDER BY n.nspname, c.relname
             LIMIT $3
         ), dense_page AS MATERIALIZED (
             SELECT c.oid, n.nspname, c.relname, c.relkind
             FROM catalog_shape shape
             CROSS JOIN pg_namespace n
             CROSS JOIN LATERAL (
                 SELECT c.oid, c.relnamespace, c.relname, c.relkind
                 FROM pg_class c
                 WHERE c.relnamespace = n.oid
                   AND ($1 = '' OR c.relname ILIKE '%' || $1 || '%')
                   AND ($2::text IS NULL OR c.relkind::text = $2)
                   AND c.relkind IN ('r', 'p', 'v', 'm', 'f')
                 ORDER BY c.relname
                 LIMIT $3
             ) c
             WHERE shape.relations_per_schema >= 8
               AND n.nspname <> 'information_schema'
               AND n.nspname NOT LIKE 'pg_%'
             ORDER BY n.nspname, c.relname
             LIMIT $3
         )
         SELECT page.oid::text, page.nspname, page.relname, page.relkind::text
         FROM (
             SELECT * FROM sparse_page
             UNION ALL
             SELECT * FROM dense_page
         ) page
         ORDER BY page.nspname, page.relname
         LIMIT $3";
const TABLE_PAGE_GLOBAL_CURSOR_QUERY: &str = "WITH catalog_shape AS MATERIALIZED (
             SELECT CASE
                 WHEN class_stats.reltuples < 0 OR namespace_stats.reltuples <= 0 THEN 1000000::real
                 ELSE class_stats.reltuples / namespace_stats.reltuples
             END AS relations_per_schema
             FROM pg_class class_stats
             CROSS JOIN pg_class namespace_stats
             WHERE class_stats.oid = 'pg_catalog.pg_class'::regclass
               AND namespace_stats.oid = 'pg_catalog.pg_namespace'::regclass
         ), sparse_page AS MATERIALIZED (
             SELECT c.oid, n.nspname, c.relname, c.relkind
             FROM catalog_shape shape
             CROSS JOIN pg_class c
             JOIN pg_namespace n ON n.oid = c.relnamespace
             WHERE shape.relations_per_schema < 8
               AND n.nspname <> 'information_schema'
               AND n.nspname NOT LIKE 'pg_%'
               AND ($1 = '' OR c.relname ILIKE '%' || $1 || '%')
               AND ($2::text IS NULL OR c.relkind::text = $2)
               AND c.relkind IN ('r', 'p', 'v', 'm', 'f')
               AND (n.nspname, c.relname) > ($4::name, $5::name)
             ORDER BY n.nspname, c.relname
             LIMIT $3
         ), current_page AS MATERIALIZED (
             SELECT c.oid, n.nspname, c.relname, c.relkind
             FROM pg_namespace n
             JOIN LATERAL (
                 SELECT c.oid, c.relnamespace, c.relname, c.relkind
                 FROM pg_class c
                 WHERE c.relnamespace = n.oid
                   AND c.relname > $5::name
                   AND ($1 = '' OR c.relname ILIKE '%' || $1 || '%')
                   AND ($2::text IS NULL OR c.relkind::text = $2)
                   AND c.relkind IN ('r', 'p', 'v', 'm', 'f')
                 ORDER BY c.relname
                 LIMIT $3
             ) c ON TRUE
             CROSS JOIN catalog_shape shape
             WHERE shape.relations_per_schema >= 8
               AND n.nspname = $4
               AND n.nspname <> 'information_schema'
               AND n.nspname NOT LIKE 'pg_%'
             ORDER BY n.nspname, c.relname
             LIMIT $3
         ), later_page AS MATERIALIZED (
             SELECT c.oid, n.nspname, c.relname, c.relkind
             FROM pg_namespace n
             CROSS JOIN LATERAL (
                 SELECT c.oid, c.relnamespace, c.relname, c.relkind
                 FROM pg_class c
                 WHERE c.relnamespace = n.oid
                   AND ($1 = '' OR c.relname ILIKE '%' || $1 || '%')
                   AND ($2::text IS NULL OR c.relkind::text = $2)
                   AND c.relkind IN ('r', 'p', 'v', 'm', 'f')
                 ORDER BY c.relname
                 LIMIT $3
             ) c
             CROSS JOIN catalog_shape shape
             WHERE shape.relations_per_schema >= 8
               AND n.nspname > $4
               AND n.nspname <> 'information_schema'
               AND n.nspname NOT LIKE 'pg_%'
             ORDER BY n.nspname, c.relname
             LIMIT $3
         )
         SELECT page.oid::text, page.nspname, page.relname, page.relkind::text
         FROM (
             SELECT * FROM sparse_page
             UNION ALL
             SELECT * FROM current_page
             UNION ALL
             SELECT * FROM later_page
             WHERE (SELECT count(*) FROM current_page) < $3
         ) page
         ORDER BY page.nspname, page.relname
         LIMIT $3";
const TABLE_PAGE_SCHEMA_CURSOR_QUERY: &str =
    "SELECT c.oid::text, n.nspname, c.relname, c.relkind::text
         FROM pg_class c
         JOIN pg_namespace n ON n.oid = c.relnamespace
         WHERE n.nspname = $2
           AND c.relname > $5::name
           AND ($1 = '' OR c.relname ILIKE '%' || $1 || '%')
           AND ($3::text IS NULL OR c.relkind::text = $3)
           AND c.relkind IN ('r', 'p', 'v', 'm', 'f')
         ORDER BY c.relname
         LIMIT $4";
const DATABASE_NAMES_QUERY: &str = "SELECT datname
         FROM pg_database
         WHERE datistemplate = false
           AND datallowconn = true
           AND has_database_privilege(datname, 'CONNECT')
         ORDER BY datname
         LIMIT $1::int";
const SCHEMA_NAMES_QUERY: &str = "SELECT nspname
         FROM pg_namespace
         WHERE nspname <> 'information_schema' AND nspname NOT LIKE 'pg_%'
         ORDER BY nspname
         LIMIT $1::int";
const SERVER_INFO_QUERY: &str = "SELECT current_database(), current_user, current_setting('server_version'), current_setting('server_version_num')";

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct TableListRequest {
    pub search: String,
    pub schema: Option<String>,
    pub relation_type: Option<TableRelationType>,
    pub limit: usize,
    pub offset: usize,
    pub after: Option<TableCursor>,
}

impl TableListRequest {
    fn normalized(self) -> Result<Self, DatabaseError> {
        if self.search.len() > 256 {
            return Err(DatabaseError::new("Table search is limited to 256 bytes"));
        }
        let schema = self.schema.filter(|value| !value.is_empty());
        let after = self.after.filter(|cursor| {
            schema
                .as_ref()
                .is_none_or(|schema| cursor.schema == *schema)
        });
        Ok(Self {
            search: self.search.trim().to_owned(),
            schema,
            relation_type: self.relation_type,
            limit: self.limit.clamp(1, MAX_TABLE_PAGE_SIZE),
            offset: self.offset,
            after,
        })
    }
}

pub(crate) type TableListResult = Result<TablePage, DatabaseError>;

#[derive(Default)]
pub(super) struct MetadataSessionCache {
    generation: AtomicU64,
    session: tokio::sync::Mutex<Option<Arc<MetadataSession>>>,
}

pub(super) struct MetadataSession {
    generation: u64,
    identity: MetadataSessionIdentity,
    pub(super) client: tokio::sync::Mutex<Client>,
    pub(super) catalog_client: tokio::sync::Mutex<Client>,
    prepared_statements: tokio::sync::Mutex<HashMap<&'static str, Statement>>,
    catalog_prepared_statements: tokio::sync::Mutex<HashMap<&'static str, Statement>>,
    read_statements: tokio::sync::Mutex<VecDeque<(String, Statement)>>,
}

#[derive(PartialEq, Eq)]
struct MetadataSessionIdentity {
    id: String,
    host: String,
    port: u16,
    database: String,
    user: String,
    ssl: PostgresSslMode,
    reject_unauthorized: bool,
    ca_certificate_path: Option<String>,
}

impl From<&PostgresConnectionProfile> for MetadataSessionIdentity {
    fn from(profile: &PostgresConnectionProfile) -> Self {
        Self {
            id: profile.id.clone(),
            host: profile.host.clone(),
            port: profile.port,
            database: profile.database.clone(),
            user: profile.user.clone(),
            ssl: profile.ssl,
            reject_unauthorized: profile.reject_unauthorized,
            ca_certificate_path: profile.ca_certificate_path.clone(),
        }
    }
}

impl MetadataSessionCache {
    pub(super) fn invalidate_generation(&self) {
        self.generation.fetch_add(1, Ordering::AcqRel);
    }

    pub(super) async fn get_or_connect(
        &self,
        profile: &PostgresConnectionProfile,
    ) -> Result<Arc<MetadataSession>, DatabaseError> {
        let generation = self.generation.load(Ordering::Acquire);
        let identity = MetadataSessionIdentity::from(profile);
        let mut cached = self.session.lock().await;
        if let Some(session) = cached.as_ref()
            && session.generation == generation
            && session.identity == identity
            && !session.has_closed_client()
        {
            return Ok(session.clone());
        }

        let client_config = read_only_connection_config(profile);
        let catalog_config = read_only_catalog_connection_config(profile);
        let (client, catalog_client) = match profile.ssl {
            PostgresSslMode::Disable => futures_util::try_join!(
                connect_metadata_client(client_config.connect(NoTls), profile, |error| {
                    DatabaseError::new(format_connection_error(profile, error))
                },),
                connect_metadata_client(catalog_config.connect(NoTls), profile, |error| {
                    DatabaseError::new(format_connection_error(profile, error))
                },),
            )?,
            PostgresSslMode::Require | PostgresSslMode::Prefer => {
                let client_tls = rustls_connector(profile)?;
                let catalog_tls = rustls_connector(profile)?;
                futures_util::try_join!(
                    connect_metadata_client(client_config.connect(client_tls), profile, |error| {
                        format_tls_connection_error(profile, error, profile.ssl)
                    },),
                    connect_metadata_client(
                        catalog_config.connect(catalog_tls),
                        profile,
                        |error| format_tls_connection_error(profile, error, profile.ssl),
                    ),
                )?
            }
        };
        let session = Arc::new(MetadataSession {
            generation,
            identity,
            client: tokio::sync::Mutex::new(client),
            catalog_client: tokio::sync::Mutex::new(catalog_client),
            prepared_statements: tokio::sync::Mutex::new(HashMap::new()),
            catalog_prepared_statements: tokio::sync::Mutex::new(HashMap::new()),
            read_statements: tokio::sync::Mutex::new(VecDeque::new()),
        });
        *cached = Some(session.clone());
        Ok(session)
    }

    pub(super) async fn invalidate_if_current(&self, session: &Arc<MetadataSession>) {
        let mut cached = self.session.lock().await;
        if cached
            .as_ref()
            .is_some_and(|current| Arc::ptr_eq(current, session))
        {
            *cached = None;
        }
    }
}

impl MetadataSession {
    pub(super) fn has_closed_client(&self) -> bool {
        self.client
            .try_lock()
            .is_ok_and(|client| client.is_closed())
            || self
                .catalog_client
                .try_lock()
                .is_ok_and(|client| client.is_closed())
    }

    pub(super) async fn read_statement(
        &self,
        client: &Client,
        sql: &str,
    ) -> Result<Statement, DatabaseError> {
        let mut statements = self.read_statements.lock().await;
        if let Some(index) = statements
            .iter()
            .position(|(cached_sql, _)| cached_sql == sql)
        {
            let entry = statements
                .remove(index)
                .expect("the matching prepared statement exists");
            let statement = entry.1.clone();
            statements.push_front(entry);
            return Ok(statement);
        }
        drop(statements);

        let statement = client.prepare(sql).await.map_err(|error| {
            DatabaseError::new(format!(
                "could not prepare query: {}",
                format_postgres_error(&error)
            ))
        })?;
        let mut statements = self.read_statements.lock().await;
        if let Some(index) = statements
            .iter()
            .position(|(cached_sql, _)| cached_sql == sql)
        {
            let entry = statements
                .remove(index)
                .expect("the matching prepared statement exists");
            let cached_statement = entry.1.clone();
            statements.push_front(entry);
            return Ok(cached_statement);
        }
        statements.push_front((sql.to_owned(), statement.clone()));
        if statements.len() > MAX_CACHED_READ_STATEMENTS {
            statements.pop_back();
        }
        Ok(statement)
    }
}

pub(crate) fn inspect(
    provider: &PostgresProvider,
    profile: PostgresConnectionProfile,
    request: TableListRequest,
) -> Result<PostgresInspection, DatabaseError> {
    runtime::run(move || inspect_on_runtime(&provider.metadata_sessions, &profile, request))
}

pub(crate) fn list_databases(
    provider: &PostgresProvider,
    profile: PostgresConnectionProfile,
) -> Result<Vec<String>, DatabaseError> {
    runtime::run(move || list_databases_on_runtime(&provider.metadata_sessions, &profile))
}

pub(crate) fn test_connection(
    _provider: &PostgresProvider,
    profile: PostgresConnectionProfile,
) -> Result<PostgresServerInfo, DatabaseError> {
    runtime::run(move || test_connection_on_runtime(&profile))
}

pub(crate) fn list_tables(
    provider: &PostgresProvider,
    profile: PostgresConnectionProfile,
    request: TableListRequest,
) -> TableListResult {
    let request = request.normalized()?;
    #[cfg(test)]
    let timing = metadata_timing_enabled();
    #[cfg(test)]
    let started = std::time::Instant::now();
    let result = runtime::run(move || {
        list_tables_on_runtime(&provider.metadata_sessions, &profile, request)
    });
    #[cfg(test)]
    if timing {
        eprintln!(
            "[tableX pg timing] list_tables wrapper: {:.2} ms",
            started.elapsed().as_secs_f64() * 1000.0
        );
    }
    result
}

#[cfg(test)]
fn metadata_timing_enabled() -> bool {
    std::env::var_os("TABLEX_TEST_PG_TIMING").is_some()
}

fn list_tables_on_runtime(
    sessions: &MetadataSessionCache,
    profile: &PostgresConnectionProfile,
    request: TableListRequest,
) -> TableListResult {
    let runtime = runtime::handle()?;
    runtime.block_on(async {
        for attempt in 0..=1 {
            #[cfg(test)]
            let started = std::time::Instant::now();
            let session = sessions.get_or_connect(profile).await?;
            #[cfg(test)]
            let session_elapsed = started.elapsed();
            #[cfg(test)]
            let query_started = std::time::Instant::now();
            let result = read_table_page_query(&session, None, request.clone())
                .await
                .inspect(|_| {
                    #[cfg(test)]
                    if metadata_timing_enabled() {
                        eprintln!(
                            "[tableX pg timing] session={:.2} ms, selected_client_lock_query_and_decode={:.2} ms",
                            session_elapsed.as_secs_f64() * 1000.0,
                            query_started.elapsed().as_secs_f64() * 1000.0
                        );
                    }
                });
            if session.has_closed_client() {
                sessions.invalidate_if_current(&session).await;
                if attempt == 0 {
                    continue;
                }
            }
            return result;
        }
        unreachable!("the bounded metadata reconnect loop always returns")
    })
}

fn inspect_on_runtime(
    sessions: &MetadataSessionCache,
    profile: &PostgresConnectionProfile,
    request: TableListRequest,
) -> Result<PostgresInspection, DatabaseError> {
    let runtime = runtime::handle()?;
    runtime.block_on(async {
        for attempt in 0..=1 {
            let session = sessions.get_or_connect(profile).await?;
            let result = {
                let client = session.client.lock().await;
                inspect_client(&session, &client, profile, request.clone()).await
            };
            if session.has_closed_client() {
                sessions.invalidate_if_current(&session).await;
                if attempt == 0 {
                    continue;
                }
            }
            return result;
        }
        unreachable!("the bounded metadata reconnect loop always returns")
    })
}

fn list_databases_on_runtime(
    sessions: &MetadataSessionCache,
    profile: &PostgresConnectionProfile,
) -> Result<Vec<String>, DatabaseError> {
    let runtime = runtime::handle()?;
    runtime.block_on(async {
        for attempt in 0..=1 {
            let session = sessions.get_or_connect(profile).await?;
            let result = {
                let client = session.client.lock().await;
                read_database_names(&session, &client, &profile.database).await
            };
            if session.has_closed_client() {
                sessions.invalidate_if_current(&session).await;
                if attempt == 0 {
                    continue;
                }
            }
            return Ok(result);
        }
        unreachable!("the bounded metadata reconnect loop always returns")
    })
}

async fn inspect_client(
    session: &MetadataSession,
    client: &Client,
    profile: &PostgresConnectionProfile,
    request: TableListRequest,
) -> Result<PostgresInspection, DatabaseError> {
    let server_future = read_server_info(session, client, profile);
    let database_future = read_database_names(session, client, &profile.database);
    let metadata_future = async {
        let schema_future = read_schema_names(session, client);
        let table_page_future = async {
            Ok::<_, DatabaseError>(match request.normalized() {
                Ok(request) => read_table_page_query(session, Some(client), request).await,
                Err(error) => Err(error),
            })
        };
        let (schemas, table_page) = futures_util::try_join!(schema_future, table_page_future)?;
        Ok((schemas, Some(table_page)))
    };
    // Poll independent metadata reads together so tokio-postgres can pipeline
    // them over this connection instead of paying one network round trip each.
    let ((server, databases), (schemas, table_page)) = futures_util::try_join!(
        async {
            let (server, databases) = futures_util::join!(server_future, database_future);
            Ok::<_, DatabaseError>((server?, databases))
        },
        metadata_future
    )?;
    Ok(PostgresInspection {
        server,
        databases,
        schemas,
        table_page,
    })
}

async fn connect_metadata_client<T, Fut, F>(
    connect: Fut,
    profile: &PostgresConnectionProfile,
    format_error: F,
) -> Result<Client, DatabaseError>
where
    Fut: Future<Output = Result<(Client, T), tokio_postgres::Error>>,
    T: Future<Output = Result<(), tokio_postgres::Error>> + Send + 'static,
    F: FnOnce(tokio_postgres::Error) -> DatabaseError,
{
    let (client, connection) = connect_with_timeout(connect, profile, format_error).await?;
    tokio::spawn(async move {
        if let Err(error) = connection.await {
            eprintln!("PostgreSQL metadata connection closed: {error}");
        }
    });
    Ok(client)
}

fn test_connection_on_runtime(
    profile: &PostgresConnectionProfile,
) -> Result<PostgresServerInfo, DatabaseError> {
    let runtime = runtime::handle()?;
    runtime.block_on(async {
        let config = read_only_connection_config(profile);
        let client = match profile.ssl {
            PostgresSslMode::Disable => {
                connect_metadata_client(config.connect(NoTls), profile, |error| {
                    DatabaseError::new(format_connection_error(profile, error))
                })
                .await?
            }
            PostgresSslMode::Require | PostgresSslMode::Prefer => {
                let tls = rustls_connector(profile)?;
                connect_metadata_client(config.connect(tls), profile, |error| {
                    format_tls_connection_error(profile, error, profile.ssl)
                })
                .await?
            }
        };
        read_server_info_uncached(&client, profile).await
    })
}

async fn prepared_statement(
    session: &MetadataSession,
    client: &Client,
    query: &'static str,
) -> Result<Statement, DatabaseError> {
    prepared_statement_from(&session.prepared_statements, client, query).await
}

async fn prepared_catalog_statement(
    session: &MetadataSession,
    client: &Client,
    query: &'static str,
) -> Result<Statement, DatabaseError> {
    prepared_statement_from(&session.catalog_prepared_statements, client, query).await
}

async fn prepared_statement_from(
    cache: &tokio::sync::Mutex<HashMap<&'static str, Statement>>,
    client: &Client,
    query: &'static str,
) -> Result<Statement, DatabaseError> {
    if let Some(statement) = cache.lock().await.get(query).cloned() {
        return Ok(statement);
    }
    let statement = client.prepare(query).await.map_err(|error| {
        DatabaseError::new(format!(
            "failed to prepare PostgreSQL metadata query: {}",
            format_postgres_error(&error)
        ))
    })?;
    let mut statements = cache.lock().await;
    Ok(statements.entry(query).or_insert(statement).clone())
}

async fn read_schema_names(
    session: &MetadataSession,
    client: &Client,
) -> Result<Vec<String>, DatabaseError> {
    let statement = prepared_statement(session, client, SCHEMA_NAMES_QUERY).await?;
    let schema_rows = client
        .query(&statement, &[&((MAX_METADATA_SCHEMAS + 1) as i32)])
        .await
        .map_err(|error| {
            DatabaseError::new(format!(
                "failed to read PostgreSQL schemas: {}",
                format_postgres_error(&error)
            ))
        })?;
    if schema_rows.len() > MAX_METADATA_SCHEMAS {
        return Err(DatabaseError::new(format!(
            "PostgreSQL metadata contains more than {MAX_METADATA_SCHEMAS} schemas"
        )));
    }

    schema_rows
        .into_iter()
        .map(|row| {
            row.try_get(0)
                .map_err(|error| DatabaseError::new(format!("invalid schema name: {error}")))
        })
        .collect()
}

async fn read_database_names(
    session: &MetadataSession,
    client: &Client,
    fallback_database: &str,
) -> Vec<String> {
    let Ok(statement) = prepared_statement(session, client, DATABASE_NAMES_QUERY).await else {
        return vec![fallback_database.to_owned()];
    };
    let Ok(rows) = client
        .query(&statement, &[&((MAX_METADATA_DATABASES + 1) as i32)])
        .await
    else {
        return vec![fallback_database.to_owned()];
    };

    let mut databases = Vec::new();
    for row in rows.into_iter().take(MAX_METADATA_DATABASES) {
        if let Ok(name) = row.try_get::<_, String>(0) {
            databases.push(name);
        }
    }
    if databases.is_empty() {
        vec![fallback_database.to_owned()]
    } else {
        databases
    }
}

pub(super) async fn read_table_page_query(
    session: &MetadataSession,
    main_client: Option<&Client>,
    request: TableListRequest,
) -> TableListResult {
    let uses_catalog_client =
        request.schema.is_none() && (request.after.is_some() || request.offset == 0);
    if uses_catalog_client {
        let catalog_client = session.catalog_client.lock().await;
        read_table_page_query_on_client(session, &catalog_client, request).await
    } else if let Some(main_client) = main_client {
        read_table_page_query_on_client(session, main_client, request).await
    } else {
        let main_client = session.client.lock().await;
        read_table_page_query_on_client(session, &main_client, request).await
    }
}

async fn read_table_page_query_on_client(
    session: &MetadataSession,
    client: &Client,
    request: TableListRequest,
) -> TableListResult {
    let type_filter = request.relation_type.map(|kind| kind.relkind());
    let fetch_limit = (request.limit + 1) as i64;
    #[cfg(test)]
    let query_started = std::time::Instant::now();
    let rows = if let (Some(schema), Some(after)) = (&request.schema, &request.after) {
        let statement = prepared_statement(session, client, TABLE_PAGE_SCHEMA_CURSOR_QUERY).await?;
        client
            .query(
                &statement,
                &[
                    &request.search,
                    schema,
                    &type_filter,
                    &fetch_limit,
                    &after.table,
                ],
            )
            .await
    } else if let (None, Some(after)) = (&request.schema, &request.after) {
        let statement =
            prepared_catalog_statement(session, client, TABLE_PAGE_GLOBAL_CURSOR_QUERY).await?;
        client
            .query(
                &statement,
                &[
                    &request.search,
                    &type_filter,
                    &fetch_limit,
                    &after.schema,
                    &after.table,
                ],
            )
            .await
    } else if request.schema.is_none() && request.offset == 0 {
        let statement = prepared_catalog_statement(session, client, TABLE_PAGE_FIRST_QUERY).await?;
        client
            .query(&statement, &[&request.search, &type_filter, &fetch_limit])
            .await
    } else {
        let statement = prepared_statement(session, client, TABLE_PAGE_QUERY).await?;
        client
            .query(
                &statement,
                &[
                    &request.search,
                    &request.schema,
                    &type_filter,
                    &fetch_limit,
                    &(request.offset as i64),
                ],
            )
            .await
    }
    .map_err(|error| {
        DatabaseError::new(format!(
            "failed to list PostgreSQL tables: {}",
            format_postgres_error(&error)
        ))
    })?;
    #[cfg(test)]
    let database_elapsed = query_started.elapsed();
    #[cfg(test)]
    let decode_started = std::time::Instant::now();

    let has_next = rows.len() > request.limit;
    let tables = rows
        .into_iter()
        .take(request.limit)
        .map(|row| {
            let relation_type: String = row.try_get(3).map_err(|error| {
                DatabaseError::new(format!("invalid PostgreSQL relation type: {error}"))
            })?;
            let relation_type =
                TableRelationType::from_relkind(&relation_type).ok_or_else(|| {
                    DatabaseError::new("PostgreSQL returned an unsupported relation type")
                })?;
            Ok(TableSummary {
                id: row.try_get(0).map_err(|error| {
                    DatabaseError::new(format!("invalid PostgreSQL table id: {error}"))
                })?,
                schema: row.try_get(1).map_err(|error| {
                    DatabaseError::new(format!("invalid PostgreSQL schema name: {error}"))
                })?,
                name: row.try_get(2).map_err(|error| {
                    DatabaseError::new(format!("invalid PostgreSQL table name: {error}"))
                })?,
                relation_type,
            })
        })
        .collect::<Result<Vec<_>, DatabaseError>>()?;
    let next_cursor = has_next
        .then(|| {
            tables.last().map(|table| TableCursor {
                schema: table.schema.clone(),
                table: table.name.clone(),
            })
        })
        .flatten();

    let result = TablePage {
        tables,
        limit: request.limit,
        offset: request.offset,
        has_next,
        next_cursor,
    };
    #[cfg(test)]
    if metadata_timing_enabled() {
        eprintln!(
            "[tableX pg timing] prepared_query_round_trip={:.2} ms, row_conversion={:.2} ms, returned_rows={}",
            database_elapsed.as_secs_f64() * 1000.0,
            decode_started.elapsed().as_secs_f64() * 1000.0,
            result.tables.len()
        );
    }
    Ok(result)
}

async fn read_server_info(
    session: &MetadataSession,
    client: &Client,
    profile: &PostgresConnectionProfile,
) -> Result<PostgresServerInfo, DatabaseError> {
    let statement = prepared_statement(session, client, SERVER_INFO_QUERY).await?;
    let row = client.query_one(&statement, &[]).await.map_err(|error| {
        DatabaseError::new(format!(
            "failed to read PostgreSQL server info: {}",
            format_postgres_error(&error)
        ))
    })?;
    server_info_from_row(row, profile)
}

async fn read_server_info_uncached(
    client: &Client,
    profile: &PostgresConnectionProfile,
) -> Result<PostgresServerInfo, DatabaseError> {
    let row = client
        .query_one(SERVER_INFO_QUERY, &[])
        .await
        .map_err(|error| {
            DatabaseError::new(format!(
                "failed to read PostgreSQL server info: {}",
                format_postgres_error(&error)
            ))
        })?;
    server_info_from_row(row, profile)
}

fn server_info_from_row(
    row: tokio_postgres::Row,
    profile: &PostgresConnectionProfile,
) -> Result<PostgresServerInfo, DatabaseError> {
    let version_num: i32 = row
        .try_get::<_, String>(3)
        .map_err(|error| {
            DatabaseError::new(format!("invalid PostgreSQL server_version_num: {error}"))
        })?
        .parse()
        .map_err(|error| {
            DatabaseError::new(format!("invalid PostgreSQL server_version_num: {error}"))
        })?;
    let version = PostgresVersion::from_server_version_num(version_num).ok_or_else(|| {
        DatabaseError::new(format!(
            "unsupported PostgreSQL server_version_num: {version_num}"
        ))
    })?;

    Ok(PostgresServerInfo {
        server_version: row.try_get(2).map_err(|error| {
            DatabaseError::new(format!("invalid PostgreSQL server_version: {error}"))
        })?,
        version,
        database: row
            .try_get(0)
            .map_err(|error| DatabaseError::new(format!("invalid current_database(): {error}")))?,
        user: row
            .try_get(1)
            .map_err(|error| DatabaseError::new(format!("invalid current_user: {error}")))?,
        host: profile.host.clone(),
        port: profile.port,
    })
}

#[cfg(test)]
#[path = "../../../tests/unit/infrastructure/postgres/metadata.rs"]
mod tests;
