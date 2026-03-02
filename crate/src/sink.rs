use crate::event::WideEvent;
use std::io::Write;

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
}
