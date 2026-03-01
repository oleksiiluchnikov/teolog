use crate::context::LogContext;
use serde::Serialize;

/// Log level matching the shared schema.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Level {
    Trace,
    Debug,
    Info,
    Warn,
    Error,
}

impl std::fmt::Display for Level {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Trace => write!(f, "trace"),
            Self::Debug => write!(f, "debug"),
            Self::Info => write!(f, "info"),
            Self::Warn => write!(f, "warn"),
            Self::Error => write!(f, "error"),
        }
    }
}

/// A single wide event — the core data structure.
/// Matches `schema/wide-event.schema.json`.
#[derive(Debug, Clone, Serialize)]
pub struct WideEvent {
    pub ts: String,
    pub lvl: Level,
    pub msg: String,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub span: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub dur_ms: Option<u64>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub project: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub module: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub ctx: Option<serde_json::Value>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<serde_json::Value>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub err: Option<serde_json::Value>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub parent: Option<String>,
}

impl WideEvent {
    pub fn new(level: Level, msg: &str) -> Self {
        Self {
            ts: chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true),
            lvl: level,
            msg: msg.to_string(),
            span: None,
            dur_ms: None,
            project: None,
            module: None,
            ctx: None,
            data: None,
            err: None,
            parent: None,
        }
    }

    pub fn with_project(mut self, project: &str) -> Self {
        self.project = Some(project.to_string());
        self
    }

    pub fn with_module(mut self, module: &str) -> Self {
        self.module = Some(module.to_string());
        self
    }

    pub fn with_span(mut self, span: &str) -> Self {
        self.span = Some(span.to_string());
        self
    }

    pub fn with_dur_ms(mut self, dur: u64) -> Self {
        self.dur_ms = Some(dur);
        self
    }

    pub fn with_ctx(mut self, ctx: &LogContext) -> Self {
        let val = serde_json::to_value(ctx).unwrap_or_default();
        if val != serde_json::Value::Object(serde_json::Map::new()) {
            self.ctx = Some(val);
        }
        self
    }

    pub fn with_data(mut self, data: serde_json::Value) -> Self {
        self.data = Some(data);
        self
    }

    pub fn with_parent(mut self, parent: &str) -> Self {
        self.parent = Some(parent.to_string());
        self
    }

    /// Serialize to a single NDJSON line (no trailing newline).
    pub fn to_json(&self) -> String {
        serde_json::to_string(self).unwrap_or_else(|_| "{}".to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn event_serializes_to_json() {
        let event = WideEvent::new(Level::Info, "test message")
            .with_project("teolog")
            .with_span("http_request")
            .with_dur_ms(42);

        let json = event.to_json();
        assert!(json.contains("\"lvl\":\"info\""));
        assert!(json.contains("\"msg\":\"test message\""));
        assert!(json.contains("\"dur_ms\":42"));
        assert!(json.contains("\"project\":\"teolog\""));
    }

    #[test]
    fn omits_none_fields() {
        let event = WideEvent::new(Level::Debug, "minimal");
        let json = event.to_json();
        assert!(!json.contains("span"));
        assert!(!json.contains("dur_ms"));
        assert!(!json.contains("ctx"));
    }
}
