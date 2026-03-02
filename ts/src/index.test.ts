import { test, expect, describe } from 'bun:test';
import { Logger, SpanBuilder, CallbackSink, genTraceId, genSpanId, HttpSink } from './index.js';
import type { WideEvent, Sink } from './index.js';

/** Collect events for assertions. */
class CollectSink implements Sink {
  events: WideEvent[] = [];
  emit(event: WideEvent): void {
    this.events.push(structuredClone(event));
  }
}

describe('ID generation', () => {
  test('genTraceId produces 32 hex chars', () => {
    const id = genTraceId();
    expect(id).toHaveLength(32);
    expect(id).toMatch(/^[0-9a-f]{32}$/);
  });

  test('genSpanId produces 16 hex chars', () => {
    const id = genSpanId();
    expect(id).toHaveLength(16);
    expect(id).toMatch(/^[0-9a-f]{16}$/);
  });

  test('IDs are unique', () => {
    const a = genTraceId();
    const b = genTraceId();
    expect(a).not.toBe(b);
  });
});

describe('Logger', () => {
  test('one-shot events have trace_id and span_id', () => {
    const sink = new CollectSink();
    const log = new Logger('test', sink);
    log.info('hello');

    expect(sink.events).toHaveLength(1);
    const e = sink.events[0];
    expect(e.trace_id).toHaveLength(32);
    expect(e.span_id).toHaveLength(16);
    expect(e.project).toBe('test');
    expect(e.msg).toBe('hello');
  });

  test('service propagates to events', () => {
    const sink = new CollectSink();
    const log = new Logger('test', sink, undefined, { service: 'my-svc' });
    log.info('hello');

    expect(sink.events[0].service).toBe('my-svc');
  });

  test('shared trace_id propagates', () => {
    const sink = new CollectSink();
    const tid = 'aaaa1111bbbb2222cccc3333dddd4444';
    const log = new Logger('test', sink, undefined, { traceId: tid });
    log.info('first');
    log.info('second');

    expect(sink.events[0].trace_id).toBe(tid);
    expect(sink.events[1].trace_id).toBe(tid);
    // But different span_ids
    expect(sink.events[0].span_id).not.toBe(sink.events[1].span_id);
  });
});

describe('SpanBuilder', () => {
  test('span emits trace_id and span_id', () => {
    const sink = new CollectSink();
    const log = new Logger('test', sink);
    const span = log.span('my_op');
    span.set('key', 'val');
    span.finish();

    expect(sink.events).toHaveLength(1);
    const e = sink.events[0];
    expect(e.trace_id).toHaveLength(32);
    expect(e.span_id).toHaveLength(16);
    expect(e.span).toBe('my_op');
    expect(e.dur_ms).toBeGreaterThanOrEqual(0);
  });

  test('child spans share trace_id and set parent_id', () => {
    const sink = new CollectSink();
    const log = new Logger('test', sink, undefined, { service: 'test-svc' });

    const parent = log.span('parent_op');
    parent.set('user_id', 'abc');
    const child = parent.span('child_op');
    child.addData('step', 'validation');
    child.finish();
    parent.finish();

    expect(sink.events).toHaveLength(2);
    const [childEvt, parentEvt] = sink.events;

    // Same trace_id
    expect(childEvt.trace_id).toBe(parentEvt.trace_id);
    // Child's parent_id == parent's span_id
    expect(childEvt.parent_id).toBe(parentEvt.span_id);
    // Different span_ids
    expect(childEvt.span_id).not.toBe(parentEvt.span_id);
    // Service propagated
    expect(childEvt.service).toBe('test-svc');
    // Context inherited
    expect(childEvt.ctx?.['user_id']).toBe('abc');
  });

  test('spanId and traceId accessors', () => {
    const sink = new CollectSink();
    const log = new Logger('test', sink);
    const span = log.span('op');

    expect(span.spanId).toHaveLength(16);
    expect(span.traceId).toHaveLength(32);
    span.finish();
  });
});

describe('HttpSink', () => {
  test('batches events and flushes', () => {
    // We can't test actual HTTP, but we can test the queue logic
    const sink = new HttpSink('http://localhost:9999', {}, 3, 0);
    const mockEvent: WideEvent = { ts: '', lvl: 'info', msg: 'test' };

    // Queue 2 events — should not flush yet
    sink.emit(mockEvent);
    sink.emit(mockEvent);

    // Queue 3rd — should trigger flush (will fail silently due to no server)
    sink.emit(mockEvent);

    // After flush, dispose should not error
    sink.dispose();
  });
});
