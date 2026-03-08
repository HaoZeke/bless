use bless::cli::Cli;
use bless::db::{list_databases, setup_mongodb};
use bless::error::BlessError;
use bless::logger::{setup_logger, LoggerConfig};
use bless::runner::run_command;
use bless::storage_backends::mongodb::{MongoDBStorage, SaveGzipBlobParams};
use clap::Parser;
use log::{error, trace};
use std::path::Path;
use std::process::{ExitCode, ExitStatus};
use uuid::Uuid;

fn main() -> ExitCode {
    let cli = Cli::parse();
    match run(cli) {
        Ok(status) => status
            .code()
            .map(|c| ExitCode::from(c as u8))
            .unwrap_or(ExitCode::FAILURE),
        Err(e) => {
            eprintln!("bless: {e}");
            ExitCode::FAILURE
        }
    }
}

#[tokio::main]
async fn run(cli: Cli) -> Result<ExitStatus, BlessError> {
    let run_uuid = Uuid::new_v4().to_string();

    let logger_config = LoggerConfig {
        label: &cli.label,
        uuid: &run_uuid,
        use_mongodb: cli.use_mongodb,
        no_timestamp: cli.no_timestamp,
        format: &cli.format,
        output: cli.output.as_deref(),
        split: cli.split,
    };

    let handles = setup_logger(&logger_config)?;

    let (command, args) = cli.command.split_first().expect("clap requires at least 1");

    let start_time = std::time::SystemTime::now();
    let status = match run_command(command, args).await {
        Ok(status) => {
            if !status.success() {
                error!(
                    "Command exited with status: {}",
                    status.code().unwrap_or(-1)
                );
            }
            status
        }
        Err(BlessError::Io(e)) => {
            error!("Failed to run command: {} {}", command, args.join(" "));
            error!("Error: {}", e);
            handles.finish_all()?;
            return Err(BlessError::Io(e));
        }
        Err(e) => {
            handles.finish_all()?;
            return Err(e);
        }
    };
    let end_time = std::time::SystemTime::now();

    let duration = match end_time.duration_since(start_time) {
        Ok(d) => {
            if !cli.use_mongodb {
                trace!(
                    "{} {} took {} to complete.",
                    command,
                    args.join(" "),
                    humantime::format_duration(d)
                );
            }
            humantime::format_duration(d).to_string()
        }
        Err(e) => {
            error!("Error calculating duration: {}", e);
            "unknown".to_string()
        }
    };

    handles.finish_all()?;

    if cli.use_mongodb {
        let client = setup_mongodb().await?;
        list_databases(&client).await?;
        let mongodb_storage = MongoDBStorage::new(&client, "local", "commands").await;

        let filename = cli
            .output
            .clone()
            .unwrap_or_else(|| format!("{}_{}.log.gz", cli.label, run_uuid));
        let file_path = Path::new(&filename);

        let params = SaveGzipBlobParams {
            cmd: command,
            args: &args.join(" "),
            label: &cli.label,
            duration: &duration,
            uuid: &run_uuid,
            file_path,
            start_time: start_time.into(),
            end_time: end_time.into(),
        };

        mongodb_storage.save_gzip_blob(params).await?;
    }

    Ok(status)
}
