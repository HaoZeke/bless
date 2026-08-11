use crate::error::BlessError;
use mongodb::{
    options::{ClientOptions, ResolverConfig},
    Client,
};
use std::env;

pub async fn setup_mongodb() -> Result<Client, BlessError> {
    let client_uri = env::var("MONGODB_URI")
        .map_err(|_| BlessError::Config("MONGODB_URI environment variable not set".into()))?;

    let options = if client_uri.starts_with("mongodb+srv") {
        ClientOptions::parse_with_resolver_config(&client_uri, ResolverConfig::cloudflare()).await?
    } else {
        ClientOptions::parse(&client_uri).await?
    };

    Ok(Client::with_options(options)?)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::error::BlessError;

    #[tokio::test]
    async fn setup_mongodb_without_uri_is_config_error() {
        static LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());
        let _guard = LOCK.lock().expect("env lock");
        // SAFETY: exclusive lock; no other test in this crate reads MONGODB_URI
        // during the call. env::remove_var is unsafe on rustc 1.87+.
        #[allow(unused_unsafe)]
        unsafe {
            std::env::remove_var("MONGODB_URI");
        }
        let err = setup_mongodb().await.unwrap_err();
        match err {
            BlessError::Config(msg) => {
                assert!(msg.contains("MONGODB_URI"), "{msg}");
            }
            other => panic!("expected Config, got {other:?}"),
        }
    }
}
