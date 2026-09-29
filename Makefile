.DEFAULT_GOAL := help

.PHONY: help fmt fmt-check check clippy test test-postgres build build-release run verify tag push-tag clean

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
		'  make tag        Create an annotated v<package version> Git tag locally' \
		'  make push-tag   Push the version tag to origin' \
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

tag:
	@version="$$(awk -F '"' '/^\[package\]$$/ { in_package=1; next } /^\[/ { in_package=0 } in_package && /^version = / { print $$2; exit }' Cargo.toml)"; \
	test -n "$$version" || { echo 'Could not read package version from Cargo.toml' >&2; exit 1; }; \
	tag="v$$version"; \
	if git rev-parse -q --verify "refs/tags/$$tag" >/dev/null; then \
		echo "Tag $$tag already exists" >&2; exit 1; \
	fi; \
	git tag -a "$$tag" -m "tableX $$tag"

push-tag:
	@version="$$(awk -F '"' '/^\[package\]$$/ { in_package=1; next } /^\[/ { in_package=0 } in_package && /^version = / { print $$2; exit }' Cargo.toml)"; \
	test -n "$$version" || { echo 'Could not read package version from Cargo.toml' >&2; exit 1; }; \
	tag="v$$version"; \
	git rev-parse -q --verify "refs/tags/$$tag" >/dev/null || { echo "Create the local tag with 'make tag' first" >&2; exit 1; }; \
	git push origin "$$tag"

clean:
	cargo clean
