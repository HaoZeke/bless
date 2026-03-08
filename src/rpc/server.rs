use crate::bless_log_capnp::{bless_server, log_sink};
use capnp::capability::Promise;
use capnp_rpc::pry;
use flate2::write::GzEncoder;
use flate2::Compression;
use std::collections::HashMap;
use std::fs::{self, File};
use std::io::Write;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

/// Helper to extract capnp text fields, converting Utf8Error to capnp::Error.
fn text(r: Result<&str, std::str::Utf8Error>) -> Result<String, capnp::Error> {
    r.map(|s: &str| s.to_string())
        .map_err(|e| capnp::Error::failed(format!("UTF-8 error: {e}")))
}

struct SessionState {
    label: String,
    uuid: String,
    command: String,
    encoder: GzEncoder<File>,
    line_count: u64,
}

pub struct BlessServerImpl {
    sessions: Arc<Mutex<HashMap<String, Arc<Mutex<SessionState>>>>>,
    data_dir: PathBuf,
}

impl BlessServerImpl {
    pub fn new(data_dir: PathBuf) -> Self {
        fs::create_dir_all(&data_dir).expect("failed to create data directory");
        Self {
            sessions: Arc::new(Mutex::new(HashMap::new())),
            data_dir,
        }
    }
}

impl bless_server::Server for BlessServerImpl {
    fn open_session(
        &mut self,
        params: bless_server::OpenSessionParams,
        mut results: bless_server::OpenSessionResults,
    ) -> Promise<(), capnp::Error> {
        let meta = pry!(pry!(params.get()).get_meta());
        let label = pry!(text(pry!(meta.get_label()).to_str()));
        let uuid = pry!(text(pry!(meta.get_uuid()).to_str()));
        let command = pry!(text(pry!(meta.get_command()).to_str()));

        let filename = format!("{}_{}.log.gz", label, uuid);
        let filepath = self.data_dir.join(&filename);

        let file = match File::create(&filepath) {
            Ok(f) => f,
            Err(e) => {
                return Promise::err(capnp::Error::failed(format!(
                    "failed to create log file: {e}"
                )));
            }
        };
        let encoder = GzEncoder::new(file, Compression::default());

        let state = Arc::new(Mutex::new(SessionState {
            label: label.clone(),
            uuid: uuid.clone(),
            command: command.clone(),
            encoder,
            line_count: 0,
        }));

        self.sessions
            .lock()
            .expect("sessions mutex poisoned")
            .insert(uuid.clone(), Arc::clone(&state));

        let sink = LogSinkImpl {
            state,
            data_dir: self.data_dir.clone(),
        };
        results.get().set_sink(capnp_rpc::new_client(sink));

        eprintln!("[serve] session opened: {label} ({uuid}) cmd={command}");
        Promise::ok(())
    }

    fn list_sessions(
        &mut self,
        params: bless_server::ListSessionsParams,
        mut results: bless_server::ListSessionsResults,
    ) -> Promise<(), capnp::Error> {
        let limit = pry!(params.get()).get_limit() as usize;
        let sessions = self.sessions.lock().expect("sessions mutex poisoned");

        let entries: Vec<_> = sessions.values().take(limit).collect();
        let mut list = results.get().init_sessions(entries.len() as u32);

        for (i, session_arc) in entries.iter().enumerate() {
            let session = session_arc.lock().expect("session mutex poisoned");
            let mut entry = list.reborrow().get(i as u32);
            entry.set_uuid(&session.uuid);
            entry.set_label(&session.label);
            entry.set_command(&session.command);
            entry.set_line_count(session.line_count);
        }

        Promise::ok(())
    }
}

struct LogSinkImpl {
    state: Arc<Mutex<SessionState>>,
    data_dir: PathBuf,
}

impl log_sink::Server for LogSinkImpl {
    fn write_batch(
        &mut self,
        params: log_sink::WriteBatchParams,
        _results: log_sink::WriteBatchResults,
    ) -> Promise<(), capnp::Error> {
        let lines = pry!(pry!(params.get()).get_lines());
        let mut state = self.state.lock().expect("session mutex poisoned");

        for line in lines.iter() {
            let ts = line.get_timestamp();
            let level = match line.get_level() {
                Ok(l) => format!("{:?}", l),
                Err(_) => "UNKNOWN".to_string(),
            };
            let msg = line
                .get_message()
                .ok()
                .and_then(|r| r.to_str().ok())
                .unwrap_or("");

            let _ = writeln!(state.encoder, "[{:.3} {}] {}", ts, level, msg);
            state.line_count += 1;
        }

        Promise::ok(())
    }

    fn close(
        &mut self,
        params: log_sink::CloseParams,
        _results: log_sink::CloseResults,
    ) -> Promise<(), capnp::Error> {
        let reader = pry!(params.get());
        let exit_code = reader.get_exit_code();
        let duration = pry!(reader.get_duration())
            .to_str()
            .unwrap_or("unknown")
            .to_string();

        let mut state = self.state.lock().expect("session mutex poisoned");

        let line_count = state.line_count;
        let _ = writeln!(
            state.encoder,
            "[session-end] exit_code={} duration={} lines={}",
            exit_code, duration, line_count
        );

        let label = state.label.clone();
        let uuid = state.uuid.clone();

        eprintln!(
            "[serve] session closed: {label} ({uuid}) exit={exit_code} lines={line_count} duration={duration}"
        );

        let index_path = self.data_dir.join("index.json");
        let entry = serde_json::json!({
            "uuid": uuid,
            "label": label,
            "command": state.command,
            "exit_code": exit_code,
            "duration": duration,
            "line_count": line_count,
        });
        if let Ok(mut f) = fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&index_path)
        {
            let _ = writeln!(f, "{}", entry);
        }

        Promise::ok(())
    }
}
