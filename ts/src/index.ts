// teolog — Cross-stack wide-event structured logging (TypeScript)

import {
  appendFileSync,
  existsSync,
  mkdirSync,
  renameSync,
  rmSync,
  statSync,
  writeFileSync,
} from "node:fs";
import { homedir } from "node:os";
import { basename, dirname, join } from "node:path";

export type Level = "trace" | "debug" | "info" | "warn" | "error";

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

type SpanRun<T> = (span: SpanBuilder) => T | Promise<T>;
type SpanArgs<T> =
  | [SpanRun<T>]
  | [Record<string, unknown> | undefined, SpanRun<T>];

interface EventBaseInput {
  level: Level;
  msg: string;
  project?: string;
  traceId: string;
  spanId: string;
  service?: string;
}

export function applyOptionalEventFields(
  event: WideEvent,
  ctx?: Record<string, unknown>,
  data?: Record<string, unknown>,
  err?: WideEvent["err"],
): WideEvent {
  if (ctx && Object.keys(ctx).length > 0) event.ctx = ctx;
  if (data && Object.keys(data).length > 0) event.data = data;
  if (err) event.err = err;
  return event;
}

function buildEventBase(input: EventBaseInput): WideEvent {
  const event: WideEvent = {
    ts: new Date().toISOString(),
    lvl: input.level,
    msg: input.msg,
    project: input.project,
    trace_id: input.traceId,
    span_id: input.spanId,
  };
  if (input.service) event.service = input.service;
  return event;
}

function resolveSpanArgs<T>(
  args: SpanArgs<T>,
  missingCallbackMessage: string,
): { context?: Record<string, unknown>; run: SpanRun<T> } {
  const [first, second] = args;
  const context = typeof first === "function" ? undefined : first;
  const run = typeof first === "function" ? first : second;
  if (!run) throw new Error(missingCallbackMessage);
  return { context, run };
}

function sourceFields(): Pick<SpanOptions, "source" | "lnum"> {
  const loc = captureSource();
  return { source: loc?.source, lnum: loc?.lnum };
}

async function runWithSpan<T>(
  span: SpanBuilder,
  run: SpanRun<T>,
  context?: Record<string, unknown>,
): Promise<T> {
  if (context) {
    for (const [key, value] of Object.entries(context)) {
      if (value !== undefined) span.set(key, value);
    }
  }

  try {
    const result = await run(span);
    span.finish();
    return result;
  } catch (error) {
    span.err(error).finish();
    throw error;
  }
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
  let s = "";
  for (const b of bytes) {
    s += b.toString(16).padStart(2, "0");
  }
  return s;
}

// --- Source Location ---

/** Parse caller file:line from Error.stack (best-effort). */
const LOG_PATHS = process.env.TEOLOG_LOG_PATHS === "1";
const LOG_STACKS = process.env.TEOLOG_LOG_STACKS === "1";

function sanitizeSource(source: string): string {
  return LOG_PATHS ? source : basename(source);
}

function captureSource(): { source: string; lnum: number } | undefined {
  const err = new Error();
  if (!err.stack) return undefined;
  const lines = err.stack.split("\n");
  for (let i = 2; i < lines.length; i++) {
    const line = lines[i];
    const match =
      line.match(/\((.+):(\d+):\d+\)/) ?? line.match(/at\s+(.+):(\d+):\d+/);
    if (!match) continue;

    const source = match[1];
    if (
      source.includes("/teolog/ts/src/") ||
      source.includes("/teolog/ts/dist/") ||
      source.endsWith("/src/log.ts") ||
      source.endsWith("\\src\\log.ts")
    ) {
      continue;
    }

    return { source: sanitizeSource(source), lnum: parseInt(match[2], 10) };
  }
  return undefined;
}

// --- Sink ---

export interface Sink {
  emit(event: WideEvent): void;
}

export interface LoggerOptions {
  service?: string;
  traceId?: string;
}

export function resolveXdgStatePath(
  appName: string,
  ...parts: string[]
): string {
  const xdgStateHome = process.env.XDG_STATE_HOME;
  const stateRoot =
    xdgStateHome && xdgStateHome.length > 0
      ? xdgStateHome
      : join(homedir(), ".local", "state");
  return join(stateRoot, appName, ...parts);
}

