use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::fs;
use std::io::{self, BufRead, IsTerminal};
use std::path::{Path, PathBuf};

use clap::{Args, Parser, Subcommand, ValueEnum};
use regex::RegexBuilder;
use serde::{Deserialize, Serialize};
use serde_json::{json, Map, Value};
use teolog::WideEvent;

#[derive(Parser, Debug)]
#[command(name = "teolog")]
#[command(about = "Compact CLI for teolog log queries")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand, Debug)]
enum Commands {
    Status(QueryCommand),
    Recent(QueryCommand),
    Errors(QueryCommand),
    Search(SearchCommand),
    Trace(TraceCommand),
    Summary(SummaryCommand),
    Monitor(MonitorCommand),
}

#[derive(Args, Debug, Clone)]
struct QueryCommand {
    path: Option<PathBuf>,
    #[arg(long, default_value_t = 20)]
    limit: usize,
    #[arg(long)]
    level: Option<String>,
    #[command(flatten)]
    render: RenderArgs,
}

#[derive(Args, Debug, Clone)]
struct SearchCommand {
    pattern: String,
    path: Option<PathBuf>,
    #[arg(long, default_value_t = 20)]
    limit: usize,
    #[arg(long)]
    level: Option<String>,
    #[command(flatten)]
    render: RenderArgs,
}

#[derive(Args, Debug, Clone)]
struct TraceCommand {
    trace_id: String,
    path: Option<PathBuf>,
    #[arg(long)]
    summary_only: bool,
    #[command(flatten)]
    render: RenderArgs,
}

#[derive(Args, Debug, Clone)]
struct SummaryCommand {
    path: Option<PathBuf>,
    #[arg(long)]
    since_ts: Option<String>,
    #[arg(long, default_value_t = 200)]
    slow_ms: u64,
    #[arg(long, default_value_t = 5)]
    limit: usize,
    #[command(flatten)]
    render: RenderArgs,
}

#[derive(Args, Debug, Clone)]
struct MonitorCommand {
    path: Option<PathBuf>,
    #[arg(long, default_value_t = 1000)]
    interval_ms: u64,
    #[arg(long, default_value_t = 200)]
    slow_ms: u64,
    #[arg(long, default_value_t = 5)]
    limit: usize,
    #[arg(long)]
    once: bool,
    #[arg(long)]
    since_last: bool,
    #[arg(long)]
    state_file: Option<PathBuf>,
    #[command(flatten)]
    render: RenderArgs,
}

#[derive(Args, Debug, Clone, Default)]
struct RenderArgs {
    #[arg(long)]
    json: bool,
    #[arg(long)]
    count: bool,
    #[arg(long, value_enum)]
    output: Option<OutputFormat>,
    #[arg(long)]
    fields: Option<String>,
}

#[derive(ValueEnum, Clone, Copy, Debug, Eq, PartialEq)]
enum OutputFormat {
    Json,
    Compact,
    Ndjson,
    Table,
}

#[derive(Debug, Clone)]
struct LoadedEvent {
    event: WideEvent,
    raw: Value,
    text: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct MonitorState {
    log_file: String,
    events_seen: usize,
}

fn main() -> std::process::ExitCode {
    let argv: Vec<String> = std::env::args().collect();
    let json_mode = argv.iter().any(|arg| arg == "--json");

    let cli = match Cli::try_parse_from(&argv) {
        Ok(cli) => cli,
        Err(error) => {
            if json_mode {
                emit_json_error("parse", "parse", &error.to_string());
            } else {
                let _ = error.print();
            }
            return std::process::ExitCode::from(2);
        }
    };

    let command = command_name(&cli.command);
    let result = match cli.command {
        Commands::Status(cmd) => cmd_status(cmd),
        Commands::Recent(cmd) => cmd_recent(cmd),
        Commands::Errors(cmd) => cmd_errors(cmd),
        Commands::Search(cmd) => cmd_search(cmd),
        Commands::Trace(cmd) => cmd_trace(cmd),
        Commands::Summary(cmd) => cmd_summary(cmd),
        Commands::Monitor(cmd) => cmd_monitor(cmd),
    };

    match result {
        Ok(()) => std::process::ExitCode::SUCCESS,
        Err(error) => {
            if json_mode {
                emit_json_error(command, "runtime", &error.to_string());
            } else {
                eprintln!("Error: {error}");
            }
            std::process::ExitCode::from(1)
        }
    }
}

fn command_name(command: &Commands) -> &'static str {
    match command {
        Commands::Status(_) => "status",
        Commands::Recent(_) => "recent",
        Commands::Errors(_) => "errors",
        Commands::Search(_) => "search",
        Commands::Trace(_) => "trace",
        Commands::Summary(_) => "summary",
        Commands::Monitor(_) => "monitor",
    }
}

