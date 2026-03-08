use crate::cli::OutputFormat;
use crate::error::BlessError;
use crate::storage_backends::gzip::GzipLogWrapper;
use log::Log;
use std::time::SystemTime;

pub struct LoggerConfig<'a> {
    pub label: &'a str,
    pub uuid: &'a str,
    pub use_mongodb: bool,
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
}

fn resolve_output_path(config: &LoggerConfig) -> Option<String> {
    match config.output {
        Some("-") => None,
        Some(path) => Some(path.to_string()),
        None => Some(format!("{}_{}.log.gz", config.label, config.uuid)),
    }
}

pub fn setup_logger(config: &LoggerConfig) -> Result<LoggerHandles, BlessError> {
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

    let mut handles = LoggerHandles {
        gzip_logger: None,
        stdout_gzip: None,
        stderr_gzip: None,
    };

    let output_path = resolve_output_path(config);

    if config.split {
        // Split mode: separate files for stdout (INFO) and stderr (WARN+)
        let default_base = format!("{}_{}.log.gz", config.label, config.uuid);
        let base = output_path.as_deref().unwrap_or(&default_base);
        let stdout_path = base.replace(".log.gz", "_stdout.log.gz");
        let stderr_path = base.replace(".log.gz", "_stderr.log.gz");

        let stdout_logger = GzipLogWrapper::new(&stdout_path)?;
        let stderr_logger = GzipLogWrapper::new(&stderr_path)?;
        let stdout_clone = Box::new(stdout_logger.clone()) as Box<dyn Log>;
        let stderr_clone = Box::new(stderr_logger.clone()) as Box<dyn Log>;

        let stdout_file_dispatch = fern::Dispatch::new()
            .filter(|metadata| metadata.level() <= log::Level::Info)
            .chain(stdout_clone)
            .level(log::LevelFilter::Trace);

        let stderr_file_dispatch = fern::Dispatch::new()
            .filter(|metadata| {
                metadata.level() == log::Level::Warn || metadata.level() == log::Level::Error
            })
            .chain(stderr_clone)
            .level(log::LevelFilter::Trace);

        fern::Dispatch::new()
            .chain(stdout_dispatch)
            .chain(stdout_file_dispatch)
            .chain(stderr_file_dispatch)
            .apply()?;

        handles.stdout_gzip = Some(Box::new(stdout_logger));
        handles.stderr_gzip = Some(Box::new(stderr_logger));
    } else if let Some(path) = output_path {
        let file_logger = GzipLogWrapper::new(&path)?;
        let logger_clone = Box::new(file_logger.clone()) as Box<dyn Log>;

        let file_dispatch = fern::Dispatch::new()
            .chain(logger_clone)
            .level(log::LevelFilter::Trace);

        if config.use_mongodb {
            let mongodb_dispatch = fern::Dispatch::new()
                .filter(|metadata| matches!(metadata.level(), log::Level::Info | log::Level::Warn))
                .chain(file_dispatch);

            fern::Dispatch::new()
                .chain(stdout_dispatch)
                .chain(mongodb_dispatch)
                .apply()?;
        } else {
            fern::Dispatch::new()
                .chain(stdout_dispatch)
                .chain(file_dispatch)
                .apply()?;

            log::trace!("Label: {} UUID: {}", config.label, config.uuid);
        }

        handles.gzip_logger = Some(Box::new(file_logger));
    } else {
        // stdout only (output = "-")
        fern::Dispatch::new().chain(stdout_dispatch).apply()?;
    }

    Ok(handles)
}
