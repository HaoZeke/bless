use flate2::write::GzEncoder;
use flate2::Compression;
use std::fs::File;
use std::io::{self, Write};
use std::sync::{Arc, Mutex};

pub trait Storage {
    fn save(&self, data: &[String]);
    fn finish(&self) -> io::Result<()>;
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

impl Storage for FileStorage {
    fn save(&self, data: &[String]) {
        let mut encoder_guard = self.encoder.lock().unwrap();
        if let Some(ref mut encoder) = *encoder_guard {
            data.iter().for_each(|line| {
                writeln!(encoder, "{}", line).unwrap();
            });
        }
    }

    fn finish(&self) -> io::Result<()> {
        let mut encoder_guard = self.encoder.lock().unwrap();
        if let Some(encoder) = encoder_guard.take() {
            encoder.finish()?;
        }
        Ok(())
    }
}