fn cmd_status(cmd: QueryCommand) -> Result<(), Box<dyn std::error::Error>> {
    let path = resolve_log_file(cmd.path)?;
    let events = load_events(&path)?;
    let value = build_status_value(&path, &events);
    render_value("status", value, &cmd.render)
}

fn cmd_recent(cmd: QueryCommand) -> Result<(), Box<dyn std::error::Error>> {
    let path = resolve_log_file(cmd.path)?;
    let events = load_events(&path)?;
    let rows = limit_events(
        filter_by_level(events.iter(), cmd.level.as_deref()),
        cmd.limit,
    )
    .into_iter()
    .map(|loaded| loaded.raw.clone())
    .collect();
    render_rows("recent", rows, &cmd.render)
}

fn cmd_errors(cmd: QueryCommand) -> Result<(), Box<dyn std::error::Error>> {
    let path = resolve_log_file(cmd.path)?;
    let events = load_events(&path)?;
    let rows = events
        .iter()
        .filter(|loaded| loaded.event.lvl.to_string() == "error")
        .filter(|loaded| level_matches(&loaded.event, cmd.level.as_deref()))
        .take(cmd.limit)
        .map(|loaded| loaded.raw.clone())
        .collect();
    render_rows("errors", rows, &cmd.render)
}

fn cmd_search(cmd: SearchCommand) -> Result<(), Box<dyn std::error::Error>> {
    let path = resolve_log_file(cmd.path)?;
    let events = load_events(&path)?;
    let regex = RegexBuilder::new(&cmd.pattern)
        .case_insensitive(true)
        .build()?;
    let rows = events
        .iter()
        .filter(|loaded| regex.is_match(&loaded.text))
        .filter(|loaded| level_matches(&loaded.event, cmd.level.as_deref()))
        .take(cmd.limit)
        .map(|loaded| loaded.raw.clone())
        .collect();
    render_rows("search", rows, &cmd.render)
}

fn cmd_trace(cmd: TraceCommand) -> Result<(), Box<dyn std::error::Error>> {
    let path = resolve_log_file(cmd.path)?;
    let events = load_events(&path)?;
    let matching: Vec<&LoadedEvent> = events
        .iter()
        .filter(|loaded| {
            loaded
                .event
                .trace_id
                .as_deref()
                .map(|trace| trace.starts_with(&cmd.trace_id))
                .unwrap_or(false)
        })
        .collect();

    if cmd.summary_only {
        let value = build_trace_summary_value(&path, &matching);
        render_value("trace", value, &cmd.render)
    } else {
        let rows = matching
            .into_iter()
            .map(|loaded| loaded.raw.clone())
            .collect();
        render_rows("trace", rows, &cmd.render)
    }
}

fn cmd_summary(cmd: SummaryCommand) -> Result<(), Box<dyn std::error::Error>> {
    let path = resolve_log_file(cmd.path)?;
    let events = load_events(&path)?;
    let filtered: Vec<&LoadedEvent> = events
        .iter()
        .filter(|loaded| match cmd.since_ts.as_deref() {
            Some(since_ts) => loaded.event.ts.as_str() >= since_ts,
            None => true,
        })
        .collect();
    let value = build_summary_value(&path, &filtered, cmd.limit, cmd.slow_ms);
    render_value("summary", value, &cmd.render)
}

