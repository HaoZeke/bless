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
    #[arg(long)]
    pub use_mongodb: bool,

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
    #[cfg(feature = "serve")]
    #[arg(long, value_name = "ADDR")]
    pub serve: Option<String>,

    /// Stream logs to a remote bless server
    #[cfg(feature = "serve")]
    #[arg(long, value_name = "ADDR")]
    pub remote: Option<String>,

    /// Also write local gzip when using --remote
    #[cfg(feature = "serve")]
    #[arg(long)]
    pub local: bool,

    /// Command to run (after --)
    #[arg(required = true, last = true, num_args = 1..)]
    pub command: Vec<String>,
}
