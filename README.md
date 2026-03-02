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
  "span": "load_menu",
  "dur_ms": 42,
  "ctx": { "session": "abc123", "user": "dev" },
  "data": { "menu_count": 5, "source": "disk" }
}
```

Required: `ts`, `lvl`, `msg`. Everything else is optional.

## Usage

### Rust

```rust
use teolog::{Logger, FileSink, Level};

let logger = Logger::new("my-app", Box::new(FileSink::new("app.log")?));
logger.info("started");

let mut span = logger.span("load_config");
span.set("path", "/etc/app.toml");
span.data("keys_loaded", 42);
// span emits on drop with duration
```

### TypeScript

```typescript
import { Logger, ConsoleSink } from 'teolog';

const log = new Logger('my-app', new ConsoleSink());
log.info('started');

const span = log.span('fetch_data');
span.set('url', '/api/items');
span.data('count', 100);
span.finish(); // emits with duration
```

### Lua (Neovim)

See [teolog.nvim](https://github.com/oleksiiluchnikov/teolog.nvim) — the dedicated Neovim plugin with async file writes, log rotation, and user commands.

## Sinks

Each implementation has matching sink types:

| Sink | Rust | TypeScript | Lua |
|------|------|------------|-----|
| File (NDJSON) | `FileSink` | - | `FileSink` (async via vim.uv) |
| Console/Notify | `StdoutSink` | `ConsoleSink` | `NotifySink` (vim.notify) |
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