/** Append NDJSON events to file. Creates parent dir on first write. */
export class FileSink implements Sink {
  private filePath: string;

  private constructor(filePath: string) {
    this.filePath = filePath;
  }

  static open(filePath: string): FileSink {
    mkdirSync(dirname(filePath), { recursive: true });
    return new FileSink(filePath);
  }

  emit(event: WideEvent): void {
    appendFileSync(this.filePath, `${JSON.stringify(event)}\n`, "utf8");
  }
}

export interface RotationOptions {
  maxBytes?: number;
  maxFiles?: number;
}

const DEFAULT_ROTATION_MAX_BYTES = 10 * 1024 * 1024;
const DEFAULT_ROTATION_MAX_FILES = 5;

function archivePath(filePath: string, index: number): string {
  return `${filePath}.${index}`;
}

/** Append NDJSON to file with size-based rotation. */
export class RotatingFileSink implements Sink {
  private filePath: string;
  private maxBytes: number;
  private maxFiles: number;

  private constructor(filePath: string, options: Required<RotationOptions>) {
    this.filePath = filePath;
    this.maxBytes = options.maxBytes;
    this.maxFiles = options.maxFiles;
  }

  static open(
    filePath: string,
    options: RotationOptions = {},
  ): RotatingFileSink {
    mkdirSync(dirname(filePath), { recursive: true });
    return new RotatingFileSink(filePath, {
      maxBytes: options.maxBytes ?? DEFAULT_ROTATION_MAX_BYTES,
      maxFiles: options.maxFiles ?? DEFAULT_ROTATION_MAX_FILES,
    });
  }

  emit(event: WideEvent): void {
    const line = `${JSON.stringify(event)}\n`;
    const lineBytes = Buffer.byteLength(line, "utf8");
    const currentSize = existsSync(this.filePath)
      ? statSync(this.filePath).size
      : 0;

    if (
      this.maxBytes > 0 &&
      currentSize > 0 &&
      currentSize + lineBytes > this.maxBytes
    ) {
      this.rotate();
    }

    appendFileSync(this.filePath, line, "utf8");
  }

  private rotate(): void {
    if (this.maxFiles === 0) {
      writeFileSync(this.filePath, "", "utf8");
      return;
    }

    const oldest = archivePath(this.filePath, this.maxFiles);
    if (existsSync(oldest)) rmSync(oldest);

    for (let index = this.maxFiles - 1; index >= 1; index -= 1) {
      const src = archivePath(this.filePath, index);
      const dst = archivePath(this.filePath, index + 1);
      if (existsSync(src)) renameSync(src, dst);
    }

    if (existsSync(this.filePath))
      renameSync(this.filePath, archivePath(this.filePath, 1));
    writeFileSync(this.filePath, "", "utf8");
  }
}

/** Emit to console with level-appropriate method. */
export class ConsoleSink implements Sink {
  emit(event: WideEvent): void {
    const json = JSON.stringify(event);
    switch (event.lvl) {
      case "error":
        console.error(json);
        break;
      case "warn":
        console.warn(json);
        break;
      case "debug":
        console.debug(json);
        break;
      case "trace":
        console.debug(json);
        break;
      default:
        console.log(json);
    }
  }
}

/** Emit via a callback (for Tauri plugin-log bridge, HTTP, etc). */
export class CallbackSink implements Sink {
  constructor(
    private cb: (level: Level, json: string) => void | Promise<void>,
  ) {}
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
    const body = batch.join("\n");

