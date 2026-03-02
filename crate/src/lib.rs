//! teolog — Cross-stack wide-event structured logging.
//!
//! One NDJSON format shared across Rust, TypeScript, and Lua.
//! Build context throughout a request lifecycle, emit once.

mod context;
mod event;
pub mod id;
mod macros;
mod sink;

pub use context::LogContext;
pub use event::{Level, WideEvent};
pub use id::{gen_span_id, gen_trace_id};
pub use macros::{is_verbose, set_verbose};
pub use sink::{FileSink, MultiSink, Sink, StderrSink, StdoutSink, WriterSink};

#[cfg(feature = "http-sink")]
pub use sink::HttpSink;

/// Initialize teolog with a project name and default sink (stdout).
pub fn init(project: &str) -> Logger {
    Logger {
        project: project.to_string(),
        service: None,
        trace_id: None,
        sink: Box::new(StdoutSink),
        context: LogContext::new(),
    }
}

/// The main logger. Holds project name, sink, and root context.
pub struct Logger {
    project: String,
    service: Option<String>,
    /// If set, all events from this logger share this trace_id.
    /// If None, each event/span gets a fresh trace_id.
    trace_id: Option<String>,
    sink: Box<dyn Sink>,
    context: LogContext,
}

impl Logger {
    /// Set the sink (stdout, file, writer).
    pub fn with_sink(mut self, sink: impl Sink + 'static) -> Self {
        self.sink = Box::new(sink);
        self
    }

    /// Set root context fields that appear on every event.
    pub fn with_context(mut self, ctx: LogContext) -> Self {
        self.context = ctx;
        self
    }

    /// Set the service name (e.g. "tauri-backend", "neovim").
    pub fn with_service(mut self, service: &str) -> Self {
        self.service = Some(service.to_string());
        self
    }

    /// Set a shared trace_id for all events from this logger.
    /// Use this when continuing a trace from another service.
    pub fn with_trace_id(mut self, id: &str) -> Self {
        self.trace_id = Some(id.to_string());
        self
    }

    /// Get the current trace_id (or None if each event generates its own).
    pub fn trace_id(&self) -> Option<&str> {
        self.trace_id.as_deref()
    }

    /// Get the project name.
    pub fn project(&self) -> &str {
        &self.project
    }

    fn resolve_trace_id(&self) -> String {
        self.trace_id.clone().unwrap_or_else(id::gen_trace_id)
    }

    /// Start a new span (returns a builder that emits on drop).
    #[track_caller]
    pub fn span(&self, name: &str) -> SpanBuilder<'_> {
        let caller = std::panic::Location::caller();
        SpanBuilder {
            logger: self,
            name: name.to_string(),
            context: self.context.clone(),
            data: serde_json::Map::new(),
            started: std::time::Instant::now(),
            trace_id: self.resolve_trace_id(),
            span_id: id::gen_span_id(),
            parent_id: None,
            source: Some(caller.file().to_string()),
            lnum: Some(caller.line()),
        }
    }

    /// Emit a one-shot event (no span/duration).
    #[track_caller]
    pub fn emit(&self, level: Level, msg: &str) {
        let caller = std::panic::Location::caller();
        let mut event = WideEvent::new(level, msg)
            .with_project(&self.project)
            .with_trace_id(&self.resolve_trace_id())
            .with_source(caller.file(), caller.line())
            .with_ctx(&self.context);
        if let Some(ref svc) = self.service {
            event = event.with_service(svc);
        }
        self.sink.emit(&event);
    }

    /// Emit with data.
    #[track_caller]
    pub fn emit_with(&self, level: Level, msg: &str, data: serde_json::Value) {
        let caller = std::panic::Location::caller();
        let mut event = WideEvent::new(level, msg)
            .with_project(&self.project)
            .with_trace_id(&self.resolve_trace_id())
            .with_source(caller.file(), caller.line())
            .with_ctx(&self.context)
            .with_data(data);
        if let Some(ref svc) = self.service {
            event = event.with_service(svc);
        }
        self.sink.emit(&event);
    }

    #[track_caller]
    pub fn info(&self, msg: &str) {
        self.emit(Level::Info, msg);
    }
    #[track_caller]
    pub fn warn(&self, msg: &str) {
        self.emit(Level::Warn, msg);
    }
    #[track_caller]
    pub fn error(&self, msg: &str) {
        self.emit(Level::Error, msg);
    }
    #[track_caller]
    pub fn debug(&self, msg: &str) {
        self.emit(Level::Debug, msg);
    }
    #[track_caller]
    pub fn trace(&self, msg: &str) {
        self.emit(Level::Trace, msg);
    }
}

/// A span that accumulates context and data, emitting one wide event on drop.
pub struct SpanBuilder<'a> {
    logger: &'a Logger,
    name: String,
    context: LogContext,
    data: serde_json::Map<String, serde_json::Value>,
    started: std::time::Instant,
    trace_id: String,
    span_id: String,
    parent_id: Option<String>,
    source: Option<String>,
    lnum: Option<u32>,
}

impl<'a> SpanBuilder<'a> {
    /// Add a key-value pair to this span's context.
    pub fn set(&mut self, key: &str, value: impl Into<serde_json::Value>) -> &mut Self {
        self.context.set(key, value);
        self
    }

    /// Add a key-value pair to this span's data payload.
    pub fn data(&mut self, key: &str, value: impl Into<serde_json::Value>) -> &mut Self {
        self.data.insert(key.to_string(), value.into());
        self
    }

