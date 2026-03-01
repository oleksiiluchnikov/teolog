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
lint: lint-rust lint-lua

lint-rust:
    cargo clippy --manifest-path crate/Cargo.toml -- -D warnings

lint-lua:
    @echo "luacheck not configured (optional)"

# Format all stacks
fmt: fmt-rust fmt-ts fmt-lua

fmt-rust:
    cargo fmt --manifest-path crate/Cargo.toml

fmt-ts:
    cd ts && bunx prettier --write src/

fmt-lua:
    stylua lua/

# === Tests ===

# Run all tests
test: test-rust test-lua

test-rust:
    cargo test --manifest-path crate/Cargo.toml

test-lua:
    nvim --headless -u NONE --listen /tmp/teolog-test-$$.sock \
        -c "set rtp+=." \
        -c "luafile lua/teolog/test_spec.lua" \
        -c "qa!"

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
