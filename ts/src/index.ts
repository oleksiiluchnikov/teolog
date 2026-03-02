// teolog — Cross-stack wide-event structured logging (TypeScript)

export type Level = 'trace' | 'debug' | 'info' | 'warn' | 'error';

/** Matches schema/wide-event.schema.json */
export interface WideEvent {
  ts: string;
  lvl: Level;
  msg: string;
  trace_id?: string;
  span_id?: string;
  parent_id?: string;
  span?: string;
  dur_ms?: number;
  project?: string;
  service?: string;
  module?: string;
  source?: string;
  lnum?: number;
  ctx?: Record<string, unknown>;
  data?: Record<string, unknown>;
  err?: { message: string; kind?: string; stack?: string };
  [key: string]: unknown;
}

// --- ID Generation (OTEL-compatible hex) ---

/** Generate a trace ID: 16 random bytes → 32 lowercase hex chars. */
export function genTraceId(): string {
  const bytes = new Uint8Array(16);
  crypto.getRandomValues(bytes);
  return hexEncode(bytes);
}

/** Generate a span ID: 8 random bytes → 16 lowercase hex chars. */
export function genSpanId(): string {
  const bytes = new Uint8Array(8);
  crypto.getRandomValues(bytes);
  return hexEncode(bytes);
}

function hexEncode(bytes: Uint8Array): string {
  let s = '';
  for (const b of bytes) {
    s += b.toString(16).padStart(2, '0');
  }
  return s;
}

// --- Source Location ---

/** Parse caller file:line from Error.stack (best-effort). */
function captureSource(): { source: string; lnum: number } | undefined {
  const err = new Error();
  if (!err.stack) return undefined;
  // Stack format varies by runtime. Look for the 4th line (skip Error, captureSource, caller, actual caller).
  const lines = err.stack.split('\n');
  // Find first line that looks like a file path after skipping internal frames
  for (let i = 3; i < lines.length; i++) {
    const line = lines[i];
    // Node/Bun: "    at functionName (file:line:col)" or "    at file:line:col"
    const match = line.match(/\((.+):(\d+):\d+\)/) ?? line.match(/at\s+(.+):(\d+):\d+/);
    if (match) {
      return { source: match[1], lnum: parseInt(match[2], 10) };
    }
  }
  return undefined;
}

// --- Sink ---

export interface Sink {
  emit(event: WideEvent): void;
}

/** Emit to console with level-appropriate method. */
export class ConsoleSink implements Sink {
  emit(event: WideEvent): void {
    const json = JSON.stringify(event);
    switch (event.lvl) {
      case 'error': console.error(json); break;
      case 'warn':  console.warn(json); break;
      case 'debug': console.debug(json); break;
      case 'trace': console.debug(json); break;
      default:      console.log(json);
    }
  }
}

/** Emit via a callback (for Tauri plugin-log bridge, HTTP, etc). */
export class CallbackSink implements Sink {
  constructor(private cb: (level: Level, json: string) => void | Promise<void>) {}
  emit(event: WideEvent): void {
    this.cb(event.lvl, JSON.stringify(event));
  }
}

/** Fan out to multiple sinks. */
export class MultiSink implements Sink {
  constructor(private sinks: Sink[]) {}
  emit(event: WideEvent): void {
    for (const s of this.sinks) s.emit(event);
  }
}

/** Batch POST NDJSON to a URL with configurable headers. */
export class HttpSink implements Sink {
  private url: string;
  private headers: Record<string, string>;
  private batchSize: number;
  private flushIntervalMs: number;
  private queue: string[] = [];
  private timer: ReturnType<typeof setInterval> | null = null;

  /**
   * @param url POST endpoint (e.g. `https://api.axiom.co/v1/datasets/logs/ingest`)
   * @param headers Extra headers (e.g. `{ Authorization: "Bearer ..." }`)
   * @param batchSize Flush after this many events (default: 100)
   * @param flushIntervalMs Auto-flush interval in ms (default: 5000, 0 = disabled)
   */
  constructor(
    url: string,
    headers: Record<string, string> = {},
    batchSize = 100,
    flushIntervalMs = 5000,
  ) {
    this.url = url;
    this.headers = headers;
    this.batchSize = batchSize;
    this.flushIntervalMs = flushIntervalMs;

    if (flushIntervalMs > 0) {
      this.timer = setInterval(() => this.flush(), flushIntervalMs);
    }
  }

  emit(event: WideEvent): void {
    this.queue.push(JSON.stringify(event));
    if (this.queue.length >= this.batchSize) {
      this.flush();
    }
  }

  /** Flush all queued events to the HTTP endpoint. */
  flush(): void {
    if (this.queue.length === 0) return;
    const batch = this.queue.splice(0);
    const body = batch.join('\n');

    fetch(this.url, {
      method: 'POST',
      headers: {
        'Content-Type': 'application/x-ndjson',
        ...this.headers,
      },
      body,
    }).catch((err) => {
      console.error('[teolog] HttpSink flush error:', err);
    });
  }

  /** Stop the auto-flush timer and drain remaining events. */
  dispose(): void {
    if (this.timer) {
      clearInterval(this.timer);
      this.timer = null;
    }
    this.flush();
  }
}

// --- Context ---

export class LogContext {
  private fields: Record<string, unknown> = {};

  set(key: string, value: unknown): this {
    this.fields[key] = value;
    return this;
  }

  merge(other: Record<string, unknown>): this {
    Object.assign(this.fields, other);
    return this;
  }

  snapshot(): Record<string, unknown> {
    return { ...this.fields };
  }

