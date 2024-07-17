use bless::storage_backends::gzip::GzipLogWrapper;
use fern::InitError;
use log::Log;
use std::time::SystemTime;

pub fn setup_logger(
    labelname: &str,
    uuid: &String,
    use_mongodb: bool,
) -> Result<Option<Box<GzipLogWrapper>>, InitError> {
    let dispatch = fern::Dispatch::new()
        .format(move |out, message, record| {
            out.finish(format_args!(
                "[{} {}] {}",
                humantime::format_rfc3339_seconds(SystemTime::now()),
                record.level(),
                message
            ))
        })
        .level(log::LevelFilter::Debug)
        .chain(std::io::stdout());

    if !use_mongodb {
        let filename = format!("{}_{}.log.gz", labelname, uuid);
        let file_logger = GzipLogWrapper::new(&filename);
        let logger_clone = Box::new(file_logger.clone()) as Box<dyn Log>;
        dispatch.chain(logger_clone).apply()?;
        Ok(Some(Box::new(file_logger)))
    } else {
        dispatch.apply()?;
        Ok(None)
    }
}
