# teolog

Cross-stack wide-event structured logging. One NDJSON format, three implementations.

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
use teolog::{Logger, FileSink, Level};

let logger = Logger::new("my-app", Box::new(FileSink::new("app.log")?))
    .with_service("api");

// One-shot events get auto-generated trace_id + span_id
logger.info("started");

// Spans track duration and support child spans
let mut span = logger.span("load_config");
span.set("path", "/etc/app.toml");
span.data("keys_loaded", 42);
// span emits on drop with duration

// Child spans share trace_id, link via parent_id
let child = span.span("parse_toml");
drop(child); // emits with parent_id = span.span_id()
```

### TypeScript

```typescript
import { Logger, ConsoleSink } from 'teolog';

const log = new Logger('my-app', new ConsoleSink(), {
  service: 'web-frontend'
});
log.info('started');

const span = log.span('fetch_data');
span.set('url', '/api/items');
span.data('count', 100);
span.finish(); // emits with duration

// Child spans share trace_id
const child = span.span('parse_response');
child.finish(); // parent_id = span.spanId()
```

### Lua (Neovim)

See [teolog.nvim](https://github.com/oleksiiluchnikov/teolog.nvim) — the dedicated Neovim plugin with async file writes, log rotation, and user commands.

## Sinks

Each implementation has matching sink types:

| Sink | Rust | TypeScript | Lua |
|------|------|------------|-----|
| File (NDJSON) | `FileSink` | - | `FileSink` (async via vim.uv) |
| Console/Notify | `StdoutSink` | `ConsoleSink` | `NotifySink` (vim.notify) |
| HTTP (batch POST) | `HttpSink` (feature `http-sink`) | `HttpSink` | `HttpSink` (curl jobstart) |
| Callback | `WriterSink` | `CallbackSink` | `CallbackSink` |
| Multiple | `MultiSink` | `MultiSink` | `MultiSink` |

## Log Analysis

```bash
# Pretty-print NDJSON stream
tail -f app.log | bash scripts/logs-pretty.sh

# Query tools
bash scripts/analyze-logs.sh errors app.log
bash scripts/analyze-logs.sh slow 200 app.log
bash scripts/analyze-logs.sh stats app.log
bash scripts/analyze-logs.sh search "menu" app.log
bash scripts/analyze-logs.sh trace              # list recent traces
bash scripts/analyze-logs.sh trace a1b2c3d4     # show all events in a trace
```

## Development

```bash
just qa        # lint + typecheck + test (all stacks)
just test      # run all tests
just test-rust # rust only
just test-lua  # lua only (headless nvim)
just fmt       # format all stacks
just build     # build all stacks
```

## License

MIT
