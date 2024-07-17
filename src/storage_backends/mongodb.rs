use mongodb::{
    bson::{doc, Binary, Document},
    Client, Collection,
};
use std::fs::File;
use std::io::Read;
use std::path::Path;
use tokio::io;

pub struct SaveGzipBlobParams<'a> {
    pub cmd: &'a str,
    pub args: &'a str,
    pub label: &'a str,
    pub duration: &'a str,
    pub uuid: &'a str,
    pub file_path: &'a Path,
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

    pub async fn save_gzip_blob(&self, params: SaveGzipBlobParams<'_>) -> io::Result<()> {
        let mut file = File::open(params.file_path)?;
        let mut buffer = Vec::new();
        file.read_to_end(&mut buffer)?;

        let doc = doc! {
            "cmd": params.cmd,
            "args": params.args,
            "label": params.label,
            "run_uuid": params.uuid,
            "duration": params.duration,
            "gzip_blob": Binary { subtype: mongodb::bson::spec::BinarySubtype::Generic, bytes: buffer },
        };

        self.collection
            .insert_one(doc, None)
            .await
            .map_err(|e| io::Error::new(io::ErrorKind::Other, e.to_string()))?;

        Ok(())
    }
}
