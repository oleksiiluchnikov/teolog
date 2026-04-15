use crate::event::WideEvent;
use std::io::Write;
use std::path::{Path, PathBuf};

/// Trait for emitting wide events to a destination.
pub trait Sink: Send + Sync {
    fn emit(&self, event: &WideEvent);
}

/// Emit NDJSON to stdout.
pub struct StdoutSink;

impl Sink for StdoutSink {
    fn emit(&self, event: &WideEvent) {
        let stdout = std::io::stdout();
        let mut handle = stdout.lock();
        let _ = writeln!(handle, "{}", event.to_json());
    }
}

/// Emit NDJSON to stderr (useful for CLI tools following stdout/stderr discipline).
pub struct StderrSink;

impl Sink for StderrSink {
    fn emit(&self, event: &WideEvent) {
        let stderr = std::io::stderr();
        let mut handle = stderr.lock();
        let _ = writeln!(handle, "{}", event.to_json());
    }
}

/// Emit NDJSON to any `Write` implementor (file, buffer, etc.).
pub struct WriterSink<W: Write + Send + Sync> {
    writer: std::sync::Mutex<W>,
}

impl<W: Write + Send + Sync> WriterSink<W> {
    pub fn new(writer: W) -> Self {
        Self {
            writer: std::sync::Mutex::new(writer),
        }
    }
}

impl<W: Write + Send + Sync> Sink for WriterSink<W> {
    fn emit(&self, event: &WideEvent) {
        if let Ok(mut w) = self.writer.lock() {
            let _ = writeln!(w, "{}", event.to_json());
        }
    }
}

/// Emit NDJSON to a file (append mode). Opens once, keeps handle.
pub struct FileSink {
    inner: std::sync::Mutex<std::io::BufWriter<std::fs::File>>,
}

impl FileSink {
    /// Open (or create) a file for append-mode NDJSON writing.
    pub fn open(path: impl AsRef<std::path::Path>) -> std::io::Result<Self> {
        if let Some(parent) = path.as_ref().parent() {
            std::fs::create_dir_all(parent)?;
        }
        let file = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(path)?;
        Ok(Self {
            inner: std::sync::Mutex::new(std::io::BufWriter::new(file)),
        })
    }
}

