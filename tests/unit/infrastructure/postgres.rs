use super::PostgresProvider;
use super::TableListRequest;
use super::connect::connection_config;
use super::error::normalize_error_terms;
use super::model::{PostgresConnectionProfile, PostgresSslMode};
use super::query::{compare_decoder_timings, validate_mutation, validate_read};
use super::runtime;
use crate::domain::database_object::TableCursor;
use crate::domain::query::{
    CellUpdateRequest, TableColumnFilter, TableDataCursor, TableDataCursorDirection,
    TableFilterOperator, TablePreviewPageRequest,
};
use std::time::Instant;
use tokio_postgres::config::SslMode;

#[test]
fn rejects_mutating_or_multiple_statements() {
    assert!(validate_read("UPDATE users SET name = 'x'").is_err());
    assert!(validate_read("SELECT 1; SELECT 2").is_err());
    assert!(validate_read("SELECT 1;").is_ok());
    assert!(PostgresProvider::is_mutating_query(
        "UPDATE users SET name = 'x'"
    ));
    assert!(validate_mutation("UPDATE users SET name = 'x'").is_ok());
    assert!(validate_mutation("SELECT 1").is_err());
}

#[test]
fn makes_tls_unknown_issuer_readable() {
    assert_eq!(
        normalize_error_terms("invalid peer certificate: UnknownIssuer".into()),
        "invalid peer certificate: unknown issuer"
    );
}

#[test]
fn maps_selected_ssl_mode_to_postgres_config() {
    let mut profile =
        PostgresConnectionProfile::new("ssl-test", "SSL test", "localhost", "postgres", "postgres");

    profile.ssl = PostgresSslMode::Disable;
    assert_eq!(connection_config(&profile).get_ssl_mode(), SslMode::Disable);

    profile.ssl = PostgresSslMode::Prefer;
    assert_eq!(connection_config(&profile).get_ssl_mode(), SslMode::Prefer);

    profile.ssl = PostgresSslMode::Require;
    assert_eq!(connection_config(&profile).get_ssl_mode(), SslMode::Require);
}

