use bless::cli::build_cli;
use bless::db::{list_databases, setup_mongodb};
use bless::logger::setup_logger;
use bless::runner::run_command;
use bless::storage_backends::mongodb::{MongoDBStorage, SaveGzipBlobParams};
use log::{error, trace};
use std::path::Path;
use uuid::Uuid;

#[tokio::main]
async fn main() -> std::io::Result<()> {
    let matches = build_cli();

    let command_args = matches.values_of("command").unwrap();
    let command_vec: Vec<&str> = command_args.collect();
    let (command, args) = command_vec.split_first().unwrap();
    let label = matches.value_of("label").unwrap_or("default_label");
    let use_mongodb = matches.is_present("use_mongodb");
    let run_uuid = Uuid::new_v4().to_string();

    let gzip_logger = setup_logger(label, &run_uuid, use_mongodb).expect("Failed to set up logger");

    let start_time = std::time::SystemTime::now();
    if let Err(e) = run_command(command, args).await {
        error!("Failed to run command: {} {}", command, args.join(" "));
        error!("Error: {}", e);
    }
    let end_time = std::time::SystemTime::now();
    let duration = match end_time.duration_since(start_time) {
        Ok(duration) => {
            if !use_mongodb {
                trace!(
                    "{} {} took {} to complete.",
                    command,
                    args.join(" "),
                    humantime::format_duration(duration)
                );
            }
            humantime::format_duration(duration).to_string()
        }
        Err(e) => {
            error!("Error calculating duration: {}", e);
            "unknown".to_string()
        }
    };

    if let Some(logger) = gzip_logger {
        logger.finish().expect("Failed to finalize GzipLogger");
    }

    if use_mongodb {
        let client = setup_mongodb().await?;
        list_databases(&client).await?;
        let mongodb_storage = MongoDBStorage::new(&client, "local", "commands")
            .await
            .expect("Failed to create MongoDB storage");

        // Store the gzip file in MongoDB
        let filename = format!("{}_{}.log.gz", label, run_uuid);
        let file_path = Path::new(&filename);

        let params = SaveGzipBlobParams {
            cmd: command,
            args: &args.join(" "),
            label,
            duration: &duration,
            uuid: &run_uuid,
            file_path,
        };

        mongodb_storage.save_gzip_blob(params).await?;
    }

    Ok(())
}
