use crate::cli::OutputFormat;
use crate::error::BlessError;
use crate::storage_backends::gzip::GzipLogWrapper;
use log::Log;
use std::path::PathBuf;
use std::time::SystemTime;

pub struct LoggerConfig<'a> {
    pub label: &'a str,
    pub uuid: &'a str,
    pub no_timestamp: bool,
    pub format: &'a OutputFormat,
    pub output: Option<&'a str>,
    pub split: bool,
}

pub struct LoggerHandles {
    pub gzip_logger: Option<Box<GzipLogWrapper>>,
    pub stdout_gzip: Option<Box<GzipLogWrapper>>,
    pub stderr_gzip: Option<Box<GzipLogWrapper>>,
}

/// One gzip file the logger opened.
///
/// Combined logs have `stream == None`. `--split` sets `stream` to
/// `"stdout"` or `"stderr"` so Mongo persist can tag each document.
#[cfg_attr(not(feature = "mongodb"), allow(dead_code))]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GzipFile {
    pub path: PathBuf,
    pub stream: Option<&'static str>,
}

impl LoggerHandles {
    pub fn finish_all(&self) -> Result<(), BlessError> {
        if let Some(ref logger) = self.gzip_logger {
            logger.finish()?;
        }
        if let Some(ref logger) = self.stdout_gzip {
            logger.finish()?;
        }
        if let Some(ref logger) = self.stderr_gzip {
            logger.finish()?;
        }
        Ok(())
    }

    /// Gzip paths this logger created. Empty for `-o -`.
    #[cfg_attr(not(feature = "mongodb"), allow(dead_code))]
    pub fn gzip_files(&self) -> Vec<GzipFile> {
        let mut files = Vec::new();
        if let Some(ref logger) = self.gzip_logger {
            files.push(GzipFile {
                path: logger.path().to_path_buf(),
                stream: None,
            });
        }
        if let Some(ref logger) = self.stdout_gzip {
            files.push(GzipFile {
                path: logger.path().to_path_buf(),
                stream: Some("stdout"),
            });
        }
        if let Some(ref logger) = self.stderr_gzip {
            files.push(GzipFile {
                path: logger.path().to_path_buf(),
                stream: Some("stderr"),
            });
        }
        files
    }

    /// Files `--use-mongodb` should persist after `finish_all`.
    ///
    /// `-o -` opens no gzip, so there is nothing to upload.
    #[cfg_attr(not(feature = "mongodb"), allow(dead_code))]
    pub fn require_gzip_files(&self) -> Result<Vec<GzipFile>, BlessError> {
        let files = self.gzip_files();
        if files.is_empty() {
            Err(BlessError::Config(
                "--use-mongodb requires a gzip log file; -o - writes nothing to persist".into(),
            ))
        } else {
            Ok(files)
        }
    }
}

/// Insert `_stdout` / `_stderr` before the log extension.
///
/// `.log.gz` is a compound extension so `foo.log.gz` becomes
/// `foo_stdout.log.gz`. A bare `.gz` (`build_log.gz` from the README)
/// becomes `build_log_stdout.log.gz`. A path with no extension gets
/// `_{stream}.log.gz`. The two stream names never resolve to one path.
pub(crate) fn split_stream_path(base: &str, stream: &str) -> String {
    if let Some(stem) = base.strip_suffix(".log.gz") {
        format!("{stem}_{stream}.log.gz")
    } else if let Some(stem) = base.strip_suffix(".gz") {
        format!("{stem}_{stream}.log.gz")
    } else {
        format!("{base}_{stream}.log.gz")
    }
}

/// Combined (non-`--split`) gzip archives every level.
///
/// Shared by the local and `--use-mongodb` single-file paths.
pub(crate) fn is_combined_file_level(level: log::Level) -> bool {
    matches!(
        level,
        log::Level::Trace
            | log::Level::Debug
            | log::Level::Info
            | log::Level::Warn
            | log::Level::Error
    )
}

/// Command stdout and bless metadata belong in the stdout gzip.
pub(crate) fn is_stdout_split_level(level: log::Level) -> bool {
    matches!(level, log::Level::Info | log::Level::Trace)
}

/// Command stderr and command-failure lines belong in the stderr gzip.
pub(crate) fn is_stderr_split_level(level: log::Level) -> bool {
    matches!(level, log::Level::Warn | log::Level::Error)
}

fn resolve_output_path(config: &LoggerConfig) -> Option<String> {
    match config.output {
        Some("-") => None,
        Some(path) => Some(path.to_string()),
        None => Some(format!("{}_{}.log.gz", config.label, config.uuid)),
    }
}