#[test]
#[ignore = "requires an explicitly configured PostgreSQL server"]
fn inspects_configured_postgres_server() {
    let host = std::env::var("TABLEX_TEST_PG_HOST").expect("TABLEX_TEST_PG_HOST is required");
    let port = std::env::var("TABLEX_TEST_PG_PORT")
        .expect("TABLEX_TEST_PG_PORT is required")
        .parse()
        .expect("TABLEX_TEST_PG_PORT must be a number");
    let database =
        std::env::var("TABLEX_TEST_PG_DATABASE").expect("TABLEX_TEST_PG_DATABASE is required");
    let user = std::env::var("TABLEX_TEST_PG_USER").expect("TABLEX_TEST_PG_USER is required");

    let mut profile = PostgresConnectionProfile::new(
        "integration-test",
        "Integration test",
        host,
        database,
        user,
    );
    profile.port = port;
    profile.password = std::env::var("TABLEX_TEST_PG_PASSWORD").ok();
    profile.ssl = match std::env::var("TABLEX_TEST_PG_SSL_MODE").as_deref() {
        Ok("disable") => PostgresSslMode::Disable,
        Ok("require") => PostgresSslMode::Require,
        Ok("prefer") | Err(_) => PostgresSslMode::Prefer,
        Ok(_) => panic!("TABLEX_TEST_PG_SSL_MODE must be disable, prefer, or require"),
    };

    if std::env::var_os("TABLEX_TEST_PG_BENCHMARK_DECODER").is_some() {
        let mut benchmark_profile = profile.clone();
        benchmark_profile.ssl = PostgresSslMode::Disable;
        let columns = (0..40)
            .map(|index| format!("value::integer AS column_{index}"))
            .collect::<Vec<_>>()
            .join(", ");
        let (optimized, probe_chain) = compare_decoder_timings(
            benchmark_profile,
            format!("SELECT {columns} FROM generate_series(1, 500) AS rows(value)"),
        )
        .expect("decoder benchmark query should succeed");
        eprintln!(
            "PostgreSQL integer decoder, 20,000 cells: type dispatch median={:.2} ms, previous probe chain median={:.2} ms, speedup={:.2}x",
            optimized.as_secs_f64() * 1000.0,
            probe_chain.as_secs_f64() * 1000.0,
            probe_chain.as_secs_f64() / optimized.as_secs_f64()
        );
    }

    let provider = PostgresProvider::new();
    if let Ok(table) = std::env::var("TABLEX_TEST_PG_PREVIEW_TABLE") {
        let schema =
            std::env::var("TABLEX_TEST_PG_PREVIEW_SCHEMA").unwrap_or_else(|_| "public".into());
        let primary_key = std::env::var("TABLEX_TEST_PG_PREVIEW_PRIMARY_KEY")
            .expect("TABLEX_TEST_PG_PREVIEW_PRIMARY_KEY is required with a preview table");
        let offset = std::env::var("TABLEX_TEST_PG_PREVIEW_OFFSET")
            .map(|offset| {
                offset
                    .parse()
                    .expect("TABLEX_TEST_PG_PREVIEW_OFFSET must be a number")
            })
            .unwrap_or(150_000);
        compare_primary_key_preview_timings(
            &provider,
            &profile,
            &schema,
            &table,
            &primary_key,
            offset,
        );
        assert_cursor_navigation_pages(&provider, &profile, &schema, &table, &primary_key, offset);
        if let Ok(sort_column) = std::env::var("TABLEX_TEST_PG_PREVIEW_STALE_SORT") {
            let sort = (sort_column.clone(), false);
            let fallback = provider
                .preview_table_page(
                    profile.clone(),
                    schema.clone(),
                    table.clone(),
                    preview_request(100, offset, Some(sort.clone()), Vec::new(), None),
                )
                .expect("ordinary sorted preview should succeed");
            let stale_hint = provider
                .preview_table_page(
                    profile.clone(),
                    schema.clone(),
                    table.clone(),
                    preview_request(
                        100,
                        offset,
                        Some(sort.clone()),
                        vec![sort_column],
                        Some(TableDataCursor {
                            values: vec!["unused stale cursor".into()],
                            direction: TableDataCursorDirection::After,
                        }),
                    ),
                )
                .expect("stale primary-key hint should fall back safely");
            assert_eq!(stale_hint.rows, fallback.rows);
            assert_eq!(stale_hint.has_next, fallback.has_next);
        }
    }
    if let Ok(table) = std::env::var("TABLEX_TEST_PG_COMPOSITE_PREVIEW_TABLE") {
        let schema = std::env::var("TABLEX_TEST_PG_COMPOSITE_PREVIEW_SCHEMA")
            .unwrap_or_else(|_| "public".into());
        let key_columns = std::env::var("TABLEX_TEST_PG_COMPOSITE_PREVIEW_KEYS")
            .expect("TABLEX_TEST_PG_COMPOSITE_PREVIEW_KEYS is required with a preview table")
            .split(',')
            .map(str::trim)
            .filter(|column| !column.is_empty())
            .map(str::to_owned)
            .collect::<Vec<_>>();
        let offset = std::env::var("TABLEX_TEST_PG_COMPOSITE_PREVIEW_OFFSET")
            .map(|offset| {
                offset
                    .parse()
                    .expect("TABLEX_TEST_PG_COMPOSITE_PREVIEW_OFFSET must be a number")
            })
            .unwrap_or(50_000);
        assert_composite_cursor_navigation_pages(
            &provider,
            &profile,
            &schema,
            &table,
            &key_columns,
            offset,
        );
    }
    assert_read_statement_cache_reuses_prepared_query(&provider, &profile);
    assert_session_health_checks_skip_busy_clients(&provider, &profile);
    let inspection = provider
        .inspect(profile.clone(), TableListRequest::default())
        .expect("PostgreSQL inspection should succeed");
    let initial_metadata_backend_pid = metadata_backend_pid(&provider, profile.clone());
    let initial_catalog_backend_pid = catalog_backend_pid(&provider, profile.clone());

    assert!(!inspection.server.server_version.is_empty());
    assert_eq!(
        inspection.server.database,
        std::env::var("TABLEX_TEST_PG_DATABASE").unwrap()
    );
    assert!(inspection.schemas.iter().all(|schema| !schema.is_empty()));
    assert!(
        inspection
            .table_page
            .as_ref()
            .is_some_and(|page| page.as_ref().is_ok_and(|page| page.tables.len() <= 100))
    );
    let invalid_table_search = provider
        .inspect(
            profile.clone(),
            TableListRequest {
                search: "x".repeat(257),
                ..TableListRequest::default()
            },
        )
        .expect("table-list validation errors should not hide successful connectivity");
    assert!(
        invalid_table_search
            .table_page
            .is_some_and(|page| page.is_err())
    );

    let result = provider
        .execute_read_query(profile.clone(), "SELECT 1 AS answer, 'ok' AS status".into())
        .expect("read-only PostgreSQL query should succeed");
    assert_eq!(result.columns, ["answer", "status"]);
    assert_eq!(result.rows, [["1", "ok"]]);
    let typed_values = provider
        .execute_read_query(
            profile.clone(),
            "SELECT TRUE AS boolean_value, 7::smallint AS smallint_value, 42::integer AS integer_value, 99::bigint AS bigint_value, 1.25::real AS real_value, 2.5::double precision AS double_value, DATE '2024-02-03' AS date_value, TIME '04:05:06' AS time_value, TIMESTAMP '2024-02-03 04:05:06' AS timestamp_value, TIMESTAMPTZ '2024-02-03 04:05:06+00' AS timestamp_tz_value, decode('00ff', 'hex') AS bytes_value, NULL::text AS null_value".into(),
        )
        .expect("supported PostgreSQL types should decode for display");
    assert_eq!(
        typed_values.rows,
        [[
            "true",
            "7",
            "42",
            "99",
            "1.25",
            "2.5",
            "2024-02-03",
            "04:05:06",
            "2024-02-03 04:05:06",
            "2024-02-03 04:05:06+00:00",
            "\\x00ff",
            "NULL",
        ]]
    );
    let session_settings = provider
        .execute_read_query(
            profile.clone(),
            "SELECT current_setting('default_transaction_read_only'), current_setting('statement_timeout')".into(),
        )
        .expect("read-only session settings should be applied at connection startup");
    assert_eq!(session_settings.rows, [["on", "30s"]]);
    let capped_result = provider
        .execute_read_query(
            profile.clone(),
            "SELECT value FROM generate_series(1, 501) AS value".into(),
        )
        .expect("bounded portal query should complete");
    assert_eq!(capped_result.rows.len(), 500);
    assert!(capped_result.truncated);

    let oversized_result = provider
        .execute_read_query(
            profile.clone(),
            "SELECT repeat('x', 1048577) AS oversized_value".into(),
        )
        .expect_err("oversized result cells should be rejected before UI rendering");
    assert!(oversized_result.message.contains("display limits"));
    let oversized_page = provider
        .execute_read_query(
            profile.clone(),
            "SELECT repeat('x', 1048576) FROM generate_series(1, 33)".into(),
        )
        .expect_err("oversized query pages should stop at the total display budget");
    assert!(oversized_page.message.contains("display limits"));

    let table_page = provider
        .list_tables(
            profile.clone(),
            TableListRequest {
                limit: 100,
                ..TableListRequest::default()
            },
        )
        .expect("table list should load from PostgreSQL");
    assert!(table_page.tables.len() <= 100);
    assert_eq!(
        metadata_backend_pid(&provider, profile.clone()),
        initial_metadata_backend_pid,
        "inspection and user queries should reuse the standard read-only PostgreSQL session"
    );
    assert_eq!(
        catalog_backend_pid(&provider, profile.clone()),
        initial_catalog_backend_pid,
        "global table browsing should reuse its read-only catalog session"
    );

    if std::env::var_os("TABLEX_TEST_PG_BENCHMARK_METADATA").is_some() {
        compare_metadata_session_timings(&provider, &profile);
        compare_inspection_timings(&provider, &profile);
    }
    if std::env::var_os("TABLEX_TEST_PG_BENCHMARK_QUERY_CACHE").is_some() {
        compare_read_statement_timings(&provider, &profile);
    }
    provider.invalidate_metadata_session();
    assert_ne!(
        metadata_backend_pid(&provider, profile.clone()),
        initial_metadata_backend_pid,
        "explicit reconnect should replace the cached PostgreSQL backend session"
    );
    assert_ne!(
        catalog_backend_pid(&provider, profile.clone()),
        initial_catalog_backend_pid,
        "explicit reconnect should replace the cached catalog backend session"
    );

    if let Ok(schema) = std::env::var("TABLEX_TEST_PG_CURSOR_SCHEMA") {
        let after = std::env::var("TABLEX_TEST_PG_CURSOR_AFTER")
            .expect("TABLEX_TEST_PG_CURSOR_AFTER is required with a cursor schema");
        let first_page = provider
            .list_tables(
                profile.clone(),
                TableListRequest {
                    schema: Some(schema.clone()),
                    limit: 100,
                    ..TableListRequest::default()
                },
            )
            .expect("first schema-filtered page should load");
        let first_page_cursor = first_page
            .next_cursor
            .clone()
            .expect("a non-final schema page should return a next cursor");
        let cursor_next_page = provider
            .list_tables(
                profile.clone(),
                TableListRequest {
                    schema: Some(schema.clone()),
                    limit: 100,
                    offset: 100,
                    after: Some(first_page_cursor),
                    ..TableListRequest::default()
                },
            )
            .expect("next schema-filtered page should load from its cursor");
        let offset_next_page = provider
            .list_tables(
                profile.clone(),
                TableListRequest {
                    schema: Some(schema.clone()),
                    limit: 100,
                    offset: 100,
                    ..TableListRequest::default()
                },
            )
            .expect("offset-based second schema page should load");
        assert_eq!(
            cursor_next_page.tables, offset_next_page.tables,
            "a next cursor must preserve existing page ordering"
        );
        let cursor_request = TableListRequest {
            schema: Some(schema.clone()),
            limit: 100,
            offset: 49_900,
            after: Some(TableCursor {
                schema: schema.clone(),
                table: after.clone(),
            }),
            ..TableListRequest::default()
        };
        let offset_request = TableListRequest {
            schema: Some(schema),
            limit: 100,
            offset: 49_900,
            ..TableListRequest::default()
        };
        let mut cursor_times = Vec::with_capacity(5);
        let mut offset_times = Vec::with_capacity(5);
        let mut cursor_page = None;
        let mut offset_page = None;
        for _ in 0..5 {
            let start = std::time::Instant::now();
            let page = provider
                .list_tables(profile.clone(), cursor_request.clone())
                .expect("schema cursor table page should load");
            cursor_times.push(start.elapsed());
            cursor_page = Some(page);

            let start = std::time::Instant::now();
            let page = provider
                .list_tables(profile.clone(), offset_request.clone())
                .expect("schema offset table page should load");
            offset_times.push(start.elapsed());
            offset_page = Some(page);
        }
        cursor_times.sort_unstable();
        offset_times.sort_unstable();
        let cursor_page = cursor_page.expect("cursor benchmark ran at least once");
        let offset_page = offset_page.expect("offset benchmark ran at least once");
        eprintln!(
            "PostgreSQL schema page after {after}: cursor median={:.2} ms, offset median={:.2} ms",
            cursor_times[2].as_secs_f64() * 1000.0,
            offset_times[2].as_secs_f64() * 1000.0,
        );
        assert!(
            cursor_page.tables.iter().all(|table| table.name > after),
            "a cursor page must contain only relation names after its cursor"
        );
        assert_eq!(
            cursor_page.tables, offset_page.tables,
            "schema cursor and offset pagination must return the same page"
        );
    }

    if let Ok(schema) = std::env::var("TABLEX_TEST_PG_GLOBAL_CURSOR_SCHEMA") {
        let table = std::env::var("TABLEX_TEST_PG_GLOBAL_CURSOR_TABLE")
            .expect("TABLEX_TEST_PG_GLOBAL_CURSOR_TABLE is required with a cursor schema");
        let page_offset = std::env::var("TABLEX_TEST_PG_GLOBAL_CURSOR_OFFSET")
            .map(|offset| {
                offset
                    .parse::<usize>()
                    .expect("TABLEX_TEST_PG_GLOBAL_CURSOR_OFFSET must be a number")
            })
            .unwrap_or(49_900);
        let start = std::time::Instant::now();
        let first_page = provider
            .list_tables(
                profile.clone(),
                TableListRequest {
                    limit: 100,
                    ..TableListRequest::default()
                },
            )
            .expect("first all-schema page should load");
        eprintln!(
            "PostgreSQL first all-schema page: {} rows in {:.2} ms",
            first_page.tables.len(),
            start.elapsed().as_secs_f64() * 1000.0,
        );
        let first_page_cursor = first_page
            .next_cursor
            .clone()
            .expect("a non-final all-schema page should return a cursor");
        let cursor_next_page = provider
            .list_tables(
                profile.clone(),
                TableListRequest {
                    limit: 100,
                    offset: 100,
                    after: Some(first_page_cursor),
                    ..TableListRequest::default()
                },
            )
            .expect("next all-schema page should load from its cursor");
        let offset_next_page = provider
            .list_tables(
                profile.clone(),
                TableListRequest {
                    limit: 100,
                    offset: 100,
                    ..TableListRequest::default()
                },
            )
            .expect("offset-based all-schema second page should load");
        assert_eq!(
            cursor_next_page.tables, offset_next_page.tables,
            "global cursor pages must preserve schema/name ordering"
        );

        let cursor_request = TableListRequest {
            limit: 100,
            offset: page_offset,
            after: Some(TableCursor {
                schema,
                table: table.clone(),
            }),
            ..TableListRequest::default()
        };
        let offset_request = TableListRequest {
            limit: 100,
            offset: page_offset,
            ..TableListRequest::default()
        };
        const ITERATIONS: usize = 9;
        let mut cursor_times = Vec::with_capacity(ITERATIONS);
        let mut offset_times = Vec::with_capacity(ITERATIONS);
        let mut cursor_page = None;
        let mut offset_page = None;
        for iteration in 0..ITERATIONS {
            let measure_cursor = || {
                let start = std::time::Instant::now();
                let page = provider
                    .list_tables(profile.clone(), cursor_request.clone())
                    .expect("global cursor table page should load");
                (start.elapsed(), page)
            };
            let measure_offset = || {
                let start = std::time::Instant::now();
                let page = provider
                    .list_tables(profile.clone(), offset_request.clone())
                    .expect("global offset table page should load");
                (start.elapsed(), page)
            };
            if iteration.is_multiple_of(2) {
                let (elapsed, page) = measure_cursor();
                cursor_times.push(elapsed);
                cursor_page = Some(page);
                let (elapsed, page) = measure_offset();
                offset_times.push(elapsed);
                offset_page = Some(page);
            } else {
                let (elapsed, page) = measure_offset();
                offset_times.push(elapsed);
                offset_page = Some(page);
                let (elapsed, page) = measure_cursor();
                cursor_times.push(elapsed);
                cursor_page = Some(page);
            }
        }
        cursor_times.sort_unstable();
        offset_times.sort_unstable();
        let cursor_page = cursor_page.expect("cursor benchmark ran at least once");
        let offset_page = offset_page.expect("offset benchmark ran at least once");
        eprintln!(
            "PostgreSQL all-schema page after {table}: cursor median={:.2} ms, offset median={:.2} ms",
            cursor_times[ITERATIONS / 2].as_secs_f64() * 1000.0,
            offset_times[ITERATIONS / 2].as_secs_f64() * 1000.0,
        );
        assert_eq!(
            cursor_page.tables, offset_page.tables,
            "global cursor and offset must return the same page"
        );
    }

    if let Ok(table_name) = std::env::var("TABLEX_TEST_PG_MUTATION_TABLE")
        && !table_name.is_empty()
    {
        let table = quote_test_identifier(&table_name);
        let preview = provider
            .preview_table(profile.clone(), "public".into(), table_name.clone())
            .expect("PostgreSQL table preview should succeed");
        assert_eq!(
            preview
                .editable
                .as_ref()
                .map(|table| table.primary_key_columns.clone()),
            Some(vec!["id".into()])
        );
        let preview_statement = format!(
            "SELECT * FROM {}.{} LIMIT $1::bigint OFFSET $2::bigint",
            quote_test_identifier("public"),
            quote_test_identifier(&table_name)
        );
        let prepared_count_sql = format!(
            "SELECT count(*)::text FROM pg_prepared_statements WHERE statement = '{}'",
            preview_statement.replace('\'', "''")
        );
        let prepared_count = || {
            provider
                .execute_read_query(profile.clone(), prepared_count_sql.clone())
                .expect("prepared preview statement count should be readable")
                .rows[0][0]
                .parse::<usize>()
                .expect("prepared preview statement count should be an integer")
        };
        assert_eq!(prepared_count(), 1);
        let next_preview_page = provider
            .preview_table_page(
                profile.clone(),
                "public".into(),
                table_name.clone(),
                preview_request(25, 25, None, Vec::new(), None),
            )
            .expect("parameterized next preview page should succeed");
        assert_eq!(next_preview_page.offset, 25);
        assert_eq!(
            prepared_count(),
            1,
            "page changes should reuse the preview plan"
        );

        let mutation = provider
            .execute_mutation(
                profile.clone(),
                format!("INSERT INTO {table} (label) VALUES ('tableX smoke')"),
            )
            .expect("PostgreSQL mutation should commit");
        assert_eq!(mutation.affected_rows, 1);

        let mut filtered_request = preview_request(25, 0, None, Vec::new(), None);
        filtered_request.filters = vec![TableColumnFilter {
            column: "label".into(),
            operator: TableFilterOperator::Contains,
            value: Some("tableX smoke".into()),
        }];
        let filtered_preview = provider
            .preview_table_page(
                profile.clone(),
                "public".into(),
                table_name.clone(),
                filtered_request,
            )
            .expect("server-side filtered preview should succeed");
        assert_eq!(filtered_preview.rows.len(), 1);
        let editable = filtered_preview.editable.as_ref().unwrap();
        let row = &filtered_preview.rows[0];
        let primary_key_values = editable
            .primary_key_columns
            .iter()
            .map(|key| {
                let index = filtered_preview
                    .columns
                    .iter()
                    .position(|column| column == key)
                    .unwrap();
                row[index].clone()
            })
            .collect();
        let update = CellUpdateRequest {
            schema: "public".into(),
            table: table_name.clone(),
            column: "label".into(),
            primary_key_columns: editable.primary_key_columns.clone(),
            primary_key_values,
            value: Some("tableX edited".into()),
            expected_value: Some("tableX smoke".into()),
        };
        let updated = provider
            .update_table_cell(profile.clone(), update.clone())
            .expect("inline cell update should commit");
        assert_eq!(updated.affected_rows, 1);
        let stale_update = CellUpdateRequest {
            value: Some("stale overwrite".into()),
            ..update.clone()
        };
        assert!(
            provider
                .update_table_cell(profile.clone(), stale_update)
                .is_err()
        );
        let nullable_update = CellUpdateRequest {
            value: None,
            expected_value: Some("tableX edited".into()),
            ..update.clone()
        };
        provider
            .update_table_cell(profile.clone(), nullable_update)
            .expect("inline editor should persist SQL NULL");
        let null_preview = provider
            .preview_table_page(
                profile.clone(),
                "public".into(),
                table_name.clone(),
                TablePreviewPageRequest {
                    limit: 25,
                    offset: 0,
                    sort: None,
                    primary_key_columns: Vec::new(),
                    cursor: None,
                    filters: vec![TableColumnFilter {
                        column: "label".into(),
                        operator: TableFilterOperator::IsNull,
                        value: None,
                    }],
                },
            )
            .expect("IS NULL preview filter should succeed");
        assert_eq!(null_preview.rows.len(), 1);
        assert!(null_preview.null_cells[0][1]);
        provider
            .update_table_cell(
                profile.clone(),
                CellUpdateRequest {
                    value: Some("tableX edited".into()),
                    expected_value: None,
                    ..update
                },
            )
            .expect("inline editor should replace SQL NULL with a value");

        let result = provider
            .execute_read_query(profile, format!("SELECT label FROM {table}"))
            .expect("committed mutation should be readable");
        assert!(result.rows.iter().any(|row| row == &["tableX edited"]));
    }
}

