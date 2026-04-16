# teolog — Cross-stack wide-event structured logging

default:
    @just --list

# === Quality ===

# Run all checks (lint + typecheck + test)
qa: lint check test

# Typecheck all stacks
check: check-rust check-ts

check-rust:
    cargo check --manifest-path crate/Cargo.toml

check-ts:
    cd ts && bun run typecheck

# Lint all stacks
lint: lint-rust

lint-rust:
    cargo clippy --manifest-path crate/Cargo.toml -- -D warnings

# Format all stacks
fmt: fmt-rust fmt-ts

fmt-rust:
    cargo fmt --manifest-path crate/Cargo.toml

fmt-ts:
    cd ts && bunx prettier --write src/

# === Tests ===

# Run all tests
test: test-rust test-ts test-shell

test-rust:
    cargo test --manifest-path crate/Cargo.toml --all-targets

test-ts:
    cd ts && bun test

test-shell:
    bash scripts/test-analyze-logs.sh

# === Build ===

build: build-rust build-ts

build-rust:
    cargo build --manifest-path crate/Cargo.toml --release

build-ts:
    cd ts && bun run build

# === Utilities ===

# Pretty-print NDJSON logs from stdin
logs-pretty:
    @bash scripts/logs-pretty.sh

# Analyze a teolog NDJSON file
analyze *ARGS:
    @bash scripts/analyze-logs.sh {{ ARGS }}

# Clean all build artifacts
clean:
    cargo clean --manifest-path crate/Cargo.toml
    rm -rf ts/dist ts/node_modules
