# teolog

Cross-stack wide-event structured logging. One NDJSON format, three implementations.

Repo layout:
- Rust crate: `crate/`
- TypeScript package: `ts/`
- Root helpers: Cargo workspace + Bun workspace for smoother local consumption

## What

teolog emits **wide events** — single JSON objects per operation that capture all context, data, timing, and errors in one line. No scattered `log.info` breadcrumbs. Every event is self-contained and queryable with `jq`.

Shared format across:
- **Rust** (`crate/`) — CLIs, Tauri backends
- **TypeScript** (`ts/`) — Svelte frontends, Node services
- **Lua/Neovim** → see [teolog.nvim](https://github.com/oleksiiluchnikov/teolog.nvim)

## Schema

Every event follows `schema/wide-event.schema.json`:

```json
{
  "ts": "2026-03-02T10:30:00Z",
  "lvl": "info",
  "msg": "menu_loaded",
  "project": "kolo",
  "service": "api",
  "trace_id": "a1b2c3d4e5f6a7b8c9d0e1f2a3b4c5d6",
  "span_id": "1a2b3c4d5e6f7a8b",
  "parent_id": "9f8e7d6c5b4a3f2e",
  "span": "load_menu",
  "dur_ms": 42,
  "source": "src/menu.rs",
  "lnum": 42,
  "ctx": { "session": "abc123", "user": "dev" },
  "data": { "menu_count": 5, "source": "disk" }
}
```

Required: `ts`, `lvl`, `msg`. Auto-generated: `trace_id`, `span_id`. Everything else is optional.

### Trace Correlation

Every event gets OTEL-compatible IDs:
- `trace_id` — 32 hex chars (16 bytes), groups related events across services
- `span_id` — 16 hex chars (8 bytes), unique per span/event
- `parent_id` — links child spans to their parent
- `service` — identifies which service/component emitted the event

## Usage

### Rust

```rust
use teolog::{init, RotatingFileSink, RotationPolicy};

let logger = init("my-app")
    .with_sink(RotatingFileSink::open("app.log", RotationPolicy::new(10 * 1024 * 1024, 5))?)
    .with_service("api");

// One-shot events get auto-generated trace_id + span_id
logger.info("started");

// Spans track duration and support child spans
let mut span = logger.span("load_config");
span.set("path", "/etc/app.toml");
span.data("keys_loaded", 42);

// Child spans share trace_id, link via parent_id
let child = span.span("parse_toml");
drop(child); // emits with parent_id = span.span_id()
span.finish(); // or let drop emit automatically
```

### TypeScript

```typescript
import { Logger, ConsoleSink, RotatingFileSink } from 'teolog';

const log = new Logger('my-app', RotatingFileSink.open('app.log', {
  maxBytes: 10 * 1024 * 1024,
  maxFiles: 5,
}), {
  service: 'web-frontend'
});
log.info('started');

const span = log.span('fetch_data');
span.set('url', '/api/items');
span.addData('count', 100);
span.finish(); // emits with duration

// Child spans share trace_id
const child = span.span('parse_response');
child.finish(); // parent_id = span.spanId
```

Rust field helpers:

```rust
use serde_json::json;
use teolog::{init, Level};

let log = init("my-app");
log.emit_fields(Level::Info, "menu_loaded", [
    ("count", json!(5)),
    ("source", json!("disk")),
]);
log.warn_fields("menu_partial", [
    ("missing_icons", json!(2)),
]);
log.error_fields("menu_failed", [
    ("code", json!(500)),
]);
log.debug_fields("menu_debug", [
    ("cache_hit", json!(true)),
]);
log.trace_fields("menu_trace", [
    ("phase", json!("collect")),
]);
```

### Lua (Neovim)

See [teolog.nvim](https://github.com/oleksiiluchnikov/teolog.nvim) — the dedicated Neovim plugin with async file writes, log rotation, and user commands.

Pi/OpenCode bootstrap plugins also use async rotating file writes for `.teolog/pi.ndjson`.

## Sinks

Each implementation has matching sink types:

| Sink | Rust | TypeScript | Lua |
|------|------|------------|-----|
| File (NDJSON) | `FileSink`, `RotatingFileSink` | `FileSink`, `RotatingFileSink` | `FileSink` (async via vim.uv) |
| Console/Notify | `StdoutSink` | `ConsoleSink` | `NotifySink` (vim.notify) |
| HTTP (batch POST) | `HttpSink` (feature `http-sink`) | `HttpSink` | `HttpSink` (curl jobstart) |
| Callback | `WriterSink` | `CallbackSink` | `CallbackSink` |
| Multiple | `MultiSink` | `MultiSink` | `MultiSink` |

### Rotation and retention

For Rust and TypeScript file logging, use `RotatingFileSink` to cap disk usage.

Defaults:
- `maxBytes`: 10 MiB
- `maxFiles`: 5 archives

Rotation behavior:
- active file stays at the configured path
- archives become `app.log.1`, `app.log.2`, ...
- when `maxFiles = 0`, rotation truncates instead of keeping archives

Agent/bootstrap consumers now use async rotating writes by default for `.teolog/pi.ndjson`.
Override retention with env vars:
- `TEOLOG_MAX_LOG_BYTES`
- `TEOLOG_MAX_LOG_FILES`

### Privacy-safe defaults

By default, teolog avoids logging machine-specific private details in TypeScript and agent/bootstrap consumers:
- absolute workspace paths are redacted to `repo` name only
- Pi `session_file` path is not logged
- raw CLI args are reduced to `cli_arg_count`
- JavaScript stack traces are omitted
- TypeScript `source` paths are reduced to basename only

Opt in to full diagnostics when needed:
- `TEOLOG_LOG_PATHS=1`
- `TEOLOG_LOG_STACKS=1`
- `TEOLOG_LOG_CLI_ARGS=1`

## Agentic CLI

`teolog` now ships with compact, agent-facing log queries and a Pi bootstrap.

```bash
# Build CLI
cargo build --manifest-path crate/Cargo.toml --release

# Compact machine-readable status
./crate/target/release/teolog status .teolog/pi.ndjson --json
./crate/target/release/teolog recent .teolog/pi.ndjson --json --limit 10 --fields ts,msg,lvl,trace_id
./crate/target/release/teolog errors .teolog/pi.ndjson --json --limit 5
./crate/target/release/teolog trace a1b2c3d4 .teolog/pi.ndjson --json --summary-only
./crate/target/release/teolog summary .teolog/pi.ndjson --json --limit 5 --slow-ms 200
./crate/target/release/teolog search "menu" .teolog/pi.ndjson --json --limit 10

# Bootstrap project-local Pi extension
./crate/target/release/teolog bootstrap --project-dir . --log-file .teolog/pi.ndjson --json

# Bootstrap OpenCode plugin parity
./crate/target/release/teolog bootstrap --project-dir . --log-file .teolog/pi.ndjson --runtime opencode --json

# Bootstrap + sync directly into live OpenCode config
./crate/target/release/teolog bootstrap --project-dir . --log-file .teolog/pi.ndjson --runtime opencode --sync-opencode-live --json

# Sync repo plugin into live OpenCode config
just sync-opencode-plugin

# Live delta summaries
./crate/target/release/teolog monitor .teolog/pi.ndjson --json --limit 5 --slow-ms 200

# Persist last seen position across restarts
./crate/target/release/teolog monitor .teolog/pi.ndjson --json --once --since-last

# End-to-end smoke: pi + opencode + teolog summary
just smoke-agent-loop
```

Output defaults to table on TTY, JSON otherwise. Use `--json`, `--output json|compact|ndjson|table`, `--fields`, and `--count` to keep agent responses small.

Project-local bootstrap targets:
- Pi: `.pi/extensions/teolog/index.ts`
- OpenCode: `tools/teolog.ts`
- Live OpenCode sync target: `${XDG_CONFIG_HOME:-~/.config}/opencode/plugins/teolog.ts`

## Log Analysis

```bash
# Pretty-print NDJSON stream
tail -f app.log | bash scripts/logs-pretty.sh

# Query tools
bash scripts/analyze-logs.sh errors app.log
bash scripts/analyze-logs.sh warnings app.log
bash scripts/analyze-logs.sh spans app.log
bash scripts/analyze-logs.sh slow 200 app.log
bash scripts/analyze-logs.sh project teolog app.log
bash scripts/analyze-logs.sh stats app.log
bash scripts/analyze-logs.sh recent 50 app.log
bash scripts/analyze-logs.sh search "menu" app.log
bash scripts/analyze-logs.sh trace app.log            # list recent traces
bash scripts/analyze-logs.sh trace a1b2c3d4 app.log  # show matching trace
bash scripts/analyze-logs.sh clear app.log           # confirm before truncate
```

Note: `search` uses jq `test(...; "i")`, so query is regex, not plain substring. Escape regex metacharacters if needed.

Pi hot reload note:
- project-local extensions live under `.pi/extensions/`
- in an interactive pi session, run `/reload` after editing the extension
- for non-interactive verification, `pi -ne -e .pi/extensions/teolog/index.ts ...` loads the extension explicitly

## Menubar app (macOS)

Moved out of this repo.

New repo:
- `~/projects/teolog-monitor`

It is now a Tauri v2 menubar app.

Typical flow:

```bash
cd ~/projects/teolog-monitor
just check
just dev
just build
```

## Install and release flow

## Consumption from other repos

### Local repos

Rust:
```toml
[dependencies]
teolog = { path = "../teolog/crate" }
```

TypeScript / Bun:
```bash
bun add ../teolog/ts
```

### Remote packages

Current repo is split by stack:
- Rust package lives in `crate/`
- TypeScript package lives in `ts/`

Cleanest distribution path:
- publish Rust crate from `crate/` to crates.io
- publish TypeScript package from `ts/` to npm

Repo root now includes:
- `Cargo.toml` workspace for smoother Rust tooling
- `package.json` Bun workspace for smoother JS tooling


```bash
# Local install
cargo install --path crate --bin teolog --force

# Verify
teolog --help
teolog status .teolog/pi.ndjson --json

# Release build artifact
cargo build --manifest-path crate/Cargo.toml --release
```

## Development

```bash
just qa                  # lint + typecheck + test (repo stacks)
just test                # run Rust + TypeScript + shell tests
just test-rust           # Rust only
just test-ts             # TypeScript only
just test-shell          # analyzer shell tests
just sync-opencode-plugin   # copy tools/teolog.ts into live OpenCode plugins dir
just smoke-agent-loop      # noninteractive pi + opencode smoke loop
just fmt                   # format all stacks
just build                 # build all stacks
```

## License

MIT
