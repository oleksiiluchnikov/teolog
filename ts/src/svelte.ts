// teolog/svelte — Svelte 5 runes integration
//
// Re-exports core types and adds a reactive LogContext using $state.
// Usage:
//   import { logContext, createLogger } from 'teolog/svelte';
//   logContext.update({ view: 'Editor' });
//   const log = createLogger('kolo', consoleSink, { service: 'svelte-frontend' });
//   log.info('Mounted', { duration: 42 });

export {
  Logger,
  ConsoleSink,
  CallbackSink,
  MultiSink,
  HttpSink,
  FileSink,
  SpanBuilder,
  genTraceId,
  genSpanId,
  resolveXdgStatePath,
} from "./index.js";
export type { Level, WideEvent, Sink } from "./index.js";

import {
  Logger,
  ConsoleSink,
  genTraceId,
  genSpanId,
  applyOptionalEventFields,
} from "./index.js";
import type { Level, WideEvent, Sink } from "./index.js";

/** Reactive LogContext using Svelte 5 $state rune. */
class ReactiveLogContext {
  state = $state<Record<string, unknown>>({});

  update(fields: Record<string, unknown>): void {
    Object.assign(this.state, fields);
  }

  set(key: string, value: unknown): void {
    this.state[key] = value;
  }

  snapshot(): Record<string, unknown> {
    return $state.snapshot(this.state) as Record<string, unknown>;
  }

  reset(): void {
    for (const key of Object.keys(this.state)) {
      delete this.state[key];
    }
  }
}

/** Singleton reactive context — import and use across components. */
export const logContext = new ReactiveLogContext();

interface ReactiveLoggerOptions {
  service?: string;
  traceId?: string;
}

/**
 * Create a logger that automatically merges the reactive logContext
 * into every event's ctx field. Supports trace_id and service.
 */
export function createLogger(
  project: string,
  sink?: Sink,
  options?: ReactiveLoggerOptions,
): ReactiveLogger {
  return new ReactiveLogger(project, sink ?? new ConsoleSink(), options);
}

class ReactiveLogger {
  private project: string;
  private sink: Sink;
  private service?: string;
  private traceId?: string;

  constructor(project: string, sink: Sink, options?: ReactiveLoggerOptions) {
    this.project = project;
    this.sink = sink;
    this.service = options?.service;
    this.traceId = options?.traceId;
  }

  /** Set a shared trace_id for all events from this logger. */
  setTraceId(id: string): void {
    this.traceId = id;
  }

  private resolveTraceId(): string {
    return this.traceId ?? genTraceId();
  }

  private build(
    level: Level,
    msg: string,
    data?: Record<string, unknown>,
    err?: WideEvent["err"],
  ): WideEvent {
    const event: WideEvent = {
      ts: new Date().toISOString(),
      lvl: level,
      msg,
      project: this.project,
      trace_id: this.resolveTraceId(),
      span_id: genSpanId(),
    };
    if (this.service) event.service = this.service;
    return applyOptionalEventFields(event, logContext.snapshot(), data, err);
  }

  info(msg: string, data?: Record<string, unknown>) {
    this.sink.emit(this.build("info", msg, data));
  }
  warn(msg: string, data?: Record<string, unknown>) {
    this.sink.emit(this.build("warn", msg, data));
  }
  error(msg: string, data?: Record<string, unknown>) {
    this.sink.emit(this.build("error", msg, data));
  }
  debug(msg: string, data?: Record<string, unknown>) {
    this.sink.emit(this.build("debug", msg, data));
  }
  trace(msg: string, data?: Record<string, unknown>) {
    this.sink.emit(this.build("trace", msg, data));
  }

  errorWith(msg: string, err: unknown, data?: Record<string, unknown>): void {
    const errDetails =
      err instanceof Error
        ? {
            message: err.message,
            kind: err.name,
            ...(process.env.TEOLOG_LOG_STACKS === "1" && err.stack
              ? { stack: err.stack }
              : {}),
          }
        : { message: String(err) };
    this.sink.emit(this.build("error", msg, data, errDetails));
  }
}
