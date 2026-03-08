use crate::error::BlessError;
use mongodb::{
    options::{ClientOptions, ResolverConfig},
    Client,
};
use std::env;

pub async fn setup_mongodb() -> Result<Client, BlessError> {
    let client_uri = env::var("MONGODB_URI")
        .map_err(|_| BlessError::Config("MONGODB_URI environment variable not set".into()))?;

    let options =
        ClientOptions::parse_with_resolver_config(&client_uri, ResolverConfig::cloudflare())
            .await?;

    Ok(Client::with_options(options)?)
}

pub async fn list_databases(client: &Client) -> Result<(), BlessError> {
    let db_names = client.list_database_names(None, None).await?;

    println!("Databases:");
    for name in db_names {
        println!("- {}", name);
    }

    Ok(())
}
