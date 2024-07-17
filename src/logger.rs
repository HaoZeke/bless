use bless::storage_backends::gzip::GzipLogWrapper;
use fern::InitError;
use log::trace;
use log::Log;
use std::time::SystemTime;

pub fn setup_logger(
    labelname: &str,
    uuid: &String,
    use_mongodb: bool,
) -> Result<Option<Box<GzipLogWrapper>>, InitError> {
    let filename = format!("{}_{}.log.gz", labelname, uuid);
    let file_logger = GzipLogWrapper::new(&filename);
    let logger_clone = Box::new(file_logger.clone()) as Box<dyn Log>;

    let stdout_dispatch = fern::Dispatch::new()
        .format(move |out, message, record| {
            out.finish(format_args!(
                "[{} {}] {}",
                humantime::format_rfc3339_seconds(SystemTime::now()),
                record.level(),
                message
            ))
        })
        .chain(std::io::stdout())
        .level(log::LevelFilter::Trace);

    let file_dispatch = fern::Dispatch::new()
        .chain(logger_clone)
        .level(if use_mongodb {
            log::LevelFilter::Info
        } else {
            log::LevelFilter::Trace
        });

    fern::Dispatch::new()
        .chain(stdout_dispatch)
        .chain(file_dispatch)
        .apply()?;

    if !use_mongodb {
        trace!("Label: {} UUID: {}", labelname, uuid);
    }
    Ok(Some(Box::new(file_logger)))
}
