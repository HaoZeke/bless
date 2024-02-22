use crate::storage::Storage;
use async_trait::async_trait;
use mongodb::{
    bson::{doc, Document},
    Client, Collection,
};
use tokio::io;

pub struct MongoDBStorage {
    collection: Collection<Document>,
}

impl MongoDBStorage {
    pub async fn new(client: &Client, db_name: &str, collection_name: &str) -> io::Result<Self> {
        let db = client.database(db_name);
        let collection: Collection<Document> = db.collection(collection_name);
        Ok(Self { collection })
    }
}

#[async_trait]
impl Storage for MongoDBStorage {
    async fn save(&self, label: &str, uuid: &str, data: &[String]) -> io::Result<()> {
        let output = data.join("\n");
        let doc = doc! {
            "label": label,
            "run_uuid": uuid,
            "cmd_output": output,
        };

        self.collection
            .insert_one(doc, None)
            .await
            .map_err(|e| io::Error::new(io::ErrorKind::Other, e.to_string()))?;

        Ok(())
    }

    async fn finish(&self) -> io::Result<()> {
        // No specific actions needed for MongoDB storage upon finish
        Ok(())
    }
}
