//! Command-line logging helper for repeated runs with gzip (and optional
//! MongoDB or remote serve) metadata tracking. This crate is a CLI, not a
//! stable library: call `run` from the binary, use `BlessError` and `runner`
//! from tests, and treat every other module as crate-private.

#![warn(clippy::all)]

use std::process::ExitCode;

use clap::Parser;
use log::{error, trace};
use uuid::Uuid;

use crate::cli::Cli;
#[cfg(feature = "mongodb")]
use crate::db::setup_mongodb;
use crate::error::BlessError;
use crate::logger::{setup_logger_with_extra, LoggerConfig};
use crate::runner::{exit_code_from_status, run_command};
#[cfg(feature = "mongodb")]
use crate::storage_backends::mongodb::{MongoDBStorage, SaveGzipBlobParams};

pub(crate) mod cli;
#[cfg(feature = "mongodb")]
pub(crate) mod db;
pub mod error;
pub(crate) mod logger;
pub mod runner;
pub(crate) mod storage_backends;

#[cfg(feature = "serve")]
#[allow(clippy::all, unused_parens)]
mod bless_log_capnp {
    include!(concat!(env!("OUT_DIR"), "/bless_log_capnp.rs"));
}
#[cfg(feature = "serve")]
pub(crate) mod rpc;
#[cfg(feature = "serve")]
pub(crate) mod serve;

/// Parse CLI arguments and run the selected bless command.
pub fn run() -> Result<ExitCode, BlessError> {
    let cli = Cli::parse();
    run_parsed(cli)
}

#[tokio::main]
async fn run_parsed(cli: Cli) -> Result<ExitCode, BlessError> {
    tokio::task::LocalSet::new().run_until(run_async(cli)).await
}

async fn run_async(cli: Cli) -> Result<ExitCode, BlessError> {
    #[cfg(feature = "serve")]
    if let Some(addr) = cli.serve.as_deref() {
        crate::serve::run_server(addr).await?;
        return Ok(ExitCode::SUCCESS);
    }

    let run_uuid = Uuid::new_v4().to_string();

    #[cfg(feature = "serve")]
    let mut remote = match cli.remote.as_deref() {
        Some(addr) => {
            let command = cli.command.first().map(String::as_str).unwrap_or("");
            let args = cli
                .command
                .get(1..)
                .map(|a| a.join(" "))
                .unwrap_or_default();
            Some(
                crate::rpc::client::RemoteSession::connect(
                    addr, &cli.label, &run_uuid, command, &args,
                )
                .await?,
            )
        }
        None => None,
    };

    let logger_config = LoggerConfig {
        label: &cli.label,
        uuid: &run_uuid,
        #[cfg(feature = "mongodb")]
        use_mongodb: cli.use_mongodb,
        #[cfg(not(feature = "mongodb"))]
        use_mongodb: false,
        no_timestamp: cli.no_timestamp,
        format: &cli.format,
        output: cli.gzip_output(),
        split: cli.split,
    };

    #[cfg(feature = "serve")]
    let extra = remote
        .as_ref()
        .map(|session| Box::new(session.logger()) as Box<dyn log::Log>);
    #[cfg(not(feature = "serve"))]
    let extra = None;
    let handles = setup_logger_with_extra(&logger_config, extra)?;

    #[cfg(feature = "mongodb")]
    let persist_files = if cli.use_mongodb {
        Some(handles.require_gzip_files()?)
    } else {
        None
    };

    let (command, args) = cli
        .command
        .split_first()
        .ok_or_else(|| BlessError::Config("a command is required unless --serve is set".into()))?;

    let start_time = std::time::SystemTime::now();
    let status = match run_command(command, args).await {
        Ok(status) => {
            if !status.success() {
                error!(
                    "Command exited with status: {}",
                    exit_code_from_status(status)
                );
            }
            status
        }
        Err(BlessError::Io(e)) => {
            error!("Failed to run command: {} {}", command, args.join(" "));
            error!("Error: {}", e);
            handles.finish_all()?;
            #[cfg(feature = "serve")]
            finish_remote(remote.take(), 1, "unknown").await;
            return Err(BlessError::Io(e));
        }
        Err(e) => {
            handles.finish_all()?;
            #[cfg(feature = "serve")]
            finish_remote(remote.take(), 1, "unknown").await;
            return Err(e);
        }
    };
    let end_time = std::time::SystemTime::now();

    #[cfg_attr(not(feature = "mongodb"), allow(unused_variables))]
    let duration = match end_time.duration_since(start_time) {
        Ok(d) => {
            let skip_duration_trace = {
                #[cfg(feature = "mongodb")]
                {
                    cli.use_mongodb
                }
                #[cfg(not(feature = "mongodb"))]
                {
                    false
                }
            };
            if !skip_duration_trace {
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

    #[cfg(feature = "serve")]
    finish_remote(
        remote.take(),
        i32::from(exit_code_from_status(status)),
        &duration,
    )
    .await;

    #[cfg(feature = "mongodb")]
    if let Some(files) = persist_files {
        let client = setup_mongodb().await?;
        let mongodb_storage = MongoDBStorage::new(&client, &cli.db, &cli.collection).await?;
        let args_joined = args.join(" ");

        // One document per opened gzip. Combined runs insert once with
        // stream=""; --split inserts stdout then stderr.
        for file in &files {
            let params = SaveGzipBlobParams {
                cmd: command,
                args: &args_joined,
                label: &cli.label,
                duration: &duration,
                uuid: &run_uuid,
                file_path: &file.path,
                stream: file.stream.or(Some("")),
                start_time: start_time.into(),
                end_time: end_time.into(),
            };
            mongodb_storage
                .save_gzip_blob(params, cli.force_gridfs)
                .await?;
        }
    }

    Ok(ExitCode::from(exit_code_from_status(status)))
}

#[cfg(feature = "serve")]
async fn finish_remote(
    remote: Option<crate::rpc::client::RemoteSession>,
    exit_code: i32,
    duration: &str,
) {
    if let Some(remote) = remote {
        if let Err(e) = remote.finish(exit_code, duration).await {
            error!("remote session close failed: {e}");
        }
    }
}