fn cmd_monitor(cmd: MonitorCommand) -> Result<(), Box<dyn std::error::Error>> {
    let path = resolve_log_file(cmd.path)?;
    let state_path = if cmd.since_last {
        Some(resolve_monitor_state_path(cmd.state_file.clone())?)
    } else {
        None
    };
    let mut previous_len = state_path
        .as_ref()
        .and_then(|state_path| load_monitor_state(state_path).ok().flatten())
        .map(|state| {
            if state.log_file == normalize_path_for_state(&path) {
                state.events_seen
            } else {
                0
            }
        })
        .unwrap_or(0usize);

    loop {
        let events = load_events(&path)?;
        if events.len() < previous_len {
            previous_len = 0;
        }
        if events.len() > previous_len || cmd.once {
            let delta: Vec<&LoadedEvent> = events.iter().skip(previous_len).collect();
            let value = build_monitor_value(
                &path,
                &delta,
                cmd.limit,
                cmd.slow_ms,
                previous_len,
                events.len(),
            );
            render_monitor_value("monitor", value, &cmd.render)?;
            previous_len = events.len();
            if let Some(state_path) = state_path.as_ref() {
                save_monitor_state(state_path, &path, previous_len)?;
            }
        }

        if cmd.once {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(cmd.interval_ms));
    }

    Ok(())
}

fn resolve_log_file(path: Option<PathBuf>) -> Result<PathBuf, Box<dyn std::error::Error>> {
    if let Some(path) = path {
        if path.exists() {
            return Ok(path);
        }
        return Err(format!("log file not found at {}", path.display()).into());
    }

    if let Ok(env_path) = std::env::var("TEOLOG_FILE") {
        let path = PathBuf::from(env_path);
        if path.exists() {
            return Ok(path);
        }
    }

    let cwd = std::env::current_dir()?;
    let candidates = [
        cwd.join(".teolog/pi.ndjson"),
        cwd.join("teolog.ndjson"),
        cwd.join("app.log"),
        dirs_home().join(".local/state/nvim/teolog.log"),
        dirs_home().join("Library/Logs/teolog/teolog.json"),
        dirs_home().join(".local/share/teolog/teolog.json"),
    ];

    candidates
        .into_iter()
        .find(|path| path.exists())
        .ok_or_else(|| "no teolog log file found; pass a path explicitly".into())
}

fn dirs_home() -> PathBuf {
    std::env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."))
}

fn resolve_monitor_state_path(
    path: Option<PathBuf>,
) -> Result<PathBuf, Box<dyn std::error::Error>> {
    if let Some(path) = path {
        return Ok(path);
    }
    Ok(std::env::current_dir()?.join(".teolog/monitor-state.json"))
}

fn normalize_path_for_state(path: &Path) -> String {
    fs::canonicalize(path)
        .unwrap_or_else(|_| path.to_path_buf())
        .display()
        .to_string()
}

fn load_monitor_state(path: &Path) -> Result<Option<MonitorState>, Box<dyn std::error::Error>> {
    if !path.exists() {
        return Ok(None);
    }
    let text = fs::read_to_string(path)?;
    Ok(Some(serde_json::from_str(&text)?))
}

fn save_monitor_state(
    path: &Path,
    log_file: &Path,
    events_seen: usize,
) -> Result<(), Box<dyn std::error::Error>> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let state = MonitorState {
        log_file: normalize_path_for_state(log_file),
        events_seen,
    };
    fs::write(path, serde_json::to_string(&state)?)?;
    Ok(())
}

