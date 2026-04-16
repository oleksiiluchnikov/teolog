use std::fs;
use std::process::Command;

fn sample_log() -> String {
    [
        r#"{"ts":"2026-04-12T17:00:00Z","lvl":"info","msg":"menu_loaded","trace_id":"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa","span_id":"1111111111111111","span":"menu","dur_ms":42,"project":"demo","service":"api","data":{"count":5,"source":"disk"}}"#,
        r#"{"ts":"2026-04-12T17:00:01Z","lvl":"error","msg":"menu_failed","trace_id":"bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb","span_id":"2222222222222222","span":"menu","dur_ms":250,"project":"demo","service":"api","err":{"message":"boom"},"data":{"step":"parse"}}"#,
    ]
    .join("\n")
}

#[test]
fn status_outputs_json() {
    let dir = tempfile::tempdir().unwrap();
    let log_path = dir.path().join("teolog.ndjson");
    fs::write(&log_path, sample_log()).unwrap();

    let output = Command::new(env!("CARGO_BIN_EXE_teolog"))
        .args(["status", log_path.to_str().unwrap(), "--json"])
        .output()
        .unwrap();

    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(stdout.contains("\"events\": 2"));
    assert!(stdout.contains("\"unique_traces\": 2"));
}

#[test]
fn summary_includes_recent_errors() {
    let dir = tempfile::tempdir().unwrap();
    let log_path = dir.path().join("teolog.ndjson");
    fs::write(&log_path, sample_log()).unwrap();

    let output = Command::new(env!("CARGO_BIN_EXE_teolog"))
        .args([
            "summary",
            log_path.to_str().unwrap(),
            "--json",
            "--limit",
            "3",
        ])
        .output()
        .unwrap();

    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(stdout.contains("recent_errors"));
    assert!(stdout.contains("menu_failed"));
    assert!(stdout.contains("slow_spans"));
}

#[test]
fn monitor_once_outputs_delta_summary() {
    let dir = tempfile::tempdir().unwrap();
    let log_path = dir.path().join("teolog.ndjson");
    fs::write(&log_path, sample_log()).unwrap();

    let output = Command::new(env!("CARGO_BIN_EXE_teolog"))
        .args([
            "monitor",
            log_path.to_str().unwrap(),
            "--json",
            "--once",
            "--limit",
            "2",
        ])
        .output()
        .unwrap();

    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(stdout.contains("delta_summary"));
    assert!(stdout.contains("delta_events"));
    assert!(stdout.contains("menu_failed"));
}

#[test]
fn monitor_since_last_uses_state_file() {
    let dir = tempfile::tempdir().unwrap();
    let log_path = dir.path().join("teolog.ndjson");
    let state_path = dir.path().join("monitor-state.json");
    fs::write(&log_path, sample_log()).unwrap();

    let first = Command::new(env!("CARGO_BIN_EXE_teolog"))
        .args([
            "monitor",
            log_path.to_str().unwrap(),
            "--json",
            "--once",
            "--since-last",
            "--state-file",
            state_path.to_str().unwrap(),
        ])
        .output()
        .unwrap();
    assert!(first.status.success());
    assert!(String::from_utf8(first.stdout)
        .unwrap()
        .contains("\"delta_events\":2"));

    let second = Command::new(env!("CARGO_BIN_EXE_teolog"))
        .args([
            "monitor",
            log_path.to_str().unwrap(),
            "--json",
            "--once",
            "--since-last",
            "--state-file",
            state_path.to_str().unwrap(),
        ])
        .output()
        .unwrap();
    assert!(second.status.success());
    assert!(String::from_utf8(second.stdout)
        .unwrap()
        .contains("\"delta_events\":0"));

    let appended = format!(
        "{}\n{}",
        sample_log(),
        r#"{"ts":"2026-04-12T17:00:02Z","lvl":"info","msg":"menu_refreshed","trace_id":"cccccccccccccccccccccccccccccccc","span_id":"3333333333333333","project":"demo","service":"api","data":{"count":6}}"#
    );
    fs::write(&log_path, appended).unwrap();

    let third = Command::new(env!("CARGO_BIN_EXE_teolog"))
        .args([
            "monitor",
            log_path.to_str().unwrap(),
            "--json",
            "--once",
            "--since-last",
            "--state-file",
            state_path.to_str().unwrap(),
        ])
        .output()
        .unwrap();
    assert!(third.status.success());
    let third_stdout = String::from_utf8(third.stdout).unwrap();
    assert!(third_stdout.contains("\"delta_events\":1"));
    assert!(third_stdout.contains("menu_refreshed"));
}
