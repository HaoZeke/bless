use clap::{Parser, ValueEnum};

#[derive(Debug, Clone, ValueEnum, Default)]
pub enum OutputFormat {
    #[default]
    Log,
    Jsonl,
}

#[derive(Parser, Debug)]
#[command(name = "bless", version = env!("CARGO_PKG_VERSION"), about = "Runs a command and logs output with metadata tracking")]
pub struct Cli {
    /// Label for the run
    #[arg(long, default_value = "default_label")]
    pub label: String,

    /// Store output in MongoDB
    #[cfg(feature = "mongodb")]
    #[arg(long)]
    pub use_mongodb: bool,

    /// MongoDB database name
    #[cfg(feature = "mongodb")]
    #[arg(long, default_value = "bless", env = "MONGODB_DB")]
    pub db: String,

    /// MongoDB collection name
    #[cfg(feature = "mongodb")]
    #[arg(long, default_value = "commands", env = "MONGODB_COLLECTION")]
    pub collection: String,

    /// Force GridFS for the gzip blob even when it fits in a BSON document
    #[cfg(feature = "mongodb")]
    #[arg(long)]
    pub force_gridfs: bool,

    /// Omit timestamps from stdout (gzip file keeps them)
    #[arg(long)]
    pub no_timestamp: bool,

    /// Output format for stdout
    #[arg(long, default_value = "log", value_enum)]
    pub format: OutputFormat,

    /// Output file path (default: {label}_{uuid}.log.gz). Use "-" for stdout only.
    #[arg(short, long)]
    pub output: Option<String>,

    /// Write separate stdout/stderr gzip files
    #[arg(long)]
    pub split: bool,

    /// Start serve mode (capnp log aggregation server)
    #[cfg(all(feature = "serve", feature = "mongodb"))]
    #[arg(long, value_name = "ADDR", conflicts_with_all = ["remote", "use_mongodb"])]
    pub serve: Option<String>,

    /// Start serve mode (capnp log aggregation server)
    #[cfg(all(feature = "serve", not(feature = "mongodb")))]
    #[arg(long, value_name = "ADDR", conflicts_with = "remote")]
    pub serve: Option<String>,

    /// Stream logs to a remote bless server
    #[cfg(feature = "serve")]
    #[arg(long, value_name = "ADDR")]
    pub remote: Option<String>,

    /// Also write local gzip when using --remote
    #[cfg(feature = "serve")]
    #[arg(long, requires = "remote")]
    pub local: bool,

    /// Command to run (after --)
    #[cfg(feature = "serve")]
    #[arg(required_unless_present = "serve", last = true, num_args = 1..)]
    pub command: Vec<String>,

    /// Command to run (after --)
    #[cfg(not(feature = "serve"))]
    #[arg(required = true, last = true, num_args = 1..)]
    pub command: Vec<String>,
}

impl Cli {
    /// Path handed to the gzip logger.
    ///
    /// `--remote` without `--local` and without `-o` skips the local gzip
    /// (same as `-o -`). An explicit `-o` still writes that path.
    pub fn gzip_output(&self) -> Option<&str> {
        #[cfg(feature = "serve")]
        if self.remote.is_some() && !self.local && self.output.is_none() {
            return Some("-");
        }
        self.output.as_deref()
    }
}