fn load_events(path: &Path) -> Result<Vec<LoadedEvent>, Box<dyn std::error::Error>> {
    let file = fs::File::open(path)?;
    let reader = io::BufReader::new(file);
    let mut loaded = Vec::new();

    for line in reader.lines() {
        let text = line?;
        if text.trim().is_empty() {
            continue;
        }
        let raw: Value = serde_json::from_str(&text)?;
        let event: WideEvent = serde_json::from_value(raw.clone())?;
        loaded.push(LoadedEvent { event, raw, text });
    }

    Ok(loaded)
}

fn filter_by_level<'a>(
    events: impl Iterator<Item = &'a LoadedEvent>,
    level: Option<&str>,
) -> Vec<&'a LoadedEvent> {
    events
        .filter(|loaded| level_matches(&loaded.event, level))
        .collect()
}

fn level_matches(event: &WideEvent, level: Option<&str>) -> bool {
    level
        .map(|level| event.lvl.to_string() == level)
        .unwrap_or(true)
}

fn limit_events(events: Vec<&LoadedEvent>, limit: usize) -> Vec<&LoadedEvent> {
    events
        .into_iter()
        .rev()
        .take(limit)
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect()
}

fn build_status_value(path: &Path, events: &[LoadedEvent]) -> Value {
    let last = events.last().map(|loaded| loaded.event.ts.clone());
    let mut by_level: HashMap<String, u64> = HashMap::new();
    let mut traces = BTreeSet::new();
    for loaded in events {
        *by_level.entry(loaded.event.lvl.to_string()).or_insert(0) += 1;
        if let Some(trace_id) = loaded.event.trace_id.as_deref() {
            traces.insert(trace_id.to_string());
        }
    }
    json!({
        "file": path.display().to_string(),
        "events": events.len(),
        "last_ts": last,
        "unique_traces": traces.len(),
        "by_level": by_level,
    })
}

fn build_trace_summary_value(path: &Path, matching: &[&LoadedEvent]) -> Value {
    let mut by_level: HashMap<String, u64> = HashMap::new();
    let mut services = BTreeSet::new();
    let mut projects = BTreeSet::new();
    let mut total_dur_ms = 0u64;
    let trace_id = matching
        .first()
        .and_then(|loaded| loaded.event.trace_id.clone())
        .unwrap_or_default();

    for loaded in matching {
        *by_level.entry(loaded.event.lvl.to_string()).or_insert(0) += 1;
        if let Some(service) = loaded.event.service.as_deref() {
            services.insert(service.to_string());
        }
        if let Some(project) = loaded.event.project.as_deref() {
            projects.insert(project.to_string());
        }
        total_dur_ms += loaded.event.dur_ms.unwrap_or(0);
    }

    json!({
        "file": path.display().to_string(),
        "trace_id": trace_id,
        "events": matching.len(),
        "by_level": by_level,
        "services": services.into_iter().collect::<Vec<_>>(),
        "projects": projects.into_iter().collect::<Vec<_>>(),
        "total_dur_ms": total_dur_ms,
    })
}