/// Gzip paths this config will create. Empty for `-o -`, with or without `--split`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum LogTargets {
    StdoutOnly,
    SingleFile(String),
    Split { stdout: String, stderr: String },
}

pub(crate) fn resolve_log_targets(config: &LoggerConfig) -> LogTargets {
    match resolve_output_path(config) {
        None => LogTargets::StdoutOnly,
        Some(path) if config.split => LogTargets::Split {
            stdout: split_stream_path(&path, "stdout"),
            stderr: split_stream_path(&path, "stderr"),
        },
        Some(path) => LogTargets::SingleFile(path),
    }
}

/// Open gzip wrappers for the resolved targets. Creates no files for `-o -`.
pub(crate) fn open_gzip_handles(config: &LoggerConfig) -> Result<LoggerHandles, BlessError> {
    let mut handles = LoggerHandles {
        gzip_logger: None,
        stdout_gzip: None,
        stderr_gzip: None,
    };
    match resolve_log_targets(config) {
        LogTargets::StdoutOnly => {}
        LogTargets::SingleFile(path) => {
            handles.gzip_logger = Some(Box::new(GzipLogWrapper::new(&path)?));
        }
        LogTargets::Split { stdout, stderr } => {
            handles.stdout_gzip = Some(Box::new(GzipLogWrapper::new(&stdout)?));
            handles.stderr_gzip = Some(Box::new(GzipLogWrapper::new(&stderr)?));
        }
    }
    Ok(handles)
}

pub fn setup_logger(config: &LoggerConfig) -> Result<LoggerHandles, BlessError> {
    setup_logger_with_extra(config, None)
}

fn apply_dispatch(dispatch: fern::Dispatch, extra: Option<Box<dyn Log>>) -> Result<(), BlessError> {
    let dispatch = if let Some(extra) = extra {
        dispatch.chain(
            fern::Dispatch::new()
                .chain(extra)
                .level(log::LevelFilter::Trace),
        )
    } else {
        dispatch
    };
    dispatch.apply()?;
    Ok(())
}