fn quote_test_identifier(identifier: &str) -> String {
    format!("\"{}\"", identifier.replace('"', "\"\""))
}

fn assert_read_statement_cache_reuses_prepared_query(
    provider: &PostgresProvider,
    profile: &PostgresConnectionProfile,
) {
    const CACHED_SQL: &str = "SELECT 42 AS tablex_cached_read_probe";
    const COUNT_SQL: &str = "SELECT count(*)::text FROM pg_prepared_statements WHERE statement = 'SELECT 42 AS tablex_cached_read_probe'";

    let read_probe = || {
        provider
            .execute_read_query(profile.clone(), CACHED_SQL.into())
            .expect("cached read statement should execute")
    };
    let prepared_count = || {
        provider
            .execute_read_query(profile.clone(), COUNT_SQL.into())
            .expect("prepared statement count should be readable")
            .rows[0][0]
            .parse::<usize>()
            .expect("prepared statement count should be an integer")
    };

    assert_eq!(read_probe().rows, [["42"]]);
    assert_eq!(prepared_count(), 1);
    assert_eq!(read_probe().rows, [["42"]]);
    assert_eq!(
        prepared_count(),
        1,
        "identical SQL should reuse its statement"
    );
}

fn assert_session_health_checks_skip_busy_clients(
    provider: &PostgresProvider,
    profile: &PostgresConnectionProfile,
) {
    let runtime = runtime::handle().expect("PostgreSQL runtime should be available");
    runtime.block_on(async {
        let session = provider
            .metadata_sessions
            .get_or_connect(profile)
            .await
            .expect("metadata session should be available");
        let _client_guard = session.client.lock().await;
        let started = Instant::now();
        let cached_session = tokio::time::timeout(
            std::time::Duration::from_secs(1),
            provider.metadata_sessions.get_or_connect(profile),
        )
        .await
        .expect("session lookup should not wait for a busy PostgreSQL client")
        .expect("cached metadata session should remain available");
        assert!(std::sync::Arc::ptr_eq(&session, &cached_session));
        assert!(
            !session.has_closed_client(),
            "an open but busy client should not be mistaken for a closed client"
        );
        let page = tokio::time::timeout(
            std::time::Duration::from_secs(1),
            super::metadata::read_table_page_query(
                &session,
                None,
                TableListRequest {
                    limit: 1,
                    ..TableListRequest::default()
                },
            ),
        )
        .await
        .expect("catalog page query should not wait for the busy main client")
        .expect("catalog page query should succeed on its dedicated connection");
        assert!(page.tables.len() <= 1);
        eprintln!(
            "PostgreSQL catalog page while main client mutex is held: {:.2} ms",
            started.elapsed().as_secs_f64() * 1000.0
        );
    });
}

