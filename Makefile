.DEFAULT_GOAL := help

.PHONY: help fmt fmt-check check clippy test test-postgres build build-release run verify clean

help:
	@printf '%s\n' \
		'Available targets:' \
		'  make fmt        Format Rust sources' \
		'  make fmt-check  Check Rust formatting' \
		'  make check      Type-check the project' \
		'  make clippy     Run Clippy with warnings denied' \
		'  make test       Run Rust tests' \
		'  make test-postgres  Run the opt-in PostgreSQL smoke test' \
		'  make build      Build the debug application' \
		'  make build-release  Build the optimized release application' \
		'  make run        Run the desktop application' \
		'  make verify     Run formatting, checks, and tests' \
		'  make clean      Remove Cargo build artifacts'

fmt:
	cargo fmt

fmt-check:
	cargo fmt -- --check

check:
	cargo check

clippy:
	cargo clippy --all-targets -- -D warnings

test:
	cargo test

test-postgres:
	@test -n "$(TABLEX_TEST_PG_HOST)" || (echo 'Set TABLEX_TEST_PG_HOST, TABLEX_TEST_PG_PORT, TABLEX_TEST_PG_DATABASE, TABLEX_TEST_PG_USER, and optionally TABLEX_TEST_PG_PASSWORD' && exit 1)
	TABLEX_TEST_PG_HOST="$(TABLEX_TEST_PG_HOST)" \
	TABLEX_TEST_PG_PORT="$(TABLEX_TEST_PG_PORT)" \
	TABLEX_TEST_PG_DATABASE="$(TABLEX_TEST_PG_DATABASE)" \
	TABLEX_TEST_PG_USER="$(TABLEX_TEST_PG_USER)" \
	TABLEX_TEST_PG_PASSWORD="$(TABLEX_TEST_PG_PASSWORD)" \
	TABLEX_TEST_PG_MUTATION_TABLE="$(TABLEX_TEST_PG_MUTATION_TABLE)" \
	cargo test infrastructure::postgres::tests::inspects_configured_postgres_server -- --ignored --exact

build:
	cargo build

build-release:
	cargo build --release

run:
	cargo run

verify: fmt-check check clippy test

clean:
	cargo clean
