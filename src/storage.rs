use async_trait::async_trait;
use flate2::write::GzEncoder;
use flate2::Compression;
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