fn metadata_backend_pid(provider: &PostgresProvider, profile: PostgresConnectionProfile) -> i32 {
    let runtime = runtime::handle().expect("PostgreSQL runtime should exist");
    runtime.block_on(async {
        let session = provider
            .metadata_sessions
            .get_or_connect(&profile)
            .await
            .expect("read-only metadata session should be available");
        let client = session.client.lock().await;
        client
            .query_one("SELECT pg_backend_pid()", &[])
            .await
            .expect("backend pid should be readable")
            .get(0)
    })
}

fn catalog_backend_pid(provider: &PostgresProvider, profile: PostgresConnectionProfile) -> i32 {
    let runtime = runtime::handle().expect("PostgreSQL runtime should exist");
    runtime.block_on(async {
        let session = provider
            .metadata_sessions
            .get_or_connect(&profile)
            .await
            .expect("read-only catalog session should be available");
        let client = session.catalog_client.lock().await;
        client
            .query_one("SELECT pg_backend_pid()", &[])
            .await
            .expect("catalog backend pid should be readable")
            .get(0)
    })
}

fn compare_metadata_session_timings(
    cached_provider: &PostgresProvider,
    profile: &PostgresConnectionProfile,
) {
    use std::time::{Duration, Instant};

    const ITERATIONS: usize = 7;
    let request = TableListRequest {
        limit: 100,
        ..TableListRequest::default()
    };
    let mut cached = Vec::with_capacity(ITERATIONS);
    let mut fresh = Vec::with_capacity(ITERATIONS);
    for iteration in 0..ITERATIONS {
        let measure_cached = || {
            let start = Instant::now();
            cached_provider
                .list_tables(profile.clone(), request.clone())
                .expect("cached metadata request should succeed");
            start.elapsed()
        };
        let measure_fresh = || {
            let start = Instant::now();
            PostgresProvider::new()
                .list_tables(profile.clone(), request.clone())
                .expect("fresh metadata request should succeed");
            start.elapsed()
        };
        if iteration.is_multiple_of(2) {
            fresh.push(measure_fresh());
            cached.push(measure_cached());
        } else {
            cached.push(measure_cached());
            fresh.push(measure_fresh());
        }
    }
    let median = |samples: &mut [Duration]| {
        samples.sort_unstable();
        samples[samples.len() / 2]
    };
    let cached_median = median(&mut cached);
    let fresh_median = median(&mut fresh);
    eprintln!(
        "PostgreSQL table-list request: reused session median={:.2} ms, fresh connection median={:.2} ms, speedup={:.2}x",
        cached_median.as_secs_f64() * 1000.0,
        fresh_median.as_secs_f64() * 1000.0,
        fresh_median.as_secs_f64() / cached_median.as_secs_f64()
    );
}

