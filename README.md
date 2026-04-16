# teolog

Structured NDJSON logging for Rust and TypeScript.

## Packages

- Rust crate: `crate/`
- TypeScript package: `ts/`
- Schema: `schema/wide-event.schema.json`

## Event shape

Each log line is one JSON object.

Required fields:
- `ts`
- `lvl`
- `msg`

Common fields:
- `trace_id`
- `span_id`
- `parent_id`
- `span`
- `dur_ms`
- `project`
- `service`
- `ctx`
- `data`
- `err`

## Rust

```rust
use teolog::{init, RotatingFileSink, RotationPolicy};

let log = init("my-app")
    .with_sink(RotatingFileSink::open("app.log", RotationPolicy::new(10 * 1024 * 1024, 5))?)
    .with_service("api");

log.info("started");

let mut span = log.span("load_config");
span.set("path", "/etc/app.toml");
span.data("keys_loaded", 42);
span.finish();
```

## TypeScript

```ts
import { Logger, RotatingFileSink } from "teolog";

const log = new Logger("my-app", RotatingFileSink.open("app.log", {
  maxBytes: 10 * 1024 * 1024,
  maxFiles: 5,
}), {
  service: "web",
});

log.info("started");

const span = log.span("fetch_data");
span.set("url", "/api/items");
span.addData("count", 100);
span.finish();
```

## Rotation

Use rotating sinks to cap disk use.

Defaults:
- `maxBytes`: 10 MiB
- `maxFiles`: 5

Archives become `app.log.1`, `app.log.2`, ...
Set `maxFiles = 0` to truncate instead of keeping archives.

## Privacy defaults

TypeScript logging avoids machine-specific details by default:
- source paths reduced to basename
- JS stack traces omitted

Opt in when needed:
- `TEOLOG_LOG_PATHS=1`
- `TEOLOG_LOG_STACKS=1`
- `TEOLOG_LOG_CLI_ARGS=1`

## CLI

Build:

```bash
cargo build --manifest-path crate/Cargo.toml --release
```

Examples:

```bash
./crate/target/release/teolog status app.log --json
./crate/target/release/teolog recent app.log --json --limit 10 --fields ts,msg,lvl,trace_id
./crate/target/release/teolog errors app.log --json --limit 5
./crate/target/release/teolog trace a1b2c3d4 app.log --json --summary-only
./crate/target/release/teolog summary app.log --json --limit 5 --slow-ms 200
./crate/target/release/teolog search "menu" app.log --json --limit 10
./crate/target/release/teolog monitor app.log --json --once --since-last
```

## Local consumption

Rust:

```toml
[dependencies]
teolog = { path = "../teolog/crate" }
```

TypeScript / Bun:

```bash
bun add ../teolog/ts
```

## Development

```bash
just qa
just test
just fmt
just build
```

## License

MIT
