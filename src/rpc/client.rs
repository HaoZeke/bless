use crate::bless_log_capnp::log_line;
use crate::bless_log_capnp::log_sink;
use log::{Level, Log, Metadata, Record};
use std::sync::mpsc as std_mpsc;
use std::time::{SystemTime, UNIX_EPOCH};

pub struct PendingLine {
    timestamp: f64,
    level: Level,
    message: String,
}

/// A logger that buffers log lines for later batch-sending to a remote server.
/// Lines are collected via the `log::Log` trait (sync) and drained by the
/// async runtime when ready.
pub struct RemoteLogger {
    sender: std_mpsc::Sender<PendingLine>,
}

impl RemoteLogger {
    pub fn new() -> (Self, std_mpsc::Receiver<PendingLine>) {
        let (tx, rx) = std_mpsc::channel();
        (Self { sender: tx }, rx)
    }

    /// Drain all buffered lines and send them in batches to the remote sink.
    /// Call this periodically or after the command finishes.
    pub async fn flush_to_sink(
        rx: &std_mpsc::Receiver<PendingLine>,
        sink: &log_sink::Client,
    ) -> Result<(), capnp::Error> {
        let lines: Vec<_> = rx.try_iter().collect();
        if lines.is_empty() {
            return Ok(());
        }

        let mut request = sink.write_batch_request();
        let mut builder = request.get().init_lines(lines.len() as u32);

        for (i, line) in lines.iter().enumerate() {
            let mut entry = builder.reborrow().get(i as u32);
            entry.set_timestamp(line.timestamp);
            entry.set_level(match line.level {
                Level::Trace => log_line::Level::Trace,
                Level::Debug => log_line::Level::Debug,
                Level::Info => log_line::Level::Info,
                Level::Warn => log_line::Level::Warn,
                Level::Error => log_line::Level::Error,
            });
            entry.set_message(&line.message);
        }

        request.send().promise.await?;
        Ok(())
    }
}

impl Log for RemoteLogger {
    fn enabled(&self, metadata: &Metadata) -> bool {
        metadata.level() <= Level::Trace
    }

    fn log(&self, record: &Record) {
        if self.enabled(record.metadata()) {
            let timestamp = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map(|d| d.as_secs_f64())
                .unwrap_or(0.0);

            let _ = self.sender.send(PendingLine {
                timestamp,
                level: record.level(),
                message: record.args().to_string(),
            });
        }
    }

    fn flush(&self) {}
}

/// Close the remote session with exit code and duration.
pub async fn close_session(
    sink: &log_sink::Client,
    exit_code: i32,
    duration: &str,
) -> Result<(), capnp::Error> {
    let mut request = sink.close_request();
    request.get().set_exit_code(exit_code);
    request.get().set_duration(duration);
    request.send().promise.await?;
    Ok(())
}