fn compare_primary_key_preview_timings(
    provider: &PostgresProvider,
    profile: &PostgresConnectionProfile,
    schema: &str,
    table: &str,
    primary_key: &str,
    offset: usize,
) {
    use std::time::Instant;

    const ITERATIONS: usize = 7;
    let sort = (primary_key.to_owned(), false);
    assert!(
        offset > 0,
        "the preview cursor benchmark needs a later page"
    );
    let previous_page = provider
        .preview_table_page(
            profile.clone(),
            schema.to_owned(),
            table.to_owned(),
            preview_request(
                1,
                offset - 1,
                Some(sort.clone()),
                vec![primary_key.to_owned()],
                None,
            ),
        )
        .expect("cursor boundary row should load");
    let column_index = previous_page
        .columns
        .iter()
        .position(|column| column == primary_key)
        .expect("primary-key column should be present in the preview");
    let cursor = TableDataCursor {
        values: vec![previous_page.rows[0][column_index].clone()],
        direction: TableDataCursorDirection::After,
    };
    let mut offset_times = Vec::with_capacity(ITERATIONS);
    let mut keyset_times = Vec::with_capacity(ITERATIONS);
    let measure = |primary_key_columns, page_cursor| {
        let started = Instant::now();
        let result = provider
            .preview_table_page(
                profile.clone(),
                schema.to_owned(),
                table.to_owned(),
                preview_request(
                    100,
                    offset,
                    Some(sort.clone()),
                    primary_key_columns,
                    page_cursor,
                ),
            )
            .expect("table preview benchmark query should succeed");
        (started.elapsed(), result)
    };

    for iteration in 0..ITERATIONS {
        let (offset_time, offset_page, keyset_time, keyset_page) = if iteration.is_multiple_of(2) {
            let (elapsed, page) = measure(Vec::new(), None);
            let (keyset_elapsed, keyset_page) =
                measure(vec![primary_key.to_owned()], Some(cursor.clone()));
            (elapsed, page, keyset_elapsed, keyset_page)
        } else {
            let (keyset_elapsed, keyset_page) =
                measure(vec![primary_key.to_owned()], Some(cursor.clone()));
            let (elapsed, page) = measure(Vec::new(), None);
            (elapsed, page, keyset_elapsed, keyset_page)
        };
        assert_eq!(keyset_page.rows, offset_page.rows);
        assert_eq!(keyset_page.has_next, offset_page.has_next);
        offset_times.push(offset_time);
        keyset_times.push(keyset_time);
    }
    offset_times.sort_unstable();
    keyset_times.sort_unstable();
    let offset_median = offset_times[ITERATIONS / 2];
    let keyset_median = keyset_times[ITERATIONS / 2];
    eprintln!(
        "PostgreSQL deep PK-sorted preview at offset {offset}: OFFSET median={:.2} ms, keyset median={:.2} ms, speedup={:.2}x",
        offset_median.as_secs_f64() * 1000.0,
        keyset_median.as_secs_f64() * 1000.0,
        offset_median.as_secs_f64() / keyset_median.as_secs_f64()
    );
}

