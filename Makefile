.DEFAULT_GOAL := help

.PHONY: help fmt fmt-check check test build run verify clean

help:
	@printf '%s\n' \
		'Available targets:' \
		'  make fmt        Format Rust sources' \
		'  make fmt-check  Check Rust formatting' \
		'  make check      Type-check the project' \
		'  make test       Run Rust tests' \
		'  make build      Build the debug application' \
		'  make run        Run the desktop application' \
		'  make verify     Run formatting, checks, and tests' \
		'  make clean      Remove Cargo build artifacts'

fmt:
	cargo fmt

fmt-check:
	cargo fmt -- --check

check:
	cargo check

test:
	cargo test

build:
	cargo build

run:
	cargo run

verify: fmt-check check test

clean:
	cargo clean