fn build_summary_value(
    path: &Path,
    filtered: &[&LoadedEvent],
    limit: usize,
    slow_ms: u64,
) -> Value {
    let mut by_level: HashMap<String, u64> = HashMap::new();
    let mut traces = BTreeSet::new();
    let mut projects = BTreeSet::new();
    let mut services = BTreeSet::new();
    let mut msg_counts: BTreeMap<String, u64> = BTreeMap::new();
    let mut slow = Vec::new();
    let mut recent_errors = Vec::new();

    for loaded in filtered {
        *by_level.entry(loaded.event.lvl.to_string()).or_insert(0) += 1;
        *msg_counts.entry(loaded.event.msg.clone()).or_insert(0) += 1;
        if let Some(trace_id) = loaded.event.trace_id.as_deref() {
            traces.insert(trace_id.to_string());
        }
        if let Some(project) = loaded.event.project.as_deref() {
            projects.insert(project.to_string());
        }
        if let Some(service) = loaded.event.service.as_deref() {
            services.insert(service.to_string());
        }
        if loaded.event.dur_ms.unwrap_or(0) >= slow_ms {
            slow.push(json!({
                "ts": loaded.event.ts,
                "msg": loaded.event.msg,
                "dur_ms": loaded.event.dur_ms,
                "trace_id": loaded.event.trace_id,
            }));
        }
        if loaded.event.lvl.to_string() == "error" {
            recent_errors.push(json!({
                "ts": loaded.event.ts,
                "msg": loaded.event.msg,
                "trace_id": loaded.event.trace_id,
                "err": loaded.event.err,
            }));
        }
    }

    let mut top_messages: Vec<_> = msg_counts.into_iter().collect();
    top_messages.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));

    json!({
        "file": path.display().to_string(),
        "events": filtered.len(),
        "unique_traces": traces.len(),
        "by_level": by_level,
        "projects": projects.into_iter().collect::<Vec<_>>(),
        "services": services.into_iter().collect::<Vec<_>>(),
        "top_messages": top_messages.into_iter().take(limit).map(|(msg, count)| json!({"msg": msg, "count": count})).collect::<Vec<_>>(),
        "recent_errors": recent_errors.into_iter().rev().take(limit).collect::<Vec<_>>(),
        "slow_spans": slow.into_iter().rev().take(limit).collect::<Vec<_>>(),
    })
}

fn build_monitor_value(
    path: &Path,
    delta: &[&LoadedEvent],
    limit: usize,
    slow_ms: u64,
    previous_len: usize,
    current_len: usize,
) -> Value {
    let summary = build_summary_value(path, delta, limit, slow_ms);
    json!({
        "file": path.display().to_string(),
        "kind": "delta_summary",
        "from_event": previous_len,
        "to_event": current_len,
        "delta_events": delta.len(),
        "summary": summary,
    })
}

fn emit_json_success(command: &str, data: Value) {
    println!(
        "{}",
        serde_json::to_string(&json!({
            "ok": true,
            "command": command,
            "version": env!("CARGO_PKG_VERSION"),
            "data": data,
        }))
        .expect("serialize success envelope")
    );
}

fn emit_json_error(command: &str, kind: &str, message: &str) {
    println!(
        "{}",
        serde_json::to_string(&json!({
            "ok": false,
            "command": command,
            "version": env!("CARGO_PKG_VERSION"),
            "error": {
                "kind": kind,
                "message": message,
            }
        }))
        .expect("serialize error envelope")
    );
}

fn render_rows(
    command: &str,
    rows: Vec<Value>,
    render: &RenderArgs,
) -> Result<(), Box<dyn std::error::Error>> {
    if render.count {
        println!("{}", rows.len());
        return Ok(());
    }
    let projected = rows
        .into_iter()
        .map(|row| project_fields(row, render.fields.as_deref()))
        .collect::<Vec<_>>();
    let output = resolve_output(render);
    match output {
        OutputFormat::Json => emit_json_success(command, Value::Array(projected)),
        OutputFormat::Compact => println!("{}", serde_json::to_string(&projected)?),
        OutputFormat::Ndjson => {
            for row in projected {
                println!("{}", serde_json::to_string(&row)?);
            }
        }
        OutputFormat::Table => print_rows_table(&projected),
    }
    Ok(())
}

fn render_value(
    command: &str,
    value: Value,
    render: &RenderArgs,
) -> Result<(), Box<dyn std::error::Error>> {
    if render.count {
        let count = match &value {
            Value::Array(rows) => rows.len() as u64,
            Value::Object(map) => map.len() as u64,
            _ => 1,
        };
        println!("{count}");
        return Ok(());
    }
    let projected = project_fields(value, render.fields.as_deref());
    match resolve_output(render) {
        OutputFormat::Json => emit_json_success(command, projected),
        OutputFormat::Compact => println!("{}", serde_json::to_string(&projected)?),
        OutputFormat::Ndjson => println!("{}", serde_json::to_string(&projected)?),
        OutputFormat::Table => print_value_table(&projected),
    }
    Ok(())
}