fn preview_request(
    limit: usize,
    offset: usize,
    sort: Option<(String, bool)>,
    primary_key_columns: Vec<String>,
    cursor: Option<TableDataCursor>,
) -> TablePreviewPageRequest {
    TablePreviewPageRequest {
        limit,
        offset,
        sort,
        primary_key_columns,
        cursor,
        filters: Vec::new(),
    }
}

fn assert_cursor_navigation_pages(
    provider: &PostgresProvider,
    profile: &PostgresConnectionProfile,
    schema: &str,
    table: &str,
    primary_key: &str,
    offset: usize,
) {
    assert!(offset >= 100, "cursor navigation needs at least two pages");
    for descending in [false, true] {
        let sort = (primary_key.to_owned(), descending);
        let key_columns = vec![primary_key.to_owned()];
        let page_before = provider
            .preview_table_page(
                profile.clone(),
                schema.to_owned(),
                table.to_owned(),
                preview_request(1, offset - 1, Some(sort.clone()), key_columns.clone(), None),
            )
            .expect("row before the page should load");
        let current_page = provider
            .preview_table_page(
                profile.clone(),
                schema.to_owned(),
                table.to_owned(),
                preview_request(100, offset, Some(sort.clone()), key_columns.clone(), None),
            )
            .expect("offset page should load");
        let column_index = current_page
            .columns
            .iter()
            .position(|column| column == primary_key)
            .expect("primary-key column should be present");
        let next_page = provider
            .preview_table_page(
                profile.clone(),
                schema.to_owned(),
                table.to_owned(),
                preview_request(
                    100,
                    offset,
                    Some(sort.clone()),
                    key_columns.clone(),
                    Some(TableDataCursor {
                        values: vec![page_before.rows[0][column_index].clone()],
                        direction: TableDataCursorDirection::After,
                    }),
                ),
            )
            .expect("keyset next page should load");
        assert_eq!(next_page.rows, current_page.rows);
        assert_eq!(next_page.has_next, current_page.has_next);

        let previous_offset = offset - 100;
        let expected_previous = provider
            .preview_table_page(
                profile.clone(),
                schema.to_owned(),
                table.to_owned(),
                preview_request(
                    100,
                    previous_offset,
                    Some(sort.clone()),
                    key_columns.clone(),
                    None,
                ),
            )
            .expect("previous offset page should load");
        let previous_page = provider
            .preview_table_page(
                profile.clone(),
                schema.to_owned(),
                table.to_owned(),
                preview_request(
                    100,
                    previous_offset,
                    Some(sort.clone()),
                    key_columns,
                    Some(TableDataCursor {
                        values: vec![current_page.rows[0][column_index].clone()],
                        direction: TableDataCursorDirection::Before,
                    }),
                ),
            )
            .expect("keyset previous page should load");
        assert_eq!(previous_page.rows, expected_previous.rows);
        assert_eq!(previous_page.has_next, expected_previous.has_next);
    }
}

