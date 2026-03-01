// teolog/svelte — Svelte 5 runes integration
//
// Re-exports core types and adds a reactive LogContext using $state.
// Usage:
//   import { logContext, logger } from 'teolog/svelte';
//   logContext.update({ view: 'Editor' });
//   logger.info('Mounted', { duration: 42 });

export { Logger, ConsoleSink, CallbackSink, MultiSink, SpanBuilder } from './index.js';
export type { Level, WideEvent, Sink } from './index.js';

import { Logger, ConsoleSink } from './index.js';
import type { Level, WideEvent, Sink } from './index.js';

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

/**
 * Create a logger that automatically merges the reactive logContext
 * into every event's ctx field.
 */
export function createLogger(project: string, sink?: Sink): ReactiveLogger {
  return new ReactiveLogger(project, sink ?? new ConsoleSink());
}

class ReactiveLogger {
  private project: string;
  private sink: Sink;

  constructor(project: string, sink: Sink) {
    this.project = project;
    this.sink = sink;
  }

  private build(level: Level, msg: string, data?: Record<string, unknown>): WideEvent {
    const event: WideEvent = {
      ts: new Date().toISOString(),
      lvl: level,
      msg,
      project: this.project,
    };
    const ctx = logContext.snapshot();
    if (Object.keys(ctx).length > 0) event.ctx = ctx;
    if (data && Object.keys(data).length > 0) event.data = data;
    return event;
  }

  info(msg: string, data?: Record<string, unknown>)  { this.sink.emit(this.build('info', msg, data)); }
  warn(msg: string, data?: Record<string, unknown>)  { this.sink.emit(this.build('warn', msg, data)); }
  error(msg: string, data?: Record<string, unknown>) { this.sink.emit(this.build('error', msg, data)); }
  debug(msg: string, data?: Record<string, unknown>) { this.sink.emit(this.build('debug', msg, data)); }
  trace(msg: string, data?: Record<string, unknown>) { this.sink.emit(this.build('trace', msg, data)); }

  errorWith(msg: string, err: unknown, data?: Record<string, unknown>): void {
    const errDetails = err instanceof Error
      ? { message: err.message, kind: err.name, stack: err.stack }
      : { message: String(err) };
    this.sink.emit(this.build('error', msg, { ...data, error: errDetails }));
  }
}
