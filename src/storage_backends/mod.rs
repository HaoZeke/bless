//! Pluggable log storage backends.
//!
//! Gzip files are always written locally first (unless `-o -`). Optional
//! MongoDB persistence then either embeds the blob in a BSON document
//! (under the 16 MiB document limit) or uploads via GridFS for larger logs.

pub mod gzip;
#[cfg(feature = "mongodb")]
pub mod mongodb;

/// Soft ceiling for embedding a gzip blob in a BSON document.
///
/// MongoDB documents cap at 16 MiB; stay under that with headroom for
/// metadata fields on the same document.
pub const BSON_BLOB_SOFT_LIMIT: u64 = 15 * 1024 * 1024;

/// How a finished gzip log should be persisted in MongoDB.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BlobStorageKind {
    /// Store bytes as a `Binary` field on the run metadata document.
    BsonBinary,
    /// Upload to GridFS; store only the file id on the metadata document.
    GridFs,
}

/// Choose storage based on file size and an explicit GridFS request.
pub fn select_blob_storage(file_size: u64, force_gridfs: bool) -> BlobStorageKind {
    if force_gridfs || file_size > BSON_BLOB_SOFT_LIMIT {
        BlobStorageKind::GridFs
    } else {
        BlobStorageKind::BsonBinary
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn small_blob_uses_bson() {
        assert_eq!(
            select_blob_storage(1024, false),
            BlobStorageKind::BsonBinary
        );
    }

    #[test]
    fn large_blob_auto_gridfs() {
        assert_eq!(
            select_blob_storage(BSON_BLOB_SOFT_LIMIT + 1, false),
            BlobStorageKind::GridFs
        );
    }

    #[test]
    fn force_gridfs_overrides_size() {
        assert_eq!(select_blob_storage(1, true), BlobStorageKind::GridFs);
    }

    #[test]
    fn boundary_at_soft_limit_stays_bson() {
        assert_eq!(
            select_blob_storage(BSON_BLOB_SOFT_LIMIT, false),
            BlobStorageKind::BsonBinary
        );
    }
}
