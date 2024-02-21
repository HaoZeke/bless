mod cli;
mod runner;
mod storage;

use crate::cli::build_cli;
use crate::runner::run_command;
use crate::storage::{FileStorage, Storage};
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
        // MongoDB storage logic here
    } else {
        let filename = format!("{}_{}.out.gz", label, run_uuid);
        let file_storage = FileStorage::new(&filename);
        file_storage.save(&output_data);
    }

    Ok(())
}
