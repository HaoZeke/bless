use crate::bless_log_capnp::bless_server;
use crate::bless_log_capnp::log_line;
use crate::bless_log_capnp::log_sink;
use crate::error::BlessError;
use crate::rpc::resolve_tcp_addr;
use capnp_rpc::rpc_twoparty_capnp::Side;
use capnp_rpc::twoparty::VatNetwork;
use capnp_rpc::RpcSystem;
use futures::AsyncReadExt;
use log::{Level, Log, Metadata, Record};
use std::sync::mpsc as std_mpsc;
use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tokio::net::TcpStream;
use tokio_util::compat::TokioAsyncReadCompatExt;

pub struct PendingLine {
    timestamp: f64,
    level: Level,
    message: String,
}

/// A logger that buffers log lines for later batch-sending to a remote server.
/// Lines are collected via the `log::Log` trait (sync) and drained by the
/// async runtime when ready.
#[derive(Clone)]
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

fn rpc_err(err: capnp::Error) -> BlessError {
    BlessError::Rpc(err.to_string())
}

fn drain_pending(rx: &Mutex<std_mpsc::Receiver<PendingLine>>) -> Vec<PendingLine> {
    rx.lock()
        .expect("remote log receiver mutex poisoned")
        .try_iter()
        .collect()
}

async fn send_lines(sink: &log_sink::Client, lines: &[PendingLine]) -> Result<(), capnp::Error> {
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

/// Connected remote session: openSession, a RemoteLogger, periodic flush, close.
pub struct RemoteSession {
    logger: RemoteLogger,
    sink: log_sink::Client,
    rx: Arc<Mutex<std_mpsc::Receiver<PendingLine>>>,
    flush_stop: Option<tokio::sync::oneshot::Sender<()>>,
    flush_task: Option<tokio::task::JoinHandle<()>>,
}

impl RemoteSession {
    /// Connect to `addr` (`:port` is 127.0.0.1), open a session, start flushing.
    pub async fn connect(
        addr: &str,
        label: &str,
        uuid: &str,
        command: &str,
        args: &str,
    ) -> Result<Self, BlessError> {
        let addr = resolve_tcp_addr(addr);
        let stream = TcpStream::connect(&addr).await?;
        let _ = stream.set_nodelay(true);
        let (reader, writer) = TokioAsyncReadCompatExt::compat(stream).split();
        let network = VatNetwork::new(reader, writer, Side::Client, Default::default());
        let mut rpc = RpcSystem::new(Box::new(network), None);
        let server: bless_server::Client = rpc.bootstrap(Side::Server);
        tokio::task::spawn_local(rpc);

        let mut request = server.open_session_request();
        {
            let mut meta = request.get().init_meta();
            meta.set_label(label);
            meta.set_uuid(uuid);
            meta.set_command(command);
            meta.set_args(args);
            if let Ok(host) = std::env::var("HOSTNAME") {
                meta.set_hostname(&host);
            }
            let start = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map(|d| d.as_secs_f64())
                .unwrap_or(0.0);
            meta.set_start_time(start);
        }
        let reply = request.send().promise.await.map_err(rpc_err)?;
        let sink = reply.get().map_err(rpc_err)?.get_sink().map_err(rpc_err)?;

        let (logger, rx) = RemoteLogger::new();
        let mut session = Self {
            logger,
            sink,
            rx: Arc::new(Mutex::new(rx)),
            flush_stop: None,
            flush_task: None,
        };
        session.start_flusher();
        Ok(session)
    }

    pub fn logger(&self) -> RemoteLogger {
        self.logger.clone()
    }

    fn start_flusher(&mut self) {
        let sink = self.sink.clone();
        let rx = Arc::clone(&self.rx);
        let (stop_tx, mut stop_rx) = tokio::sync::oneshot::channel();
        let task = tokio::task::spawn_local(async move {
            let mut interval = tokio::time::interval(Duration::from_millis(100));
            interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
            loop {
                tokio::select! {
                    _ = &mut stop_rx => break,
                    _ = interval.tick() => {
                        let lines = drain_pending(&rx);
                        if !lines.is_empty() {
                            let _ = send_lines(&sink, &lines).await;
                        }
                    }
                }
            }
        });
        self.flush_stop = Some(stop_tx);
        self.flush_task = Some(task);
    }

    /// Stop the flusher, send remaining lines, then close the session.
    pub async fn finish(mut self, exit_code: i32, duration: &str) -> Result<(), BlessError> {
        if let Some(tx) = self.flush_stop.take() {
            let _ = tx.send(());
        }
        if let Some(task) = self.flush_task.take() {
            let _ = task.await;
        }
        let lines = drain_pending(&self.rx);
        send_lines(&self.sink, &lines).await.map_err(rpc_err)?;
        close_session(&self.sink, exit_code, duration)
            .await
            .map_err(rpc_err)
    }
}
