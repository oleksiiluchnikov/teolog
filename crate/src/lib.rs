//! teolog — Cross-stack wide-event structured logging.
//!
//! One NDJSON format shared across Rust, TypeScript, and Lua.
//! Build context throughout a request lifecycle, emit once.

mod context;
mod event;
mod macros;
mod sink;

pub use context::LogContext;
pub use event::{Level, WideEvent};
pub use macros::{is_verbose, set_verbose};
pub use sink::{FileSink, MultiSink, Sink, StderrSink, StdoutSink, WriterSink};

/// Initialize teolog with a project name and default sink (stdout).
pub fn init(project: &str) -> Logger {
    Logger {
        project: project.to_string(),
        sink: Box::new(StdoutSink),
        context: LogContext::new(),
    }
}

/// The main logger. Holds project name, sink, and root context.
pub struct Logger {
    project: String,
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

    /// Start a new span (returns a builder that emits on drop).
    pub fn span(&self, name: &str) -> SpanBuilder<'_> {
        SpanBuilder {
            logger: self,
            name: name.to_string(),
            context: self.context.clone(),
            data: serde_json::Map::new(),
            started: std::time::Instant::now(),
        }
    }

    /// Emit a one-shot event (no span/duration).
    pub fn emit(&self, level: Level, msg: &str) {
        let event = WideEvent::new(level, msg)
            .with_project(&self.project)
            .with_ctx(&self.context);
        self.sink.emit(&event);
    }

    /// Emit with data.
    pub fn emit_with(&self, level: Level, msg: &str, data: serde_json::Value) {
        let event = WideEvent::new(level, msg)
            .with_project(&self.project)
            .with_ctx(&self.context)
            .with_data(data);
        self.sink.emit(&event);
    }

    pub fn info(&self, msg: &str) {
        self.emit(Level::Info, msg);
    }
    pub fn warn(&self, msg: &str) {
        self.emit(Level::Warn, msg);
    }
    pub fn error(&self, msg: &str) {
        self.emit(Level::Error, msg);
    }
    pub fn debug(&self, msg: &str) {
        self.emit(Level::Debug, msg);
    }
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
            .with_ctx(&self.context)
            .with_dur_ms(dur.as_millis() as u64);

        if !self.data.is_empty() {
            event = event.with_data(serde_json::Value::Object(self.data.clone()));
        }

        self.logger.sink.emit(&event);
    }
}
