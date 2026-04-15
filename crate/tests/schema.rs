use std::sync::{Arc, Mutex};

use teolog::{init, Sink, WideEvent};

struct CollectSink(Arc<Mutex<Vec<String>>>);

impl Sink for CollectSink {
    fn emit(&self, event: &WideEvent) {
        self.0.lock().unwrap().push(event.to_json());
    }
}

fn assert_schema_valid(event: &serde_json::Value) {
    let schema: serde_json::Value = serde_json::from_str(include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../schema/wide-event.schema.json"
    )))
    .unwrap();
    let compiled = jsonschema::JSONSchema::compile(&schema).unwrap();

    match compiled.validate(event) {
        Ok(()) => {}
        Err(errors) => {
            let joined = errors
                .map(|err| err.to_string())
                .collect::<Vec<_>>()
                .join("\n");
            panic!("schema validation failed:\n{joined}\ninstance: {event}");
        }
    };
}

#[test]
fn one_shot_event_matches_schema() {
    let events = Arc::new(Mutex::new(Vec::new()));
    let log = init("schema-test").with_sink(CollectSink(events.clone()));

    log.info("hello");

    let raw = events.lock().unwrap()[0].clone();
    let event: serde_json::Value = serde_json::from_str(&raw).unwrap();
    assert_schema_valid(&event);
}

#[test]
fn error_span_matches_schema_and_uses_top_level_err() {
    let events = Arc::new(Mutex::new(Vec::new()));
    let log = init("schema-test").with_sink(CollectSink(events.clone()));

    let mut span = log.span("failing_op");
    span.data("step", "parse");
    span.err_msg("boom");
    span.finish();

    let raw = events.lock().unwrap()[0].clone();
    let event: serde_json::Value = serde_json::from_str(&raw).unwrap();

    assert_eq!(event["lvl"], "error");
    assert_eq!(event["err"]["message"], "boom");
    assert!(event["data"]["step"].is_string());
    assert!(event["data"]["error"].is_null());
    assert_schema_valid(&event);
}
