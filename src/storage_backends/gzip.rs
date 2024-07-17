use flate2::write::GzEncoder;
use flate2::Compression;
use log::{Level, Log, Metadata, Record};
use std::fs::File;
use std::io::{self, Write};
use std::sync::{Arc, Mutex};

pub struct GzipLogWrapper {
    encoder: Arc<Mutex<Option<GzEncoder<File>>>>,
}

impl GzipLogWrapper {
    pub fn new(filename: &str) -> Self {
        let out_file = File::create(filename).unwrap();
        let encoder = GzEncoder::new(out_file, Compression::default());
        Self {
            encoder: Arc::new(Mutex::new(Some(encoder))),
        }
    }

    pub fn finish(&self) -> io::Result<()> {
        let mut encoder_lock = self.encoder.lock().unwrap();
        if let Some(encoder) = encoder_lock.take() {
            encoder.finish()?;
        }
        Ok(())
    }
}

impl Log for GzipLogWrapper {
    fn enabled(&self, metadata: &Metadata) -> bool {
        metadata.level() <= Level::Trace
    }

    fn log(&self, record: &Record) {
        if self.enabled(record.metadata()) {
            let mut encoder_lock = self.encoder.lock().unwrap();
            if let Some(ref mut encoder) = *encoder_lock {
                writeln!(
                    encoder,
                    "[{} {}] {}",
                    humantime::format_rfc3339_seconds(std::time::SystemTime::now()),
                    record.level(),
                    record.args()
                )
                .unwrap();
            }
        }
    }

    fn flush(&self) {
        let mut encoder_lock = self.encoder.lock().unwrap();
        if let Some(ref mut encoder) = *encoder_lock {
            encoder.flush().unwrap();
        }
    }
}

impl Clone for GzipLogWrapper {
    fn clone(&self) -> Self {
        GzipLogWrapper {
            encoder: Arc::clone(&self.encoder),
        }
    }
}