fn render_monitor_value(
    command: &str,
    value: Value,
    render: &RenderArgs,
) -> Result<(), Box<dyn std::error::Error>> {
    let output = resolve_output(render);
    let projected = project_fields(value, render.fields.as_deref());
    match output {
        OutputFormat::Json => emit_json_success(command, projected),
        OutputFormat::Compact | OutputFormat::Ndjson => {
            println!("{}", serde_json::to_string(&projected)?);
        }
        OutputFormat::Table => {
            if let Some(obj) = projected.as_object() {
                let delta_events = obj.get("delta_events").and_then(Value::as_u64).unwrap_or(0);
                let summary = obj.get("summary").cloned().unwrap_or(Value::Null);
                let errors = summary
                    .get("by_level")
                    .and_then(|by| by.get("error"))
                    .and_then(Value::as_u64)
                    .unwrap_or(0);
                let slow = summary
                    .get("slow_spans")
                    .and_then(Value::as_array)
                    .map(|rows| rows.len())
                    .unwrap_or(0);
                let top = summary
                    .get("top_messages")
                    .and_then(Value::as_array)
                    .and_then(|rows| rows.first())
                    .and_then(|row| row.get("msg"))
                    .and_then(Value::as_str)
                    .unwrap_or("-");
                println!("delta_events\terrors\tslow_spans\ttop_message");
                println!("{delta_events}\t{errors}\t{slow}\t{top}");
            }
        }
    }
    Ok(())
}

fn resolve_output(render: &RenderArgs) -> OutputFormat {
    if render.json {
        return OutputFormat::Json;
    }
    if let Some(format) = render.output {
        return format;
    }
    if io::stdout().is_terminal() {
        OutputFormat::Table
    } else {
        OutputFormat::Json
    }
}

fn project_fields(value: Value, fields: Option<&str>) -> Value {
    let Some(fields) = fields else { return value };
    let wanted = fields
        .split(',')
        .map(str::trim)
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>();
    if wanted.is_empty() {
        return value;
    }

    let mut projected = Map::new();
    for field in wanted {
        if let Some(found) = lookup_path(&value, field) {
            projected.insert(field.replace('.', "_"), found.clone());
        }
    }
    Value::Object(projected)
}

fn lookup_path<'a>(value: &'a Value, path: &str) -> Option<&'a Value> {
    let mut current = value;
    for segment in path.split('.') {
        current = current.get(segment)?;
    }
    Some(current)
}

fn print_rows_table(rows: &[Value]) {
    let mut headers = BTreeSet::new();
    for row in rows {
        if let Some(obj) = row.as_object() {
            for key in obj.keys() {
                headers.insert(key.clone());
            }
        }
    }
    let headers = headers.into_iter().collect::<Vec<_>>();
    if headers.is_empty() {
        println!("(no rows)");
        return;
    }
    println!("{}", headers.join("\t"));
    for row in rows {
        if let Some(obj) = row.as_object() {
            let line = headers
                .iter()
                .map(|key| cell_to_string(obj.get(key).unwrap_or(&Value::Null)))
                .collect::<Vec<_>>()
                .join("\t");
            println!("{line}");
        }
    }
}

fn print_value_table(value: &Value) {
    match value {
        Value::Array(rows) => print_rows_table(rows),
        Value::Object(obj) => {
            for (key, val) in obj {
                println!("{key}\t{}", cell_to_string(val));
            }
        }
        other => println!("{}", cell_to_string(other)),
    }
}

fn cell_to_string(value: &Value) -> String {
    match value {
        Value::Null => String::new(),
        Value::Bool(v) => v.to_string(),
        Value::Number(v) => v.to_string(),
        Value::String(v) => v.clone(),
        other => serde_json::to_string(other).unwrap_or_default(),
    }
}
