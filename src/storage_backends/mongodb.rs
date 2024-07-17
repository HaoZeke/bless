use futures_util::AsyncWriteExt;
use log::{error, trace};
use mongodb::bson::serde_helpers::bson_datetime_as_rfc3339_string;
use mongodb::bson::{doc, Binary, DateTime, Document};
use mongodb::{Client, Collection, Database};
use serde::{Deserialize, Serialize};
use std::fs;
use std::io::Read;
use std::path::Path;
use tokio::io;

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct SaveGzipBlobParams<'a> {
    pub cmd: &'a str,
    pub args: &'a str,
    pub label: &'a str,
    pub duration: &'a str,
    pub uuid: &'a str,
    pub file_path: &'a Path,
    #[serde(with = "bson_datetime_as_rfc3339_string")]
    pub start_time: DateTime,
    #[serde(with = "bson_datetime_as_rfc3339_string")]
    pub end_time: DateTime,
}

pub struct MongoDBStorage {
    collection: Collection<Document>,
    db: Database,
}

impl MongoDBStorage {
    pub async fn new(client: &Client, db_name: &str, collection_name: &str) -> io::Result<Self> {
        let db = client.database(db_name);
        let collection: Collection<Document> = db.collection(collection_name);
        Ok(Self { collection, db })
    }

    pub async fn save_gzip_blob(&self, params: SaveGzipBlobParams<'_>) -> io::Result<()> {
        let fsize = fs::metadata(params.file_path)?.len();
        trace!("Filesize is {}", fsize);

        let doc = if fsize > 15 * 1024 * 1024 {
            // GridFS since bson size must be less than 16MB
            let bucket = self.db.gridfs_bucket(None);
            let file_bytes = fs::read(params.file_path)?;

            let mut upload_stream = bucket.open_upload_stream(
                params.file_path.file_name().unwrap().to_str().unwrap(),
                None,
            );

            upload_stream.write_all(&file_bytes).await?;
            upload_stream.close().await?;

            error!("Working");

            let file_id = upload_stream.id();

            doc! {
                "cmd": params.cmd,
                "args": params.args,
                "label": params.label,
                "run_uuid": params.uuid,
                "duration": params.duration,
                "start_time": params.start_time,
                "end_time": params.end_time,
                "gzip_blob_id": file_id,
            }
        } else {
            // Store directly in document if size is under 15MB
            let mut file = fs::File::open(params.file_path)?;
            let mut buffer = Vec::new();
            file.read_to_end(&mut buffer)?;
            error!("not right");
            doc! {
                "cmd": params.cmd,
                "args": params.args,
                "label": params.label,
                "run_uuid": params.uuid,
                "duration": params.duration,
                "start_time": params.start_time,
                "end_time": params.end_time,
                "gzip_blob": Binary { subtype: mongodb::bson::spec::BinarySubtype::Generic, bytes: buffer },
            }
        };

        self.collection
            .insert_one(doc, None)
            .await
            .map_err(|e| io::Error::new(io::ErrorKind::Other, e.to_string()))?;

        Ok(())
    }
}
