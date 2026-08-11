use crate::bless_log_capnp::{bless_server, log_line, log_sink};
use capnp::capability::Promise;
use capnp_rpc::pry;
use flate2::write::GzEncoder;
use flate2::Compression;
use log::Level;
use std::collections::HashMap;
use std::fs::{self, File};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

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

/// Fields listSessions exposes. Live sessions leave duration empty and
/// exit_code at 0; close fills both and moves the row to `completed`.
#[derive(Clone, Debug, PartialEq, Eq)]
struct SessionSummaryData {
    uuid: String,
    label: String,
    command: String,
    duration: String,
    line_count: u64,
    exit_code: i32,
}

impl SessionState {
    fn live_summary(&self) -> SessionSummaryData {
        SessionSummaryData {
            uuid: self.uuid.clone(),
            label: self.label.clone(),
            command: self.command.clone(),
            duration: String::new(),
            line_count: self.line_count,
            exit_code: 0,
        }
    }
}

/// Live sessions first, then completed, truncated to `limit`.
fn collect_session_summaries(
    live: impl IntoIterator<Item = SessionSummaryData>,
    completed: impl IntoIterator<Item = SessionSummaryData>,
    limit: usize,
) -> Vec<SessionSummaryData> {
    live.into_iter().chain(completed).take(limit).collect()
}

/// Convert `LogLine.timestamp` (unix seconds) via [`SystemTime`] when the
/// value is finite; otherwise use now so the archive never writes NaN/Inf.
fn system_time_from_unix_secs(timestamp: f64) -> SystemTime {
    if !timestamp.is_finite() {
        return SystemTime::now();
    }
    let Ok(dur) = Duration::try_from_secs_f64(timestamp.abs()) else {
        return SystemTime::now();
    };
    if timestamp >= 0.0 {
        UNIX_EPOCH.checked_add(dur).unwrap_or_else(SystemTime::now)
    } else {
        UNIX_EPOCH.checked_sub(dur).unwrap_or_else(SystemTime::now)
    }
}

fn capnp_level_display(level: Result<log_line::Level, capnp::NotInSchema>) -> &'static str {
    match level {
        Ok(log_line::Level::Trace) => Level::Trace.as_str(),
        Ok(log_line::Level::Debug) => Level::Debug.as_str(),
        Ok(log_line::Level::Info) => Level::Info.as_str(),
        Ok(log_line::Level::Warn) => Level::Warn.as_str(),
        Ok(log_line::Level::Error) => Level::Error.as_str(),
        Err(_) => "UNKNOWN",
    }
}

/// One gzip archive line, same shape as GzipLogWrapper:
/// `[rfc3339_seconds LEVEL] message`.
fn format_session_log_line(timestamp: f64, level: &str, message: &str) -> String {
    format!(
        "[{} {}] {}",
        humantime::format_rfc3339_seconds(system_time_from_unix_secs(timestamp)),
        level,
        message
    )
}

#[derive(Clone)]
pub struct BlessServerImpl {
    sessions: Arc<Mutex<HashMap<String, Arc<Mutex<SessionState>>>>>,
    completed: Arc<Mutex<Vec<SessionSummaryData>>>,
    data_dir: PathBuf,
}

impl BlessServerImpl {
    pub fn new(data_dir: PathBuf) -> Self {
        fs::create_dir_all(&data_dir).expect("failed to create data directory");
        Self {
            sessions: Arc::new(Mutex::new(HashMap::new())),
            completed: Arc::new(Mutex::new(Vec::new())),
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
            completed: Arc::clone(&self.completed),
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
        let completed = self.completed.lock().expect("completed mutex poisoned");

        let summaries = collect_session_summaries(
            sessions.values().map(|session_arc| {
                session_arc
                    .lock()
                    .expect("session mutex poisoned")
                    .live_summary()
            }),
            completed.iter().cloned(),
            limit,
        );

        let mut list = results.get().init_sessions(summaries.len() as u32);
        for (i, summary) in summaries.iter().enumerate() {
            let mut entry = list.reborrow().get(i as u32);
            entry.set_uuid(&summary.uuid);
            entry.set_label(&summary.label);
            entry.set_command(&summary.command);
            entry.set_duration(&summary.duration);
            entry.set_line_count(summary.line_count);
            entry.set_exit_code(summary.exit_code);
        }

        Promise::ok(())
    }
}

struct LogSinkImpl {
    state: Arc<Mutex<SessionState>>,
    sessions: Arc<Mutex<HashMap<String, Arc<Mutex<SessionState>>>>>,
    completed: Arc<Mutex<Vec<SessionSummaryData>>>,
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
            let level = capnp_level_display(line.get_level());
            let msg = line
                .get_message()
                .ok()
                .and_then(|r| r.to_str().ok())
                .unwrap_or("");

            let formatted = format_session_log_line(ts, level, msg);
            let _ = writeln!(state.encoder, "{formatted}");
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

        self.completed
            .lock()
            .expect("completed mutex poisoned")
            .push(SessionSummaryData {
                uuid: uuid.clone(),
                label: label.clone(),
                command: command.clone(),
                duration: duration.clone(),
                line_count,
                exit_code,
            });

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

