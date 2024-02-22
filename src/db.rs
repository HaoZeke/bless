use mongodb::{
    options::{ClientOptions, ResolverConfig},
    Client,
};
use std::env;
use std::io;

pub async fn setup_mongodb() -> io::Result<Client> {
    let client_uri =
        env::var("MONGODB_URI").expect("You must set the MONGODB_URI environment var!");

    let options =
        ClientOptions::parse_with_resolver_config(&client_uri, ResolverConfig::cloudflare())
            .await
            .map_err(|e| io::Error::new(io::ErrorKind::Other, e.to_string()))?;

    Client::with_options(options).map_err(|e| io::Error::new(io::ErrorKind::Other, e.to_string()))
}

pub async fn list_databases(client: &Client) -> io::Result<()> {
    let db_names = client
        .list_database_names(None, None)
        .await
        .map_err(|e| io::Error::new(io::ErrorKind::Other, e.to_string()))?;

    println!("Databases:");
    for name in db_names {
        println!("- {}", name);
    }

    Ok(())
}
