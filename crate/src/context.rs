use serde::Serialize;
use std::collections::BTreeMap;

/// Dynamic key-value context that accumulates throughout a span's lifecycle.
/// Uses BTreeMap for deterministic serialization order.
#[derive(Debug, Clone, Default, Serialize)]
pub struct LogContext {
    #[serde(flatten)]
    fields: BTreeMap<String, serde_json::Value>,
}

impl LogContext {
    pub fn new() -> Self {
        Self::default()
    }

    /// Set a key-value pair. Overwrites if key exists.
    pub fn set(&mut self, key: &str, value: impl Into<serde_json::Value>) -> &mut Self {
        self.fields.insert(key.to_string(), value.into());
        self
    }

    /// Builder-style set (takes ownership, returns self).
    pub fn with(mut self, key: &str, value: impl Into<serde_json::Value>) -> Self {
        self.fields.insert(key.to_string(), value.into());
        self
    }

    /// Merge another context into this one (other's values win on conflict).
    pub fn merge(&mut self, other: &LogContext) {
        for (k, v) in &other.fields {
            self.fields.insert(k.clone(), v.clone());
        }
    }

    /// Get a value by key.
    pub fn get(&self, key: &str) -> Option<&serde_json::Value> {
        self.fields.get(key)
    }

    /// Remove a key.
    pub fn remove(&mut self, key: &str) -> Option<serde_json::Value> {
        self.fields.remove(key)
    }

    /// Check if empty.
    pub fn is_empty(&self) -> bool {
        self.fields.is_empty()
    }

    /// Number of fields.
    pub fn len(&self) -> usize {
        self.fields.len()
    }

    /// Create a child context (clone + additional fields).
    pub fn child(&self) -> Self {
        self.clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builder_pattern() {
        let ctx = LogContext::new()
            .with("user_id", "abc123")
            .with("tier", "premium")
            .with("item_count", 5);

        assert_eq!(ctx.len(), 3);
        assert_eq!(ctx.get("user_id"), Some(&serde_json::json!("abc123")));
    }

    #[test]
    fn merge_overwrites() {
        let mut base = LogContext::new().with("a", 1).with("b", 2);
        let overlay = LogContext::new().with("b", 99).with("c", 3);
        base.merge(&overlay);

        assert_eq!(base.get("b"), Some(&serde_json::json!(99)));
        assert_eq!(base.len(), 3);
    }

    #[test]
    fn child_inherits() {
        let parent = LogContext::new().with("project", "kolo");
        let mut child = parent.child();
        child.set("view", "editor");

        assert_eq!(child.get("project"), Some(&serde_json::json!("kolo")));
        assert_eq!(child.get("view"), Some(&serde_json::json!("editor")));
        // parent unchanged
        assert!(parent.get("view").is_none());
    }

    #[test]
    fn serializes_flat() {
        let ctx = LogContext::new().with("user_id", "abc").with("count", 5);
        let json = serde_json::to_string(&ctx).unwrap();
        // BTreeMap = alphabetical
        assert!(json.contains("\"count\":5"));
        assert!(json.contains("\"user_id\":\"abc\""));
    }
}