    fetch(this.url, {
      method: "POST",
      headers: {
        "Content-Type": "application/x-ndjson",
        ...this.headers,
      },
      body,
    }).catch((err) => {
      console.error("[teolog] HttpSink flush error:", err);
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

function isLoggerOptions(
  value: LogContext | LoggerOptions | undefined,
): value is LoggerOptions {
  return value !== undefined && !(value instanceof LogContext);
}

export class Logger {
  private project: string;
  private service?: string;
  private traceId?: string;
  private sink: Sink;
  private context: LogContext;

  constructor(project: string, sink?: Sink, options?: LoggerOptions);
  constructor(
    project: string,
    sink: Sink | undefined,
    context: LogContext,
    options?: LoggerOptions,
  );
  constructor(
    project: string,
    sink?: Sink,
    contextOrOptions?: LogContext | LoggerOptions,
    maybeOptions?: LoggerOptions,
  ) {
    const options = isLoggerOptions(contextOrOptions)
      ? contextOrOptions
      : maybeOptions;

    this.project = project;
    this.sink = sink ?? new ConsoleSink();
    this.context =
      contextOrOptions instanceof LogContext
        ? contextOrOptions
        : new LogContext();
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
    return new SpanBuilder(this, name, this.context.child(), {
      traceId: this.resolveTraceId(),
      service: this.service,
      ...sourceFields(),
    });
  }

  async withSpan<T>(name: string, ...args: SpanArgs<T>): Promise<T> {
    const parsed = resolveSpanArgs(args, "logger.withSpan requires callback");
    return runWithSpan(this.span(name), parsed.run, parsed.context);
  }

  private buildEvent(
    level: Level,
    msg: string,
    data?: Record<string, unknown>,
    err?: WideEvent["err"],
  ): WideEvent {
    const loc = captureSource();
    const event = buildEventBase({
      level,
      msg,
      project: this.project,
      traceId: this.resolveTraceId(),
      spanId: genSpanId(),
      service: this.service,
    });
    if (loc) {
      event.source = loc.source;
      event.lnum = loc.lnum;
    }
    return applyOptionalEventFields(event, this.context.snapshot(), data, err);
  }

  /** Emit a one-shot event. */
  emit(level: Level, msg: string, data?: Record<string, unknown>): void {
    this.sink.emit(this.buildEvent(level, msg, data));
  }

  info(msg: string, data?: Record<string, unknown>) {
    this.emit("info", msg, data);
  }
  warn(msg: string, data?: Record<string, unknown>) {
    this.emit("warn", msg, data);
  }
  error(msg: string, data?: Record<string, unknown>) {
    this.emit("error", msg, data);
  }
  debug(msg: string, data?: Record<string, unknown>) {
    this.emit("debug", msg, data);
  }
  trace(msg: string, data?: Record<string, unknown>) {
    this.emit("trace", msg, data);
  }

  /** Emit with an error object. */
  errorWith(msg: string, err: unknown, data?: Record<string, unknown>): void {
    const errDetails =
      err instanceof Error
        ? {
            message: err.message,
            kind: err.name,
            ...(LOG_STACKS && err.stack ? { stack: err.stack } : {}),
          }
        : { message: String(err) };
    this.sink.emit(this.buildEvent("error", msg, data, errDetails));
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
  private _err?: WideEvent["err"];
  private started: number;
  private _traceId: string;
  private _spanId: string;
  private _parentId?: string;
  private _service?: string;
  private _source?: string;
  private _lnum?: number;

  constructor(
    logger: Logger,
    name: string,
    context: LogContext,
    opts: SpanOptions,
  ) {
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
  get spanId(): string {
    return this._spanId;
  }

  /** Get this span's trace ID. */
  get traceId(): string {
    return this._traceId;
  }

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
    this._err =
      err instanceof Error
        ? {
            message: err.message,
            kind: err.name,
            ...(LOG_STACKS && err.stack ? { stack: err.stack } : {}),
          }
        : { message: String(err) };
    return this;
  }

  /** Create a child span that shares this span's trace_id. */
  span(name: string): SpanBuilder {
    return new SpanBuilder(this.logger, name, this.context.child(), {
      traceId: this._traceId,
      parentId: this._spanId,
      service: this._service,
      ...sourceFields(),
    });
  }

  async withSpan<T>(name: string, ...args: SpanArgs<T>): Promise<T> {
    const parsed = resolveSpanArgs(args, "span.withSpan requires callback");
    return runWithSpan(this.span(name), parsed.run, parsed.context);
  }

  /** Emit the span event. */
  finish(): void {
    const dur_ms = Math.round(performance.now() - this.started);
    const hasError = this._err !== undefined;
    const ctx = this.context.snapshot();

    const event: WideEvent = {
      ts: new Date().toISOString(),
      lvl: hasError ? "error" : "info",
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
    if (this._err) event.err = this._err;

    this.logger._emitRaw(event);
  }
}
