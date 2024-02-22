use async_trait::async_trait;
use flate2::write::GzEncoder;
use flate2::Compression;
use mongodb::{bson::doc, bson::Document, Client, Collection};
use std::fs::File;
use std::io::{self, Write};
use std::sync::{Arc, Mutex};

#[async_trait]
pub trait Storage {
    async fn save(&self, data: &[String]);
    async fn finish(&self) -> io::Result<()>;
}

pub struct FileStorage {
    encoder: Arc<Mutex<Option<GzEncoder<File>>>>,
}

impl FileStorage {
    pub fn new(filename: &str) -> Self {
        let out_file = File::create(filename).unwrap();
        let encoder = GzEncoder::new(out_file, Compression::default());
        Self {
            encoder: Arc::new(Mutex::new(Some(encoder))),
        }
    }
}

#[async_trait]
impl Storage for FileStorage {
    async fn save(&self, data: &[String]) {
        let mut encoder = self.encoder.lock().unwrap();
        if let Some(ref mut encoder) = *encoder {
            for line in data {
                writeln!(encoder, "{}", line).unwrap();
            }
        }
    }

    async fn finish(&self) -> io::Result<()> {
        let encoder = self.encoder.lock().unwrap().take();
        if let Some(encoder) = encoder {
            tokio::task::spawn_blocking(move || encoder.finish())
                .await?
                .map(|_| ())
                .map_err(Into::into)
        } else {
            Ok(())
        }
    }
}

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
    async fn save(&self, data: &[String]) {
        let output = data.join("\n");
        let doc = doc! {
            "label": "your_label_here",
            "run_uuid": "your_run_uuid_here",
            "cmd_output": output,
        };

        match self.collection.insert_one(doc, None).await {
            Ok(insert_result) => {
                // Successfully inserted document, print the new document ID
                if let bson::Bson::ObjectId(oid) = insert_result.inserted_id {
                    println!("New document ID: {}", oid);
                } else {
                    eprintln!("Failed to retrieve document ID");
                }
            }
            Err(e) => {
                // Error occurred while inserting document
                eprintln!("Error saving to MongoDB: {}", e);
            }
        }
    }
    async fn finish(&self) -> io::Result<()> {
        Ok(())
    }
}
