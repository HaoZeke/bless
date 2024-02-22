mod cli;
mod runner;
mod storage;

use crate::cli::build_cli;
use crate::runner::run_command;
use crate::storage::{FileStorage, Storage};
use uuid::Uuid;

use mongodb::{
    options::{ClientOptions, ResolverConfig},
    Client,
};
use std::{env, io};

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
        let client_uri =
            env::var("MONGODB_URI").expect("You must set the MONGODB_URI environment var!");

        let options_result =
            ClientOptions::parse_with_resolver_config(&client_uri, ResolverConfig::cloudflare())
                .await;
        let options =
            options_result.map_err(|e| io::Error::new(io::ErrorKind::Other, e.to_string()))?;

        let client_result = Client::with_options(options);
        let client =
            client_result.map_err(|e| io::Error::new(io::ErrorKind::Other, e.to_string()))?;

        println!("Databases:");
        let db_names = client
            .list_database_names(None, None)
            .await
            .map_err(|e| io::Error::new(io::ErrorKind::Other, e.to_string()))?;
        for name in db_names {
            println!("- {}", name);
        }
    } else {
        let filename = format!("{}_{}.out.gz", label, run_uuid);
        let file_storage = FileStorage::new(&filename);
        file_storage.save(&output_data).await;
        file_storage.finish().await.expect("Closing GZip failed");
    }

    Ok(())
}