fn assert_composite_cursor_navigation_pages(
    provider: &PostgresProvider,
    profile: &PostgresConnectionProfile,
    schema: &str,
    table: &str,
    key_columns: &[String],
    offset: usize,
) {
    assert!(
        key_columns.len() > 1,
        "composite cursor test needs multiple keys"
    );
    assert!(offset >= 100, "cursor navigation needs at least two pages");
    for descending in [false, true] {
        let sort = (key_columns[0].clone(), descending);
        let first_page_without_key_hint = provider
            .preview_table_page(
                profile.clone(),
                schema.to_owned(),
                table.to_owned(),
                preview_request(100, 0, Some(sort.clone()), Vec::new(), None),
            )
            .expect("first composite-key sorted page should load without cached key metadata");
        let first_page_with_key_hint = provider
            .preview_table_page(
                profile.clone(),
                schema.to_owned(),
                table.to_owned(),
                preview_request(100, 0, Some(sort.clone()), key_columns.to_vec(), None),
            )
            .expect("first composite-key sorted page should load with key metadata");
        assert_eq!(
            first_page_without_key_hint.rows,
            first_page_with_key_hint.rows
        );
        let before_page = provider
            .preview_table_page(
                profile.clone(),
                schema.to_owned(),
                table.to_owned(),
                preview_request(
                    1,
                    offset - 1,
                    Some(sort.clone()),
                    key_columns.to_vec(),
                    None,
                ),
            )
            .expect("composite key cursor boundary should load");
        let current_page = provider
            .preview_table_page(
                profile.clone(),
                schema.to_owned(),
                table.to_owned(),
                preview_request(100, offset, Some(sort.clone()), key_columns.to_vec(), None),
            )
            .expect("composite-key sorted offset page should load");
        let cursor_values =
            |row: &[String], columns: &[String], result: &crate::infrastructure::QueryResult| {
                columns
                    .iter()
                    .map(|column| {
                        result
                            .columns
                            .iter()
                            .position(|name| name == column)
                            .and_then(|index| row.get(index))
                            .cloned()
                    })
                    .collect::<Option<Vec<_>>>()
                    .expect("all composite key columns should be present")
            };
        let next_page = provider
            .preview_table_page(
                profile.clone(),
                schema.to_owned(),
                table.to_owned(),
                preview_request(
                    100,
                    offset,
                    Some(sort.clone()),
                    key_columns.to_vec(),
                    Some(TableDataCursor {
                        values: cursor_values(&before_page.rows[0], key_columns, &before_page),
                        direction: TableDataCursorDirection::After,
                    }),
                ),
            )
            .expect("composite keyset next page should load");
        assert_eq!(next_page.rows, current_page.rows);
        let next_cursor = TableDataCursor {
            values: cursor_values(&before_page.rows[0], key_columns, &before_page),
            direction: TableDataCursorDirection::After,
        };
        let mut offset_times = Vec::with_capacity(7);
        let mut keyset_times = Vec::with_capacity(7);
        for _ in 0..7 {
            let started = Instant::now();
            let offset_result = provider
                .preview_table_page(
                    profile.clone(),
                    schema.to_owned(),
                    table.to_owned(),
                    preview_request(100, offset, Some(sort.clone()), key_columns.to_vec(), None),
                )
                .expect("composite-key offset benchmark page should load");
            offset_times.push(started.elapsed());

            let started = Instant::now();
            let keyset_result = provider
                .preview_table_page(
                    profile.clone(),
                    schema.to_owned(),
                    table.to_owned(),
                    preview_request(
                        100,
                        offset,
                        Some(sort.clone()),
                        key_columns.to_vec(),
                        Some(next_cursor.clone()),
                    ),
                )
                .expect("composite-key keyset benchmark page should load");
            keyset_times.push(started.elapsed());
            assert_eq!(keyset_result.rows, offset_result.rows);
        }
        offset_times.sort_unstable();
        keyset_times.sort_unstable();
        eprintln!(
            "PostgreSQL composite-PK preview at offset {offset}: OFFSET median={:.2} ms, keyset median={:.2} ms, speedup={:.2}x",
            offset_times[3].as_secs_f64() * 1000.0,
            keyset_times[3].as_secs_f64() * 1000.0,
            offset_times[3].as_secs_f64() / keyset_times[3].as_secs_f64()
        );

        let previous_offset = offset - 100;
        let expected_previous = provider
            .preview_table_page(
                profile.clone(),
                schema.to_owned(),
                table.to_owned(),
                preview_request(
                    100,
                    previous_offset,
                    Some(sort.clone()),
                    key_columns.to_vec(),
                    None,
                ),
            )
            .expect("composite-key previous offset page should load");
        let previous_page = provider
            .preview_table_page(
                profile.clone(),
                schema.to_owned(),
                table.to_owned(),
                preview_request(
                    100,
                    previous_offset,
                    Some(sort),
                    key_columns.to_vec(),
                    Some(TableDataCursor {
                        values: cursor_values(&current_page.rows[0], key_columns, &current_page),
                        direction: TableDataCursorDirection::Before,
                    }),
                ),
            )
            .expect("composite keyset previous page should load");
        assert_eq!(previous_page.rows, expected_previous.rows);
    }
}

