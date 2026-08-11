use crate::error::BlessError;
use crate::storage_backends::{select_blob_storage, BlobStorageKind};
use log::trace;
use mongodb::bson::DateTime;
use mongodb::bson::{doc, Binary, Document};
use mongodb::options::IndexOptions;
use mongodb::{Client, Collection, Database, IndexModel};
use std::fs;
use std::path::Path;
use tokio_util::compat::TokioAsyncReadCompatExt;

#[derive(Clone, Debug)]
pub struct SaveGzipBlobParams<'a> {
    pub cmd: &'a str,
    pub args: &'a str,
    pub label: &'a str,
    pub duration: &'a str,
    pub uuid: &'a str,
    pub file_path: &'a Path,
    /// Combined gzip when `None` or `Some("")`; `"stdout"` / `"stderr"`
    /// when `--split`. Persist always writes `stream` (empty for combined).
    ///
    /// Split runs persist one document per stream (`insert_one` twice).
    /// Both blobs share `run_uuid` and differ on `stream`.
    pub stream: Option<&'a str>,
    pub start_time: DateTime,
    pub end_time: DateTime,
}

pub struct MongoDBStorage {
    collection: Collection<Document>,
    db: Database,
}

impl MongoDBStorage {
    pub async fn new(
        client: &Client,
        db_name: &str,
        collection_name: &str,
    ) -> Result<Self, BlessError> {
        let db = client.database(db_name);
        let collection: Collection<Document> = db.collection(collection_name);
        collection
            .create_index(
                IndexModel::builder()
                    .keys(doc! { "run_uuid": 1, "stream": 1 })
                    .options(IndexOptions::builder().unique(true).build())
                    .build(),
                None,
            )
            .await?;
        collection
            .create_index(
                IndexModel::builder()
                    .keys(doc! { "label": 1, "start_time": 1 })
                    .build(),
                None,
            )
            .await?;
        Ok(Self { collection, db })
    }

    /// Persist a finished gzip log.
    ///
    /// When `force_gridfs` is true, or the file exceeds
    /// [`crate::storage_backends::BSON_BLOB_SOFT_LIMIT`], the blob is streamed
    /// into GridFS from disk and the metadata document stores `gzip_blob_id`.
    /// Otherwise the bytes are embedded as `gzip_blob` Binary (legacy path).
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
            "stream": params.stream.unwrap_or(""),
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
                let file = tokio::fs::File::open(params.file_path).await?;
                let file_id = bucket
                    .upload_from_futures_0_3_reader(filename, file.compat(), None)
                    .await?;
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
