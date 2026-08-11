use crate::bless_log_capnp::{bless_server, log_sink};
use capnp::capability::Promise;
use capnp_rpc::pry;
use flate2::write::GzEncoder;
use flate2::Compression;
use std::collections::HashMap;
use std::fs::{self, File};
use std::io::Write;
use std::path::{Path, PathBuf};
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

#[derive(Clone)]
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

        let filepath = match session_log_path(&self.data_dir, &label, &uuid) {
            Ok(p) => p,
            Err(e) => return Promise::err(e),
        };

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
            sessions: Arc::clone(&self.sessions),
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
    sessions: Arc<Mutex<HashMap<String, Arc<Mutex<SessionState>>>>>,
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

        let (label, uuid, command, line_count) = {
            let mut state = self.state.lock().expect("session mutex poisoned");

            let line_count = state.line_count;
            let _ = writeln!(
                state.encoder,
                "[session-end] exit_code={} duration={} lines={}",
                exit_code, duration, line_count
            );
            let _ = state.encoder.try_finish();

            (
                state.label.clone(),
                state.uuid.clone(),
                state.command.clone(),
                line_count,
            )
        };

        self.sessions
            .lock()
            .expect("sessions mutex poisoned")
            .remove(&uuid);

        eprintln!(
            "[serve] session closed: {label} ({uuid}) exit={exit_code} lines={line_count} duration={duration}"
        );

        let index_path = self.data_dir.join("index.json");
        let entry = serde_json::json!({
            "uuid": uuid,
            "label": label,
            "command": command,
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

/// Session filename component: `[A-Za-z0-9._-]` only. Rejects `..` and
/// absolute paths so a label/uuid cannot escape `data_dir`.
pub(crate) fn sanitize_session_component(value: &str) -> Result<&str, capnp::Error> {
    if value.is_empty() {
        return Err(capnp::Error::failed("empty label/uuid".into()));
    }
    if value.contains("..") {
        return Err(capnp::Error::failed(
            "label/uuid must not contain ..".into(),
        ));
    }
    if Path::new(value).is_absolute() {
        return Err(capnp::Error::failed(
            "label/uuid must not be an absolute path".into(),
        ));
    }
    if !value
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-'))
    {
        return Err(capnp::Error::failed(
            "label/uuid may only contain [A-Za-z0-9._-]".into(),
        ));
    }
    Ok(value)
}

/// `{label}_{uuid}.log.gz` under `data_dir`, after canonicalize.
pub(crate) fn session_log_path(
    data_dir: &Path,
    label: &str,
    uuid: &str,
) -> Result<PathBuf, capnp::Error> {
    let label = sanitize_session_component(label)?;
    let uuid = sanitize_session_component(uuid)?;
    let filename = format!("{label}_{uuid}.log.gz");

    let data_dir = data_dir
        .canonicalize()
        .map_err(|e| capnp::Error::failed(format!("cannot canonicalize data_dir: {e}")))?;

    let filepath = data_dir.join(filename);
    match filepath.parent() {
        Some(parent) => {
            let parent = parent.canonicalize().map_err(|e| {
                capnp::Error::failed(format!("cannot canonicalize session parent: {e}"))
            })?;
            if parent != data_dir {
                return Err(capnp::Error::failed(
                    "session path escapes data directory".into(),
                ));
            }
        }
        None => {
            return Err(capnp::Error::failed("session path has no parent".into()));
        }
    }
    Ok(filepath)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_plain_label_and_uuid() {
        assert_eq!(
            sanitize_session_component("my-label.v1_0").unwrap(),
            "my-label.v1_0"
        );
        assert_eq!(
            sanitize_session_component("01234567-89ab-cdef-0123-456789abcdef").unwrap(),
            "01234567-89ab-cdef-0123-456789abcdef"
        );
    }

    #[test]
    fn rejects_dotdot() {
        assert!(sanitize_session_component("..").is_err());
        assert!(sanitize_session_component("../etc").is_err());
        assert!(sanitize_session_component("foo..bar").is_err());
    }

    #[test]
    fn rejects_absolute_and_separators() {
        assert!(sanitize_session_component("/tmp/x").is_err());
        assert!(sanitize_session_component("a/b").is_err());
        assert!(sanitize_session_component("a\\b").is_err());
        assert!(sanitize_session_component("lab el").is_err());
    }

    #[test]
    fn rejects_empty() {
        assert!(sanitize_session_component("").is_err());
    }

    #[test]
    fn session_path_stays_under_data_dir() {
        let dir = tempfile::tempdir().unwrap();
        let path = session_log_path(dir.path(), "lab", "u-1").unwrap();
        let data_dir = dir.path().canonicalize().unwrap();
        assert!(path.starts_with(&data_dir));
        assert_eq!(path.file_name().unwrap(), "lab_u-1.log.gz");
    }

    #[test]
    fn session_path_rejects_escape_label() {
        let dir = tempfile::tempdir().unwrap();
        assert!(session_log_path(dir.path(), "../etc", "u1").is_err());
        assert!(session_log_path(dir.path(), "lab", "/tmp/x").is_err());
    }
}