    /// Record an error on this span.
    pub fn err(&mut self, err: &dyn std::error::Error) -> &mut Self {
        self.data.insert(
            "error".to_string(),
            serde_json::json!({
                "message": err.to_string(),
            }),
        );
        self
    }

    /// Record an error message string on this span.
    pub fn err_msg(&mut self, msg: &str) -> &mut Self {
        self.data.insert(
            "error".to_string(),
            serde_json::json!({
                "message": msg,
            }),
        );
        self
    }

    /// Get this span's ID (for passing to child spans or other services).
    pub fn span_id(&self) -> &str {
        &self.span_id
    }

    /// Get this span's trace ID.
    pub fn trace_id(&self) -> &str {
        &self.trace_id
    }

    /// Create a child span that shares this span's trace_id.
    /// The child's parent_id is set to this span's span_id.
    #[track_caller]
    pub fn span(&self, name: &str) -> SpanBuilder<'a> {
        let caller = std::panic::Location::caller();
        SpanBuilder {
            logger: self.logger,
            name: name.to_string(),
            context: self.context.child(),
            data: serde_json::Map::new(),
            started: std::time::Instant::now(),
            trace_id: self.trace_id.clone(),
            span_id: id::gen_span_id(),
            parent_id: Some(self.span_id.clone()),
            source: Some(caller.file().to_string()),
            lnum: Some(caller.line()),
        }
    }

    /// Emit the span event now (consumes the builder, skipping drop).
    pub fn finish(self) {
        // drop triggers emit
    }
}

impl Drop for SpanBuilder<'_> {
    fn drop(&mut self) {
        let dur = self.started.elapsed();
        let level = if self.data.contains_key("error") {
            Level::Error
        } else {
            Level::Info
        };

        let mut event = WideEvent::new(level, &self.name)
            .with_project(&self.logger.project)
            .with_span(&self.name)
            .with_trace_id(&self.trace_id)
            .with_span_id(&self.span_id)
            .with_ctx(&self.context)
            .with_dur_ms(dur.as_millis() as u64);

        if let Some(ref pid) = self.parent_id {
            event = event.with_parent_id(pid);
        }
        if let Some(ref svc) = self.logger.service {
            event = event.with_service(svc);
        }
        if let Some(ref src) = self.source {
            event = event.with_source(src, self.lnum.unwrap_or(0));
        }
        if !self.data.is_empty() {
            event = event.with_data(serde_json::Value::Object(self.data.clone()));
        }

        self.logger.sink.emit(&event);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, Mutex};

    /// Collects events for assertions.
    struct CollectSink(Arc<Mutex<Vec<String>>>);
    impl Sink for CollectSink {
        fn emit(&self, event: &WideEvent) {
            self.0.lock().unwrap().push(event.to_json());
        }
    }

    #[test]
    fn one_shot_events_have_trace_and_span_ids() {
        let events = Arc::new(Mutex::new(Vec::new()));
        let log = init("test").with_sink(CollectSink(events.clone()));
        log.info("hello");

        let json = &events.lock().unwrap()[0];
        let v: serde_json::Value = serde_json::from_str(json).unwrap();
        assert_eq!(v["trace_id"].as_str().unwrap().len(), 32);
        assert_eq!(v["span_id"].as_str().unwrap().len(), 16);
        assert!(v["source"].as_str().unwrap().contains("lib.rs"));
        assert!(v["lnum"].as_u64().is_some());
    }

    #[test]
    fn child_spans_share_trace_id() {
        let events = Arc::new(Mutex::new(Vec::new()));
        let log = init("test")
            .with_sink(CollectSink(events.clone()))
            .with_service("test-svc");

        {
            let mut parent = log.span("parent_op");
            parent.set("user_id", "abc");
            {
                let mut child = parent.span("child_op");
                child.data("step", "validation");
                child.finish();
            }
            parent.finish();
        }

        let evts = events.lock().unwrap();
        assert_eq!(evts.len(), 2);

        let child: serde_json::Value = serde_json::from_str(&evts[0]).unwrap();
        let parent: serde_json::Value = serde_json::from_str(&evts[1]).unwrap();

        // Same trace_id
        assert_eq!(child["trace_id"], parent["trace_id"]);
        // Child's parent_id == parent's span_id
        assert_eq!(child["parent_id"], parent["span_id"]);
        // Different span_ids
        assert_ne!(child["span_id"], parent["span_id"]);
        // Service propagated
        assert_eq!(child["service"].as_str().unwrap(), "test-svc");
        assert_eq!(parent["service"].as_str().unwrap(), "test-svc");
        // Context inherited
        assert_eq!(child["ctx"]["user_id"].as_str().unwrap(), "abc");
    }

    #[test]
    fn shared_trace_id_propagates() {
        let events = Arc::new(Mutex::new(Vec::new()));
        let log = init("test")
            .with_sink(CollectSink(events.clone()))
            .with_trace_id("aaaa1111bbbb2222cccc3333dddd4444");

        log.info("first");
        log.info("second");

        let evts = events.lock().unwrap();
        let e1: serde_json::Value = serde_json::from_str(&evts[0]).unwrap();
        let e2: serde_json::Value = serde_json::from_str(&evts[1]).unwrap();
        assert_eq!(
            e1["trace_id"].as_str().unwrap(),
            "aaaa1111bbbb2222cccc3333dddd4444"
        );
        assert_eq!(
            e2["trace_id"].as_str().unwrap(),
            "aaaa1111bbbb2222cccc3333dddd4444"
        );
        // But different span_ids
        assert_ne!(e1["span_id"], e2["span_id"]);
    }
}
