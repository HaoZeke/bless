use flate2::write::GzEncoder;
use flate2::Compression;
use log::{Level, Log, Metadata, Record};
use std::fs::File;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

pub struct GzipLogWrapper {
    encoder: Arc<Mutex<Option<GzEncoder<File>>>>,
    path: PathBuf,
}

impl GzipLogWrapper {
    pub fn new(filename: &str) -> io::Result<Self> {
        let path = PathBuf::from(filename);
        let out_file = File::create(&path)?;
        let encoder = GzEncoder::new(out_file, Compression::default());
        Ok(Self {
            encoder: Arc::new(Mutex::new(Some(encoder))),
            path,
        })
    }

    /// Path passed to [`File::create`] when this wrapper was opened.
    #[cfg_attr(not(feature = "mongodb"), allow(dead_code))]
    pub fn path(&self) -> &Path {
        &self.path
    }

    fn lock_encoder(&self) -> std::sync::MutexGuard<'_, Option<GzEncoder<File>>> {
        self.encoder
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    pub fn finish(&self) -> io::Result<()> {
        let mut encoder_lock = self.lock_encoder();
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
            let mut encoder_lock = self.lock_encoder();
            if let Some(ref mut encoder) = *encoder_lock {
                let _ = writeln!(
                    encoder,
                    "[{} {}] {}",
                    humantime::format_rfc3339_seconds(std::time::SystemTime::now()),
                    record.level(),
                    record.args()
                );
            }
        }
    }

    fn flush(&self) {
        let mut encoder_lock = self.lock_encoder();
        if let Some(ref mut encoder) = *encoder_lock {
            let _ = encoder.flush();
        }
    }
}

impl Clone for GzipLogWrapper {
    fn clone(&self) -> Self {
        GzipLogWrapper {
            encoder: Arc::clone(&self.encoder),
            path: self.path.clone(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use flate2::read::GzDecoder;
    use log::{Level, Record};
    use std::io::Read;

    #[test]
    fn finish_then_gunzip_yields_timestamped_level_line() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("wrap.log.gz");
        let wrapper = GzipLogWrapper::new(path.to_str().unwrap()).unwrap();

        wrapper.log(
            &Record::builder()
                .args(format_args!("hello-gzip"))
                .level(Level::Info)
                .target("bless")
                .module_path(Some("bless"))
                .file(Some("gzip.rs"))
                .line(Some(1))
                .build(),
        );
        wrapper.finish().unwrap();

        let mut text = String::new();
        GzDecoder::new(File::open(&path).unwrap())
            .read_to_string(&mut text)
            .unwrap();
        let line = text.lines().next().expect("expected a log line");

        let rest = line
            .strip_prefix('[')
            .and_then(|s| s.split_once("] "))
            .expect("expected [timestamp LEVEL] msg");
        let (inside, msg) = rest;
        let mut parts = inside.split_whitespace();
        let ts = parts.next().expect("timestamp");
        let level = parts.next().expect("LEVEL");
        assert!(
            parts.next().is_none(),
            "extra fields inside brackets: {inside}"
        );
        assert!(ts.contains('T'), "rfc3339 timestamp: {ts}");
        assert_eq!(level, "INFO");
        assert_eq!(msg, "hello-gzip");
    }

    fn poison_encoder(wrapper: &GzipLogWrapper) {
        let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let _guard = wrapper.encoder.lock().unwrap();
            panic!("poison encoder mutex");
        }));
        assert!(wrapper.encoder.lock().is_err());
    }

    #[test]
    fn log_and_flush_recover_from_poison() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("poison.log.gz");
        let wrapper = GzipLogWrapper::new(path.to_str().unwrap()).unwrap();
        poison_encoder(&wrapper);
        wrapper.log(
            &Record::builder()
                .args(format_args!("after poison"))
                .level(Level::Info)
                .target("bless")
                .build(),
        );
        wrapper.flush();
        wrapper.finish().unwrap();
        assert!(path.exists());
        assert!(path.metadata().unwrap().len() > 0);
    }
}
