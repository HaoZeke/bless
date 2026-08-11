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
    #[allow(dead_code)]
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
    closed: bool,
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
            closed: false,
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
    ///
    /// A failed final writeBatch still attempts close so the server can
    /// finalize the gzip and write the index.
    pub async fn finish(mut self, exit_code: i32, duration: &str) -> Result<(), BlessError> {
        self.stop_flusher().await;
        let lines = drain_pending(&self.rx);
        let send_result = send_lines(&self.sink, &lines).await.map_err(rpc_err);
        let close_result = close_session(&self.sink, exit_code, duration)
            .await
            .map_err(rpc_err);
        self.closed = true;
        send_result.and(close_result)
    }

    async fn stop_flusher(&mut self) {
        if let Some(tx) = self.flush_stop.take() {
            let _ = tx.send(());
        }
        if let Some(task) = self.flush_task.take() {
            let _ = task.await;
        }
    }
}

impl Drop for RemoteSession {
    fn drop(&mut self) {
        if self.closed {
            return;
        }
        if let Some(tx) = self.flush_stop.take() {
            let _ = tx.send(());
        }
        if let Some(task) = self.flush_task.take() {
            task.abort();
        }
        // Best-effort close if finish() was skipped. The caller still needs
        // an awaited finish() before tearing down the LocalSet; spawn_local
        // is only a safety net while the RPC system is still running.
        let sink = self.sink.clone();
        let _ = tokio::task::spawn_local(async move {
            let _ = close_session(&sink, 1, "unknown").await;
        });
        self.closed = true;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bless_log_capnp::bless_server;
    use capnp::capability::Promise;
    use capnp_rpc::pry;
    use capnp_rpc::rpc_twoparty_capnp::Side;
    use capnp_rpc::twoparty::VatNetwork;
    use capnp_rpc::RpcSystem;
    use futures::AsyncReadExt;
    use log::{Level, Record};
    use std::sync::atomic::{AtomicBool, AtomicI32, Ordering};
    use std::sync::Arc;
    use std::time::Duration;
    use tokio_util::compat::TokioAsyncReadCompatExt;

    #[derive(Clone)]
    struct FailWriteServer {
        closed: Arc<AtomicBool>,
        exit_code: Arc<AtomicI32>,
    }

    impl bless_server::Server for FailWriteServer {
        fn open_session(
            &mut self,
            _params: bless_server::OpenSessionParams,
            mut results: bless_server::OpenSessionResults,
        ) -> Promise<(), capnp::Error> {
            let sink = FailWriteSink {
                closed: Arc::clone(&self.closed),
                exit_code: Arc::clone(&self.exit_code),
            };
            results.get().set_sink(capnp_rpc::new_client(sink));
            Promise::ok(())
        }

        fn list_sessions(
            &mut self,
            _params: bless_server::ListSessionsParams,
            mut results: bless_server::ListSessionsResults,
        ) -> Promise<(), capnp::Error> {
            results.get().init_sessions(0);
            Promise::ok(())
        }
    }

    struct FailWriteSink {
        closed: Arc<AtomicBool>,
        exit_code: Arc<AtomicI32>,
    }

    impl log_sink::Server for FailWriteSink {
        fn write_batch(
            &mut self,
            _params: log_sink::WriteBatchParams,
            _results: log_sink::WriteBatchResults,
        ) -> Promise<(), capnp::Error> {
            Promise::err(capnp::Error::failed("write_batch failed".into()))
        }

        fn close(
            &mut self,
            params: log_sink::CloseParams,
            _results: log_sink::CloseResults,
        ) -> Promise<(), capnp::Error> {
            let reader = pry!(params.get());
            self.exit_code
                .store(reader.get_exit_code(), Ordering::SeqCst);
            self.closed.store(true, Ordering::SeqCst);
            Promise::ok(())
        }
    }

    async fn spawn_fail_write_server(server: FailWriteServer) -> String {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind test server");
        let addr = listener.local_addr().expect("local addr").to_string();
        tokio::task::spawn_local(async move {
            loop {
                let (stream, _) = match listener.accept().await {
                    Ok(s) => s,
                    Err(_) => break,
                };
                let server = server.clone();
                tokio::task::spawn_local(async move {
                    let (reader, writer) = TokioAsyncReadCompatExt::compat(stream).split();
                    let network = VatNetwork::new(reader, writer, Side::Server, Default::default());
                    let client: bless_server::Client = capnp_rpc::new_client(server);
                    let rpc = RpcSystem::new(Box::new(network), Some(client.clone().client));
                    let _ = rpc.await;
                });
            }
        });
        addr
    }

    fn enqueue_line(session: &RemoteSession, message: &str) {
        session.logger().log(
            &Record::builder()
                .args(format_args!("{message}"))
                .level(Level::Info)
                .target("bless-test")
                .module_path_static(Some("bless-test"))
                .file_static(Some("client.rs"))
                .line(Some(1))
                .build(),
        );
    }

    #[tokio::test]
    async fn finish_closes_after_write_batch_error() {
        tokio::task::LocalSet::new()
            .run_until(async {
                let closed = Arc::new(AtomicBool::new(false));
                let exit_code = Arc::new(AtomicI32::new(i32::MIN));
                let addr = spawn_fail_write_server(FailWriteServer {
                    closed: Arc::clone(&closed),
                    exit_code: Arc::clone(&exit_code),
                })
                .await;

                let mut session = RemoteSession::connect(&addr, "lab", "u-1", "true", "")
                    .await
                    .expect("connect");
                session.stop_flusher().await;
                enqueue_line(&session, "pending line");

                let err = session
                    .finish(7, "1s")
                    .await
                    .expect_err("writeBatch should fail");
                match err {
                    BlessError::Rpc(msg) => {
                        assert!(msg.contains("write_batch failed"), "{msg}");
                    }
                    other => panic!("expected RPC error, got {other}"),
                }
                assert!(closed.load(Ordering::SeqCst), "close must still run");
                assert_eq!(exit_code.load(Ordering::SeqCst), 7);
            })
            .await;
    }

    #[tokio::test]
    async fn drop_attempts_close_when_finish_is_skipped() {
        tokio::task::LocalSet::new()
            .run_until(async {
                let closed = Arc::new(AtomicBool::new(false));
                let exit_code = Arc::new(AtomicI32::new(i32::MIN));
                let addr = spawn_fail_write_server(FailWriteServer {
                    closed: Arc::clone(&closed),
                    exit_code: Arc::clone(&exit_code),
                })
                .await;

                {
                    let _session = RemoteSession::connect(&addr, "lab", "u-2", "true", "")
                        .await
                        .expect("connect");
                }

                let deadline = tokio::time::Instant::now() + Duration::from_secs(2);
                while !closed.load(Ordering::SeqCst) {
                    if tokio::time::Instant::now() >= deadline {
                        panic!("Drop should close the remote session");
                    }
                    tokio::time::sleep(Duration::from_millis(10)).await;
                }
                assert_eq!(exit_code.load(Ordering::SeqCst), 1);
            })
            .await;
    }
}