  child(): LogContext {
    const c = new LogContext();
    c.fields = { ...this.fields };
    return c;
  }
}

// --- Logger ---

export class Logger {
  private project: string;
  private service?: string;
  private traceId?: string;
  private sink: Sink;
  private context: LogContext;

  constructor(
    project: string,
    sink?: Sink,
    context?: LogContext,
    options?: { service?: string; traceId?: string },
  ) {
    this.project = project;
    this.sink = sink ?? new ConsoleSink();
    this.context = context ?? new LogContext();
    this.service = options?.service;
    this.traceId = options?.traceId;
  }

  /** Set root context fields. */
  setContext(fields: Record<string, unknown>): void {
    this.context.merge(fields);
  }

  /** Set a shared trace_id for all events from this logger. */
  setTraceId(id: string): void {
    this.traceId = id;
  }

  /** Get the current trace ID (or undefined). */
  getTraceId(): string | undefined {
    return this.traceId;
  }

  private resolveTraceId(): string {
    return this.traceId ?? genTraceId();
  }

  /** Start a span (returns a builder). Call .finish() when done. */
  span(name: string): SpanBuilder {
    const loc = captureSource();
    return new SpanBuilder(this, name, this.context.child(), {
      traceId: this.resolveTraceId(),
      service: this.service,
      source: loc?.source,
      lnum: loc?.lnum,
    });
  }

  /** Emit a one-shot event. */
  emit(level: Level, msg: string, data?: Record<string, unknown>): void {
    const loc = captureSource();
    const event: WideEvent = {
      ts: new Date().toISOString(),
      lvl: level,
      msg,
      project: this.project,
      trace_id: this.resolveTraceId(),
      span_id: genSpanId(),
    };
    if (this.service) event.service = this.service;
    if (loc) {
      event.source = loc.source;
      event.lnum = loc.lnum;
    }
    const ctx = this.context.snapshot();
    if (Object.keys(ctx).length > 0) event.ctx = ctx;
    if (data) event.data = data;
    this.sink.emit(event);
  }

  info(msg: string, data?: Record<string, unknown>)  { this.emit('info', msg, data); }
  warn(msg: string, data?: Record<string, unknown>)  { this.emit('warn', msg, data); }
  error(msg: string, data?: Record<string, unknown>) { this.emit('error', msg, data); }
  debug(msg: string, data?: Record<string, unknown>) { this.emit('debug', msg, data); }
  trace(msg: string, data?: Record<string, unknown>) { this.emit('trace', msg, data); }

  /** Emit with an error object. */
  errorWith(msg: string, err: unknown, data?: Record<string, unknown>): void {
    const errDetails = err instanceof Error
      ? { message: err.message, kind: err.name, stack: err.stack }
      : { message: String(err) };
    this.emit('error', msg, { ...data, error: errDetails });
  }

  /** Internal: used by SpanBuilder. */
  _emitRaw(event: WideEvent): void {
    this.sink.emit(event);
  }
}

// --- Span ---

interface SpanOptions {
  traceId: string;
  parentId?: string;
  service?: string;
  source?: string;
  lnum?: number;
}

export class SpanBuilder {
  private logger: Logger;
  private name: string;
  private context: LogContext;
  private _data: Record<string, unknown> = {};
  private started: number;
  private _traceId: string;
  private _spanId: string;
  private _parentId?: string;
  private _service?: string;
  private _source?: string;
  private _lnum?: number;

  constructor(logger: Logger, name: string, context: LogContext, opts: SpanOptions) {
    this.logger = logger;
    this.name = name;
    this.context = context;
    this.started = performance.now();
    this._traceId = opts.traceId;
    this._spanId = genSpanId();
    this._parentId = opts.parentId;
    this._service = opts.service;
    this._source = opts.source;
    this._lnum = opts.lnum;
  }

  /** Get this span's ID (for passing to child spans or other services). */
  get spanId(): string { return this._spanId; }

  /** Get this span's trace ID. */
  get traceId(): string { return this._traceId; }

  /** Add to span context. */
  set(key: string, value: unknown): this {
    this.context.set(key, value);
    return this;
  }

  /** Add to span data payload. */
  addData(key: string, value: unknown): this {
    this._data[key] = value;
    return this;
  }

  /** Record an error. */
  err(err: unknown): this {
    this._data['error'] = err instanceof Error
      ? { message: err.message, kind: err.name, stack: err.stack }
      : { message: String(err) };
    return this;
  }

  /** Create a child span that shares this span's trace_id. */
  span(name: string): SpanBuilder {
    const loc = captureSource();
    return new SpanBuilder(this.logger, name, this.context.child(), {
      traceId: this._traceId,
      parentId: this._spanId,
      service: this._service,
      source: loc?.source,
      lnum: loc?.lnum,
    });
  }

  /** Emit the span event. */
  finish(): void {
    const dur_ms = Math.round(performance.now() - this.started);
    const hasError = 'error' in this._data;
    const ctx = this.context.snapshot();

    const event: WideEvent = {
      ts: new Date().toISOString(),
      lvl: hasError ? 'error' : 'info',
      msg: this.name,
      span: this.name,
      dur_ms,
      trace_id: this._traceId,
      span_id: this._spanId,
    };
    if (this._parentId) event.parent_id = this._parentId;
    if (this._service) event.service = this._service;
    if (this._source) {
      event.source = this._source;
      if (this._lnum) event.lnum = this._lnum;
    }
    if (Object.keys(ctx).length > 0) event.ctx = ctx;
    if (Object.keys(this._data).length > 0) event.data = this._data;

    this.logger._emitRaw(event);
  }
}
