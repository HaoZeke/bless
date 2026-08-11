use crate::error::BlessError;
use crate::storage_backends::{select_blob_storage, BlobStorageKind};
use futures_util::AsyncWriteExt;
use log::trace;
use mongodb::bson::DateTime;
use mongodb::bson::{doc, Binary, Document};
use mongodb::{Client, Collection, Database};
use std::fs;
use std::path::Path;

#[derive(Clone, Debug)]
pub struct SaveGzipBlobParams<'a> {
    pub cmd: &'a str,
    pub args: &'a str,
    pub label: &'a str,
    pub duration: &'a str,
    pub uuid: &'a str,
    pub file_path: &'a Path,
    pub start_time: DateTime,
    pub end_time: DateTime,
}

pub struct MongoDBStorage {
    collection: Collection<Document>,
    db: Database,
}

impl MongoDBStorage {
    pub async fn new(client: &Client, db_name: &str, collection_name: &str) -> Self {
        let db = client.database(db_name);
        let collection: Collection<Document> = db.collection(collection_name);
        Self { collection, db }
    }

    /// Persist a finished gzip log.
    ///
    /// When `force_gridfs` is true, or the file exceeds
    /// [`crate::storage_backends::BSON_BLOB_SOFT_LIMIT`], the blob is uploaded
    /// via GridFS and the metadata document stores `gzip_blob_id`. Otherwise
    /// the bytes are embedded as `gzip_blob` Binary (legacy path).
    pub async fn save_gzip_blob(
        &self,
        params: SaveGzipBlobParams<'_>,
        force_gridfs: bool,
    ) -> Result<(), BlessError> {
        let file_size = fs::metadata(params.file_path)?.len();
        let kind = select_blob_storage(file_size, force_gridfs);

        let mut doc = doc! {
            "cmd": params.cmd,
            "args": params.args,
            "label": params.label,
            "run_uuid": params.uuid,
            "duration": params.duration,
            "start_time": params.start_time,
            "end_time": params.end_time,
            "size_bytes": file_size as i64,
        };

        match kind {
            BlobStorageKind::GridFs => {
                let filename = params
                    .file_path
                    .file_name()
                    .and_then(|s| s.to_str())
                    .unwrap_or("bless.log.gz");
                trace!(
                    "storing log via GridFS (size={} bytes, force={})",
                    file_size,
                    force_gridfs
                );
                let bucket = self.db.gridfs_bucket(None);
                let file_bytes = fs::read(params.file_path)?;
                let mut upload_stream = bucket.open_upload_stream(filename, None);
                upload_stream.write_all(&file_bytes).await?;
                upload_stream.close().await?;
                let file_id = upload_stream.id().clone();
                doc.insert("storage", "gridfs");
                doc.insert("gzip_blob_id", file_id);
            }
            BlobStorageKind::BsonBinary => {
                trace!("storing log as BSON Binary (size={} bytes)", file_size);
                let buffer = fs::read(params.file_path)?;
                doc.insert("storage", "bson");
                doc.insert(
                    "gzip_blob",
                    Binary {
                        subtype: mongodb::bson::spec::BinarySubtype::Generic,
                        bytes: buffer,
                    },
                );
            }
        }

        self.collection.insert_one(doc, None).await?;
        Ok(())
    }
}