    #[test]
    fn formats_unix_timestamp_as_rfc3339_display_level_line() {
        let line = format_session_log_line(0.0, Level::Info.as_str(), "hello");
        assert_eq!(line, "[1970-01-01T00:00:00Z INFO] hello");

        let line = format_session_log_line(1_000_000_000.9, Level::Warn.as_str(), "tick");
        assert_eq!(line, "[2001-09-09T01:46:40Z WARN] tick");
    }

    #[test]
    fn capnp_levels_map_to_log_display() {
        assert_eq!(capnp_level_display(Ok(log_line::Level::Trace)), "TRACE");
        assert_eq!(capnp_level_display(Ok(log_line::Level::Debug)), "DEBUG");
        assert_eq!(capnp_level_display(Ok(log_line::Level::Info)), "INFO");
        assert_eq!(capnp_level_display(Ok(log_line::Level::Warn)), "WARN");
        assert_eq!(capnp_level_display(Ok(log_line::Level::Error)), "ERROR");
        assert_ne!(
            format!("{:?}", log_line::Level::Info),
            capnp_level_display(Ok(log_line::Level::Info))
        );
    }

    #[test]
    fn formats_all_levels_via_log_display_not_debug() {
        for (level, expected) in [
            (Level::Trace, "TRACE"),
            (Level::Debug, "DEBUG"),
            (Level::Info, "INFO"),
            (Level::Warn, "WARN"),
            (Level::Error, "ERROR"),
        ] {
            let line = format_session_log_line(0.0, level.as_str(), "m");
            assert_eq!(line, format!("[1970-01-01T00:00:00Z {expected}] m"));
            assert!(
                !line.contains(&format!("{:?}", level)),
                "must not use Debug Level: {line}"
            );
        }
    }

    #[test]
    fn non_finite_timestamp_uses_now_not_unix_float() {
        for ts in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
            let line = format_session_log_line(ts, Level::Info.as_str(), "x");
            assert!(
                !line.contains("NaN") && !line.contains("inf") && !line.contains("Inf"),
                "non-finite ts leaked: {line}"
            );
            let rest = line
                .strip_prefix('[')
                .and_then(|s| s.split_once("] "))
                .expect("expected [timestamp LEVEL] msg");
            let (inside, msg) = rest;
            let mut parts = inside.split_whitespace();
            let stamp = parts.next().expect("timestamp");
            let level = parts.next().expect("LEVEL");
            assert!(parts.next().is_none(), "extra fields: {inside}");
            assert!(stamp.contains('T'), "rfc3339 timestamp: {stamp}");
            assert_eq!(level, "INFO");
            assert_eq!(msg, "x");
        }
    }

    fn summary(
        uuid: &str,
        label: &str,
        command: &str,
        duration: &str,
        line_count: u64,
        exit_code: i32,
    ) -> SessionSummaryData {
        SessionSummaryData {
            uuid: uuid.into(),
            label: label.into(),
            command: command.into(),
            duration: duration.into(),
            line_count,
            exit_code,
        }
    }

    #[test]
    fn list_summaries_sets_all_session_summary_fields() {
        let live = [summary("live-u", "live-lab", "echo", "", 3, 0)];
        let done = [summary("done-u", "done-lab", "false", "1.2s", 7, 1)];
        let rows = collect_session_summaries(live, done, 10);
        assert_eq!(rows.len(), 2);

        assert_eq!(rows[0].uuid, "live-u");
        assert_eq!(rows[0].label, "live-lab");
        assert_eq!(rows[0].command, "echo");
        assert_eq!(rows[0].duration, "");
        assert_eq!(rows[0].line_count, 3);
        assert_eq!(rows[0].exit_code, 0);

        assert_eq!(rows[1].uuid, "done-u");
        assert_eq!(rows[1].label, "done-lab");
        assert_eq!(rows[1].command, "false");
        assert_eq!(rows[1].duration, "1.2s");
        assert_eq!(rows[1].line_count, 7);
        assert_eq!(rows[1].exit_code, 1);
    }

    #[test]
    fn list_summaries_live_first_then_completed_respects_limit() {
        let live = [
            summary("a", "la", "cmd-a", "", 1, 0),
            summary("b", "lb", "cmd-b", "", 2, 0),
        ];
        let done = [
            summary("c", "lc", "cmd-c", "10ms", 3, 0),
            summary("d", "ld", "cmd-d", "20ms", 4, 2),
        ];

        let all = collect_session_summaries(live.clone(), done.clone(), 10);
        assert_eq!(
            all.iter().map(|s| s.uuid.as_str()).collect::<Vec<_>>(),
            ["a", "b", "c", "d"]
        );

        let limited = collect_session_summaries(live, done, 3);
        assert_eq!(
            limited.iter().map(|s| s.uuid.as_str()).collect::<Vec<_>>(),
            ["a", "b", "c"]
        );
        assert_eq!(limited[2].duration, "10ms");
        assert_eq!(limited[2].exit_code, 0);

        let empty = collect_session_summaries(
            std::iter::empty::<SessionSummaryData>(),
            std::iter::empty(),
            5,
        );
        assert!(empty.is_empty());
    }
}
