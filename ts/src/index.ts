// teolog — Cross-stack wide-event structured logging (TypeScript)

export type Level = 'trace' | 'debug' | 'info' | 'warn' | 'error';

/** Matches schema/wide-event.schema.json */
export interface WideEvent {
  ts: string;
  lvl: Level;
  msg: string;
  span?: string;
  dur_ms?: number;
  project?: string;
  module?: string;
  ctx?: Record<string, unknown>;
  data?: Record<string, unknown>;
  err?: { message: string; kind?: string; stack?: string };
  parent?: string;
  [key: string]: unknown;
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
  private sink: Sink;
  private context: LogContext;

  constructor(project: string, sink?: Sink, context?: LogContext) {
    this.project = project;
    this.sink = sink ?? new ConsoleSink();
    this.context = context ?? new LogContext();
  }

  /** Set root context fields. */
  setContext(fields: Record<string, unknown>): void {
    this.context.merge(fields);
  }

  /** Start a span (returns a builder). Call .finish() or let it go out of scope. */
  span(name: string): SpanBuilder {
    return new SpanBuilder(this, name, this.context.child());
  }

  /** Emit a one-shot event. */
  emit(level: Level, msg: string, data?: Record<string, unknown>): void {
    const event: WideEvent = {
      ts: new Date().toISOString(),
      lvl: level,
      msg,
      project: this.project,
    };
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

export class SpanBuilder {
  private logger: Logger;
  private name: string;
  private context: LogContext;
  private data: Record<string, unknown> = {};
  private started: number;

  constructor(logger: Logger, name: string, context: LogContext) {
    this.logger = logger;
    this.name = name;
    this.context = context;
    this.started = performance.now();
  }

  /** Add to span context. */
  set(key: string, value: unknown): this {
    this.context.set(key, value);
    return this;
  }

  /** Add to span data payload. */
  addData(key: string, value: unknown): this {
    this.data[key] = value;
    return this;
  }

  /** Record an error. */
  err(err: unknown): this {
    this.data['error'] = err instanceof Error
      ? { message: err.message, kind: err.name, stack: err.stack }
      : { message: String(err) };
    return this;
  }

  /** Emit the span event. */
  finish(): void {
    const dur_ms = Math.round(performance.now() - this.started);
    const hasError = 'error' in this.data;
    const ctx = this.context.snapshot();

    const event: WideEvent = {
      ts: new Date().toISOString(),
      lvl: hasError ? 'error' : 'info',
      msg: this.name,
      span: this.name,
      dur_ms,
    };
    if (Object.keys(ctx).length > 0) event.ctx = ctx;
    if (Object.keys(this.data).length > 0) event.data = this.data;

    this.logger._emitRaw(event);
  }
}