impl Sink for FileSink {
    fn emit(&self, event: &WideEvent) {
        if let Ok(mut w) = self.inner.lock() {
            let _ = writeln!(w, "{}", event.to_json());
            let _ = w.flush();
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct RotationPolicy {
    pub max_bytes: u64,
    pub max_files: usize,
}

impl RotationPolicy {
    pub const fn new(max_bytes: u64, max_files: usize) -> Self {
        Self {
            max_bytes,
            max_files,
        }
    }
}

impl Default for RotationPolicy {
    fn default() -> Self {
        Self {
            max_bytes: 10 * 1024 * 1024,
            max_files: 5,
        }
    }
}

struct RotatingFileState {
    path: PathBuf,
    policy: RotationPolicy,
    writer: std::io::BufWriter<std::fs::File>,
    size_bytes: u64,
}

impl RotatingFileState {
    fn open(path: impl AsRef<Path>, policy: RotationPolicy) -> std::io::Result<Self> {
        let path = path.as_ref().to_path_buf();
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let size_bytes = std::fs::metadata(&path).map(|m| m.len()).unwrap_or(0);
        let file = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&path)?;
        Ok(Self {
            path,
            policy,
            writer: std::io::BufWriter::new(file),
            size_bytes,
        })
    }

    fn emit_line(&mut self, line: &str) -> std::io::Result<()> {
        let line_bytes = line.as_bytes().len() as u64 + 1;
        if self.policy.max_bytes > 0
            && self.size_bytes > 0
            && self.size_bytes + line_bytes > self.policy.max_bytes
        {
            self.rotate()?;
        }
        writeln!(self.writer, "{line}")?;
        self.writer.flush()?;
        self.size_bytes += line_bytes;
        Ok(())
    }

    fn rotate(&mut self) -> std::io::Result<()> {
        self.writer.flush()?;
        if self.policy.max_files == 0 {
            self.writer = std::io::BufWriter::new(
                std::fs::OpenOptions::new()
                    .create(true)
                    .write(true)
                    .truncate(true)
                    .open(&self.path)?,
            );
            self.size_bytes = 0;
            return Ok(());
        }

        let oldest = archive_path(&self.path, self.policy.max_files);
        if oldest.exists() {
            let _ = std::fs::remove_file(&oldest);
        }
        for idx in (1..self.policy.max_files).rev() {
            let src = archive_path(&self.path, idx);
            let dst = archive_path(&self.path, idx + 1);
            if src.exists() {
                let _ = std::fs::rename(&src, &dst);
            }
        }
        if self.path.exists() {
            let _ = std::fs::rename(&self.path, archive_path(&self.path, 1));
        }
        self.writer = std::io::BufWriter::new(
            std::fs::OpenOptions::new()
                .create(true)
                .write(true)
                .truncate(true)
                .open(&self.path)?,
        );
        self.size_bytes = 0;
        Ok(())
    }
}

fn archive_path(path: &Path, idx: usize) -> PathBuf {
    let name = path
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_else(|| "teolog.ndjson".to_string());
    path.with_file_name(format!("{name}.{idx}"))
}

/// Emit NDJSON to a file with size-based rotation.
pub struct RotatingFileSink {
    inner: std::sync::Mutex<RotatingFileState>,
}

impl RotatingFileSink {
    pub fn open(path: impl AsRef<Path>, policy: RotationPolicy) -> std::io::Result<Self> {
        Ok(Self {
            inner: std::sync::Mutex::new(RotatingFileState::open(path, policy)?),
        })
    }
}

impl Sink for RotatingFileSink {
    fn emit(&self, event: &WideEvent) {
        if let Ok(mut state) = self.inner.lock() {
            let _ = state.emit_line(&event.to_json());
        }
    }
}

/// Fan out to multiple sinks.
pub struct MultiSink {
    sinks: Vec<Box<dyn Sink>>,
}

impl MultiSink {
    pub fn new(sinks: Vec<Box<dyn Sink>>) -> Self {
        Self { sinks }
    }
}

impl Sink for MultiSink {
    fn emit(&self, event: &WideEvent) {
        for sink in &self.sinks {
            sink.emit(event);
        }
    }
}

// ---------------------------------------------------------------------------
// HTTP Sink (behind "http-sink" feature)
// ---------------------------------------------------------------------------

/// Batch POST NDJSON to a URL with configurable headers.
///
/// Events are queued and flushed either when the batch reaches `batch_size`
/// or when `flush()` is called explicitly. A background thread flushes on
/// an interval if `flush_interval` is set.
///
/// Enable with: `teolog = { features = ["http-sink"] }`
#[cfg(feature = "http-sink")]
pub struct HttpSink {
    url: String,
    headers: Vec<(String, String)>,
    batch_size: usize,
    queue: std::sync::Mutex<Vec<String>>,
}

#[cfg(feature = "http-sink")]
impl HttpSink {
    /// Create a new HTTP sink.
    ///
    /// # Arguments
    /// * `url` — POST endpoint (e.g. `https://api.axiom.co/v1/datasets/logs/ingest`)
    /// * `headers` — extra headers (e.g. `[("Authorization", "Bearer ...")]`)
    /// * `batch_size` — flush after this many events (default: 100)
    pub fn new(url: impl Into<String>, headers: Vec<(String, String)>, batch_size: usize) -> Self {
        Self {
            url: url.into(),
            headers,
            batch_size: if batch_size == 0 { 100 } else { batch_size },
            queue: std::sync::Mutex::new(Vec::new()),
        }
    }

    /// Flush all queued events to the HTTP endpoint.
    pub fn flush(&self) {
        let batch = {
            let mut q = self.queue.lock().unwrap();
            if q.is_empty() {
                return;
            }
            std::mem::take(&mut *q)
        };

        let body = batch.join("\n");
        let mut req = ureq::post(&self.url).set("Content-Type", "application/x-ndjson");
        for (k, v) in &self.headers {
            req = req.set(k, v);
        }

        // Fire-and-forget: log errors to stderr but don't propagate.
        if let Err(e) = req.send_string(&body) {
            eprintln!("[teolog] HttpSink flush error: {e}");
        }
    }
}

#[cfg(feature = "http-sink")]
impl Sink for HttpSink {
    fn emit(&self, event: &WideEvent) {
        let json = event.to_json();
        let should_flush = {
            let mut q = self.queue.lock().unwrap();
            q.push(json);
            q.len() >= self.batch_size
        };
        if should_flush {
            self.flush();
        }
    }
}

#[cfg(feature = "http-sink")]
impl Drop for HttpSink {
    fn drop(&mut self) {
        self.flush();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::event::Level;

    #[test]
    fn writer_sink_captures_output() {
        let buf = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
        let sink = WriterSink::new(CursorWriter(buf.clone()));

        let event = WideEvent::new(Level::Info, "test");
        sink.emit(&event);

        let output = String::from_utf8(buf.lock().unwrap().clone()).unwrap();
        assert!(output.contains("\"msg\":\"test\""));
        assert!(output.ends_with('\n'));
    }

    /// Helper: wraps Arc<Mutex<Vec<u8>>> as a Write impl.
    struct CursorWriter(std::sync::Arc<std::sync::Mutex<Vec<u8>>>);

    impl Write for CursorWriter {
        fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
            self.0.lock().unwrap().extend_from_slice(buf);
            Ok(buf.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }

    unsafe impl Send for CursorWriter {}
    unsafe impl Sync for CursorWriter {}

    #[test]
    fn file_sink_writes_ndjson() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("test.jsonl");
        let sink = FileSink::open(&path).unwrap();

        sink.emit(&WideEvent::new(Level::Info, "line 1"));
        sink.emit(&WideEvent::new(Level::Error, "line 2"));

        let content = std::fs::read_to_string(&path).unwrap();
        let lines: Vec<&str> = content.lines().collect();
        assert_eq!(lines.len(), 2);
        assert!(lines[0].contains("\"line 1\""));
        assert!(lines[1].contains("\"line 2\""));
    }

    #[test]
    fn rotating_file_sink_rotates_and_keeps_archives() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("rotating.ndjson");
        let sink = RotatingFileSink::open(&path, RotationPolicy::new(120, 2)).unwrap();

        sink.emit(&WideEvent::new(
            Level::Info,
            "first event with enough bytes to rotate",
        ));
        sink.emit(&WideEvent::new(
            Level::Info,
            "second event with enough bytes to rotate",
        ));
        sink.emit(&WideEvent::new(
            Level::Info,
            "third event with enough bytes to rotate",
        ));

        let current = std::fs::read_to_string(&path).unwrap();
        let archive1 = std::fs::read_to_string(dir.path().join("rotating.ndjson.1")).unwrap();
        assert!(current.contains("third event"));
        assert!(archive1.contains("second event"));
        assert!(dir.path().join("rotating.ndjson.2").exists());
    }
}
