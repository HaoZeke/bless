mod cli;
mod db;
mod runner;
mod storage;

use crate::cli::build_cli;
use crate::db::{list_databases, setup_mongodb};
use crate::runner::run_command;
use crate::storage::{FileStorage, MongoDBStorage, Storage};
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
    let output_data = run_command(command, args);

    if use_mongodb {
        let client = setup_mongodb().await?;
        list_databases(&client).await?;
        let mongodb_storage = MongoDBStorage::new(&client, "local", "commands")
            .await
            .expect("Failed to create MongoDB storage");
        mongodb_storage.save(&output_data).await;
    } else {
        let filename = format!("{}_{}.out.gz", label, run_uuid);
        let file_storage = FileStorage::new(&filename);
        file_storage.save(&output_data).await;
        file_storage.finish().await.expect("Closing GZip failed");
    }

    Ok(())
}