pub fn setup_logger_with_extra(
    config: &LoggerConfig,
    extra: Option<Box<dyn Log>>,
) -> Result<LoggerHandles, BlessError> {
    let no_timestamp = config.no_timestamp;
    let format = config.format.clone();

    let stdout_dispatch = fern::Dispatch::new()
        .format(move |out, message, record| match format {
            OutputFormat::Jsonl => {
                let ts = humantime::format_rfc3339_seconds(SystemTime::now()).to_string();
                let json = serde_json::json!({
                    "ts": ts,
                    "level": record.level().to_string(),
                    "msg": message.to_string(),
                });
                out.finish(format_args!("{}", json));
            }
            OutputFormat::Log => {
                if no_timestamp {
                    out.finish(format_args!("[{}] {}", record.level(), message));
                } else {
                    out.finish(format_args!(
                        "[{} {}] {}",
                        humantime::format_rfc3339_seconds(SystemTime::now()),
                        record.level(),
                        message
                    ));
                }
            }
        })
        .chain(std::io::stdout())
        .level(log::LevelFilter::Trace);

    let handles = open_gzip_handles(config)?;

    if let (Some(stdout_logger), Some(stderr_logger)) = (&handles.stdout_gzip, &handles.stderr_gzip)
    {
        let stdout_clone = Box::new(stdout_logger.as_ref().clone()) as Box<dyn Log>;
        let stderr_clone = Box::new(stderr_logger.as_ref().clone()) as Box<dyn Log>;

        let stdout_file_dispatch = fern::Dispatch::new()
            .filter(|metadata| is_stdout_split_level(metadata.level()))
            .chain(stdout_clone)
            .level(log::LevelFilter::Trace);

        let stderr_file_dispatch = fern::Dispatch::new()
            .filter(|metadata| is_stderr_split_level(metadata.level()))
            .chain(stderr_clone)
            .level(log::LevelFilter::Trace);

        apply_dispatch(
            fern::Dispatch::new()
                .chain(stdout_dispatch)
                .chain(stdout_file_dispatch)
                .chain(stderr_file_dispatch),
            extra,
        )?;
    } else if let Some(file_logger) = &handles.gzip_logger {
        let logger_clone = Box::new(file_logger.as_ref().clone()) as Box<dyn Log>;

        let file_dispatch = fern::Dispatch::new()
            .filter(|metadata| is_combined_file_level(metadata.level()))
            .chain(logger_clone)
            .level(log::LevelFilter::Trace);

        apply_dispatch(
            fern::Dispatch::new()
                .chain(stdout_dispatch)
                .chain(file_dispatch),
            extra,
        )?;
    } else {
        apply_dispatch(fern::Dispatch::new().chain(stdout_dispatch), extra)?;
    }

    log::trace!("Label: {} UUID: {}", config.label, config.uuid);

    Ok(handles)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    fn fmt() -> OutputFormat {
        OutputFormat::Log
    }

    fn config<'a>(
        output: Option<&'a str>,
        split: bool,
        format: &'a OutputFormat,
        uuid: &'a str,
    ) -> LoggerConfig<'a> {
        LoggerConfig {
            label: "lab",
            uuid,
            no_timestamp: true,
            format,
            output,
            split,
        }
    }

    #[test]
    fn split_stream_path_compound_log_gz() {
        assert_eq!(
            split_stream_path("foo.log.gz", "stdout"),
            "foo_stdout.log.gz"
        );
        assert_eq!(
            split_stream_path("foo.log.gz", "stderr"),
            "foo_stderr.log.gz"
        );
    }

    #[test]
    fn split_stream_path_bare_gz() {
        assert_eq!(
            split_stream_path("build_log.gz", "stdout"),
            "build_log_stdout.log.gz"
        );
        assert_eq!(
            split_stream_path("build_log.gz", "stderr"),
            "build_log_stderr.log.gz"
        );
    }

    #[test]
    fn split_stream_path_no_extension() {
        assert_eq!(split_stream_path("out", "stdout"), "out_stdout.log.gz");
        assert_eq!(split_stream_path("out", "stderr"), "out_stderr.log.gz");
    }

    #[test]
    fn split_stream_paths_never_collide() {
        for base in ["foo.log.gz", "build_log.gz", "out", "dir/run.log.gz"] {
            let stdout = split_stream_path(base, "stdout");
            let stderr = split_stream_path(base, "stderr");
            assert_ne!(stdout, stderr, "colliding paths for {base}");
            assert_ne!(stdout, base);
            assert_ne!(stderr, base);
        }
    }

    #[test]
    fn dash_output_is_stdout_only_without_split() {
        let format = fmt();
        let cfg = config(Some("-"), false, &format, "u1");
        assert_eq!(resolve_log_targets(&cfg), LogTargets::StdoutOnly);
    }

    #[test]
    fn dash_output_is_stdout_only_with_split() {
        let format = fmt();
        let cfg = config(Some("-"), true, &format, "u1");
        assert_eq!(resolve_log_targets(&cfg), LogTargets::StdoutOnly);
    }

    #[test]
    fn dash_output_with_split_creates_no_gzip() {
        let format = fmt();
        let uuid = format!("nolog-{}", uuid::Uuid::new_v4());
        let cfg = config(Some("-"), true, &format, &uuid);
        let handles = open_gzip_handles(&cfg).unwrap();
        assert!(handles.gzip_logger.is_none());
        assert!(handles.stdout_gzip.is_none());
        assert!(handles.stderr_gzip.is_none());
        assert!(handles.gzip_files().is_empty());
        assert!(!Path::new(&format!("lab_{uuid}.log.gz")).exists());
        assert!(!Path::new(&format!("lab_{uuid}_stdout.log.gz")).exists());
        assert!(!Path::new(&format!("lab_{uuid}_stderr.log.gz")).exists());
    }

    fn assert_files_match_targets(handles: &LoggerHandles, targets: &LogTargets) {
        let files = handles.gzip_files();
        match targets {
            LogTargets::StdoutOnly => {
                assert!(files.is_empty());
            }
            LogTargets::SingleFile(path) => {
                assert_eq!(files.len(), 1);
                assert_eq!(files[0].path, Path::new(path));
                assert_eq!(files[0].stream, None);
            }
            LogTargets::Split { stdout, stderr } => {
                assert_eq!(files.len(), 2);
                assert_eq!(files[0].path, Path::new(stdout));
                assert_eq!(files[0].stream, Some("stdout"));
                assert_eq!(files[1].path, Path::new(stderr));
                assert_eq!(files[1].stream, Some("stderr"));
            }
        }
    }

    #[test]
    fn handle_paths_match_resolve_targets_stdout_only() {
        let format = fmt();
        let cfg = config(Some("-"), false, &format, "u1");
        let targets = resolve_log_targets(&cfg);
        assert_eq!(targets, LogTargets::StdoutOnly);
        let handles = open_gzip_handles(&cfg).unwrap();
        assert_files_match_targets(&handles, &targets);
    }

    #[test]
    fn handle_paths_match_resolve_targets_single_file() {
        let dir = tempfile::tempdir().unwrap();
        let base = dir.path().join("run.log.gz");
        let base_str = base.to_str().unwrap();
        let format = fmt();
        let cfg = config(Some(base_str), false, &format, "u1");
        let targets = resolve_log_targets(&cfg);
        let handles = open_gzip_handles(&cfg).unwrap();
        assert_files_match_targets(&handles, &targets);
        assert!(base.exists());
        handles.finish_all().unwrap();
    }

    #[test]
    fn default_single_uses_label_uuid_name() {
        let format = fmt();
        let cfg = config(None, false, &format, "abcd");
        assert_eq!(
            resolve_log_targets(&cfg),
            LogTargets::SingleFile("lab_abcd.log.gz".into())
        );
    }

    #[test]
    fn handle_paths_match_resolve_targets_split() {
        let dir = tempfile::tempdir().unwrap();
        let base = dir.path().join("build_log.gz");
        let base_str = base.to_str().unwrap();
        let format = fmt();
        let cfg = config(Some(base_str), true, &format, "u1");
        let targets = resolve_log_targets(&cfg);
        let handles = open_gzip_handles(&cfg).unwrap();
        assert_files_match_targets(&handles, &targets);
        handles.finish_all().unwrap();
    }

    #[test]
    fn require_gzip_files_rejects_stdout_only() {
        let format = fmt();
        let cfg = config(Some("-"), false, &format, "u1");
        let handles = open_gzip_handles(&cfg).unwrap();
        let err = handles.require_gzip_files().unwrap_err();
        match err {
            BlessError::Config(msg) => {
                assert!(msg.contains("--use-mongodb"));
                assert!(msg.contains("-o -"));
                assert!(!msg.contains("Path("));
            }
            other => panic!("expected Config, got {other:?}"),
        }
    }

    #[test]
    fn require_gzip_files_returns_opened_paths() {
        let dir = tempfile::tempdir().unwrap();
        let base = dir.path().join("keep.log.gz");
        let base_str = base.to_str().unwrap();
        let format = fmt();
        let cfg = config(Some(base_str), false, &format, "u1");
        let handles = open_gzip_handles(&cfg).unwrap();
        let files = handles.require_gzip_files().unwrap();
        assert_eq!(files, handles.gzip_files());
        handles.finish_all().unwrap();
    }

    #[test]
    fn split_writes_two_distinct_paths() {
        let dir = tempfile::tempdir().unwrap();
        let base = dir.path().join("build_log.gz");
        let base_str = base.to_str().unwrap();
        let format = fmt();
        let cfg = config(Some(base_str), true, &format, "u1");

        match resolve_log_targets(&cfg) {
            LogTargets::Split { stdout, stderr } => {
                assert_ne!(stdout, stderr);
                assert!(stdout.ends_with("build_log_stdout.log.gz"));
                assert!(stderr.ends_with("build_log_stderr.log.gz"));
                let handles = open_gzip_handles(&cfg).unwrap();
                assert!(Path::new(&stdout).exists());
                assert!(Path::new(&stderr).exists());
                assert!(!base.exists());
                handles.finish_all().unwrap();
            }
            other => panic!("expected split targets, got {other:?}"),
        }
    }

    #[test]
    fn default_split_uses_label_uuid_stream_names() {
        let format = fmt();
        let cfg = config(None, true, &format, "abcd");
        assert_eq!(
            resolve_log_targets(&cfg),
            LogTargets::Split {
                stdout: "lab_abcd_stdout.log.gz".into(),
                stderr: "lab_abcd_stderr.log.gz".into(),
            }
        );
    }

    #[test]
    fn combined_file_level_accepts_error_and_trace() {
        assert!(is_combined_file_level(log::Level::Error));
        assert!(is_combined_file_level(log::Level::Trace));
        assert!(is_combined_file_level(log::Level::Debug));
        assert!(is_combined_file_level(log::Level::Info));
        assert!(is_combined_file_level(log::Level::Warn));
    }

    #[test]
    fn stdout_split_level_is_info_and_trace_only() {
        assert!(is_stdout_split_level(log::Level::Info));
        assert!(is_stdout_split_level(log::Level::Trace));
        assert!(!is_stdout_split_level(log::Level::Warn));
        assert!(!is_stdout_split_level(log::Level::Error));
        assert!(!is_stdout_split_level(log::Level::Debug));
    }

    #[test]
    fn stderr_split_level_is_warn_and_error_only() {
        assert!(is_stderr_split_level(log::Level::Warn));
        assert!(is_stderr_split_level(log::Level::Error));
        assert!(!is_stderr_split_level(log::Level::Info));
        assert!(!is_stderr_split_level(log::Level::Trace));
        assert!(!is_stderr_split_level(log::Level::Debug));
    }
}