fn compare_inspection_timings(
    cached_provider: &PostgresProvider,
    profile: &PostgresConnectionProfile,
) {
    use std::time::{Duration, Instant};

    const ITERATIONS: usize = 7;
    let request = TableListRequest {
        limit: 100,
        ..TableListRequest::default()
    };
    let mut cached = Vec::with_capacity(ITERATIONS);
    let mut fresh = Vec::with_capacity(ITERATIONS);
    for iteration in 0..ITERATIONS {
        let measure_cached = || {
            let start = Instant::now();
            cached_provider
                .inspect(profile.clone(), request.clone())
                .expect("cached metadata refresh should succeed");
            start.elapsed()
        };
        let measure_fresh = || {
            let start = Instant::now();
            PostgresProvider::new()
                .inspect(profile.clone(), request.clone())
                .expect("fresh metadata refresh should succeed");
            start.elapsed()
        };
        if iteration.is_multiple_of(2) {
            fresh.push(measure_fresh());
            cached.push(measure_cached());
        } else {
            cached.push(measure_cached());
            fresh.push(measure_fresh());
        }
    }
    let median = |samples: &mut [Duration]| {
        samples.sort_unstable();
        samples[samples.len() / 2]
    };
    let cached_median = median(&mut cached);
    let fresh_median = median(&mut fresh);
    eprintln!(
        "PostgreSQL metadata refresh: reused session median={:.2} ms, fresh connection median={:.2} ms, speedup={:.2}x",
        cached_median.as_secs_f64() * 1000.0,
        fresh_median.as_secs_f64() * 1000.0,
        fresh_median.as_secs_f64() / cached_median.as_secs_f64()
    );
}

fn compare_read_statement_timings(
    provider: &PostgresProvider,
    profile: &PostgresConnectionProfile,
) {
    use std::time::{Duration, Instant};

    const ITERATIONS: usize = 7;
    const CACHED_SQL: &str = "SELECT 1 AS tablex_read_cache_benchmark";
    let mut cached = Vec::with_capacity(ITERATIONS);
    let mut uncached = Vec::with_capacity(ITERATIONS);
    provider
        .execute_read_query(profile.clone(), CACHED_SQL.into())
        .expect("benchmark query should warm the statement cache");

    for iteration in 0..ITERATIONS {
        let measure_cached = || {
            let start = Instant::now();
            provider
                .execute_read_query(profile.clone(), CACHED_SQL.into())
                .expect("cached benchmark query should succeed");
            start.elapsed()
        };
        let measure_uncached = || {
            let start = Instant::now();
            provider
                .execute_read_query(
                    profile.clone(),
                    format!("SELECT 1 AS tablex_read_cache_benchmark /* miss {iteration} */"),
                )
                .expect("uncached benchmark query should succeed");
            start.elapsed()
        };
        if iteration.is_multiple_of(2) {
            uncached.push(measure_uncached());
            cached.push(measure_cached());
        } else {
            cached.push(measure_cached());
            uncached.push(measure_uncached());
        }
    }
    let median = |samples: &mut [Duration]| {
        samples.sort_unstable();
        samples[samples.len() / 2]
    };
    let cached_median = median(&mut cached);
    let uncached_median = median(&mut uncached);
    eprintln!(
        "PostgreSQL repeated read query: cached median={:.2} ms, uncached prepare median={:.2} ms, speedup={:.2}x",
        cached_median.as_secs_f64() * 1000.0,
        uncached_median.as_secs_f64() * 1000.0,
        uncached_median.as_secs_f64() / cached_median.as_secs_f64()
    );
}
