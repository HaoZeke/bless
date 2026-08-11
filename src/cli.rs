use clap::{Parser, Subcommand, ValueEnum};

#[derive(Debug, Clone, ValueEnum, Default)]
pub(crate) enum OutputFormat {
    #[default]
    Log,
    Jsonl,
}

#[derive(Parser, Debug)]
#[command(
    name = "bless",
    version = env!("CARGO_PKG_VERSION"),
    about = "Runs a command and logs output with metadata tracking",
    subcommand_negates_reqs = true
)]
pub(crate) struct Cli {
    /// Label for the run
    #[arg(long, default_value = "default_label")]
    pub label: String,

    /// Store output in MongoDB
    #[cfg(feature = "mongodb")]
    #[arg(long, global = true)]
    pub use_mongodb: bool,

    /// MongoDB database name
    #[cfg(feature = "mongodb")]
    #[arg(long, default_value = "bless", env = "MONGODB_DB", global = true)]
    pub db: String,

    /// MongoDB collection name
    #[cfg(feature = "mongodb")]
    #[arg(
        long,
        default_value = "commands",
        env = "MONGODB_COLLECTION",
        global = true
    )]
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

    /// Query stored runs (`ls`, `show`, `fetch`) instead of wrapping a command
    #[command(subcommand)]
    pub query: Option<QueryCommand>,

    /// Command to run (after --)
    #[cfg(feature = "serve")]
    #[arg(required_unless_present = "serve", last = true, num_args = 1..)]
    pub command: Vec<String>,

    /// Command to run (after --)
    #[cfg(not(feature = "serve"))]
    #[arg(required = true, last = true, num_args = 1..)]
    pub command: Vec<String>,
}

/// Inspect gzip runs in the current directory, or MongoDB with `--use-mongodb`.
#[derive(Subcommand, Debug, Clone, PartialEq, Eq)]
pub(crate) enum QueryCommand {
    /// List gzip runs
    Ls,
    /// Show metadata for a run (full uuid or unique prefix)
    Show {
        /// Full `run_uuid` or a unique prefix
        id: String,
    },
    /// Copy a run's gzip log to a file or stdout
    Fetch {
        /// Full `run_uuid` or a unique prefix
        id: String,
        /// Destination path. `-` writes gzip bytes to stdout.
        ///
        /// Default: `{uuid}.log.gz`, or `{uuid}_stdout.log.gz` and
        /// `{uuid}_stderr.log.gz` when the run is split.
        #[arg(short, long)]
        output: Option<String>,
    },
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

#[cfg(test)]
mod tests {
    use super::{Cli, QueryCommand};
    use clap::Parser;

    #[cfg(feature = "mongodb")]
    static MONGO_DEST_ENV: std::sync::Mutex<()> = std::sync::Mutex::new(());

    #[test]
    fn parse_basic() {
        let cli = Cli::try_parse_from(["bless", "--label", "test", "--", "echo", "hi"]);
        assert!(cli.is_ok());
        let cli = cli.unwrap();
        assert_eq!(cli.label, "test");
        assert_eq!(cli.command, vec!["echo", "hi"]);
        assert!(cli.query.is_none());
        assert!(!cli.no_timestamp);
        assert!(!cli.split);
        #[cfg(feature = "mongodb")]
        assert!(!cli.force_gridfs);
    }

    #[test]
    fn parse_all_flags() {
        let cli = Cli::try_parse_from([
            "bless",
            "--label",
            "myrun",
            "--no-timestamp",
            "--format",
            "jsonl",
            "--split",
            "-o",
            "/tmp/out.log.gz",
            "--",
            "make",
            "-j8",
        ]);
        assert!(cli.is_ok());
        let cli = cli.unwrap();
        assert_eq!(cli.label, "myrun");
        assert!(cli.no_timestamp);
        assert!(cli.split);
        assert_eq!(cli.output, Some("/tmp/out.log.gz".into()));
        assert_eq!(cli.command, vec!["make", "-j8"]);
    }

    #[cfg(feature = "mongodb")]
    #[test]
    fn parse_force_gridfs() {
        let cli = Cli::try_parse_from([
            "bless",
            "--use-mongodb",
            "--force-gridfs",
            "--",
            "echo",
            "hi",
        ]);
        assert!(cli.is_ok());
        let cli = cli.unwrap();
        assert!(cli.use_mongodb);
        assert!(cli.force_gridfs);
        assert_eq!(cli.command, vec!["echo", "hi"]);
    }

    #[cfg(not(feature = "mongodb"))]
    #[test]
    fn rejects_mongodb_flags_without_feature() {
        assert!(Cli::try_parse_from(["bless", "--use-mongodb", "--", "echo", "hi"]).is_err());
        assert!(Cli::try_parse_from(["bless", "--force-gridfs", "--", "echo", "hi"]).is_err());
        assert!(Cli::try_parse_from(["bless", "--db", "x", "--", "echo", "hi"]).is_err());
        assert!(Cli::try_parse_from(["bless", "--collection", "x", "--", "echo", "hi"]).is_err());
    }

    #[test]
    fn requires_command() {
        let cli = Cli::try_parse_from(["bless"]);
        assert!(cli.is_err());
    }

    #[test]
    fn parse_ls_subcommand() {
        let cli = Cli::try_parse_from(["bless", "ls"]);
        assert!(cli.is_ok());
        let cli = cli.unwrap();
        assert_eq!(cli.query, Some(QueryCommand::Ls));
        assert!(cli.command.is_empty());
    }

    #[test]
    fn parse_wrap_after_double_dash() {
        let cli = Cli::try_parse_from(["bless", "--", "echo"]).unwrap();
        assert!(cli.query.is_none());
        assert_eq!(cli.command, vec!["echo"]);
    }

    #[test]
    fn double_dash_ls_is_wrap_not_subcommand() {
        let cli = Cli::try_parse_from(["bless", "--", "ls"]).unwrap();
        assert!(cli.query.is_none());
        assert_eq!(cli.command, vec!["ls"]);
    }

    #[test]
    fn parse_show_id() {
        let cli = Cli::try_parse_from(["bless", "show", "abc"]).unwrap();
        assert_eq!(cli.query, Some(QueryCommand::Show { id: "abc".into() }));
        assert!(cli.command.is_empty());
    }

    #[test]
    fn parse_fetch_output() {
        let cli = Cli::try_parse_from(["bless", "fetch", "abc", "-o", "-"]).unwrap();
        assert_eq!(
            cli.query,
            Some(QueryCommand::Fetch {
                id: "abc".into(),
                output: Some("-".into()),
            })
        );
        assert!(cli.command.is_empty());
    }

    #[test]
    fn show_requires_id() {
        assert!(Cli::try_parse_from(["bless", "show"]).is_err());
    }

    #[cfg(feature = "mongodb")]
    #[test]
    fn parse_ls_use_mongodb_after_subcommand() {
        let cli = Cli::try_parse_from(["bless", "ls", "--use-mongodb"]).unwrap();
        assert!(cli.use_mongodb);
        assert_eq!(cli.query, Some(QueryCommand::Ls));
        assert!(cli.command.is_empty());
    }

    #[cfg(feature = "mongodb")]
    #[test]
    fn parse_ls_use_mongodb_before_subcommand() {
        let cli = Cli::try_parse_from(["bless", "--use-mongodb", "ls"]).unwrap();
        assert!(cli.use_mongodb);
        assert_eq!(cli.query, Some(QueryCommand::Ls));
    }

    #[cfg(feature = "mongodb")]
    #[test]
    fn parse_ls_db_collection() {
        let _guard = MONGO_DEST_ENV.lock().unwrap();
        // SAFETY: exclusive MONGO_DEST_ENV lock.
        unsafe {
            set_mongo_dest_env(None, None);
        }
        let cli = Cli::try_parse_from([
            "bless",
            "ls",
            "--use-mongodb",
            "--db",
            "otherdb",
            "--collection",
            "othercol",
        ])
        .unwrap();
        assert_eq!(cli.db, "otherdb");
        assert_eq!(cli.collection, "othercol");
        assert_eq!(cli.query, Some(QueryCommand::Ls));
    }

    #[cfg(feature = "serve")]
    #[test]
    fn serve_without_command() {
        let cli = Cli::try_parse_from(["bless", "--serve", ":9000"]);
        assert!(cli.is_ok());
        let cli = cli.unwrap();
        assert_eq!(cli.serve.as_deref(), Some(":9000"));
        assert!(cli.command.is_empty());
        assert!(cli.remote.is_none());
    }

    #[cfg(feature = "serve")]
    #[test]
    fn serve_conflicts_with_remote() {
        let cli = Cli::try_parse_from([
            "bless", "--serve", ":9000", "--remote", ":9001", "--", "true",
        ]);
        assert!(cli.is_err());
    }

    #[cfg(all(feature = "serve", feature = "mongodb"))]
    #[test]
    fn serve_conflicts_with_mongodb() {
        let cli = Cli::try_parse_from(["bless", "--serve", ":9000", "--use-mongodb"]);
        assert!(cli.is_err());
    }

    #[cfg(feature = "serve")]
    #[test]
    fn local_requires_remote() {
        let cli = Cli::try_parse_from(["bless", "--local", "--", "true"]);
        assert!(cli.is_err());
    }

    #[cfg(feature = "serve")]
    #[test]
    fn remote_requires_command() {
        let cli = Cli::try_parse_from(["bless", "--remote", ":9000"]);
        assert!(cli.is_err());

        let cli = Cli::try_parse_from(["bless", "--remote", ":9000", "--", "echo", "hi"]);
        assert!(cli.is_ok());
        let cli = cli.unwrap();
        assert_eq!(cli.remote.as_deref(), Some(":9000"));
        assert_eq!(cli.command, vec!["echo", "hi"]);
        assert!(!cli.local);
    }

    #[cfg(feature = "serve")]
    #[test]
    fn remote_without_local_skips_default_gzip() {
        let cli = Cli::try_parse_from(["bless", "--remote", ":9", "--", "true"]).unwrap();
        assert_eq!(cli.gzip_output(), Some("-"));
        assert!(cli.output.is_none());
    }

    #[cfg(feature = "serve")]
    #[test]
    fn remote_with_local_uses_default_gzip() {
        let cli =
            Cli::try_parse_from(["bless", "--remote", ":9", "--local", "--", "true"]).unwrap();
        assert_eq!(cli.gzip_output(), None);
        assert!(cli.local);
    }

    #[cfg(feature = "serve")]
    #[test]
    fn remote_with_dash_o_keeps_path() {
        let cli =
            Cli::try_parse_from(["bless", "--remote", ":9", "-o", "x.gz", "--", "true"]).unwrap();
        assert_eq!(cli.gzip_output(), Some("x.gz"));
        assert!(!cli.local);
    }

    #[cfg(feature = "mongodb")]
    #[test]
    fn parses_mongodb_with_dash_output() {
        // Clap accepts the combination; persist rejects it as BlessError::Config
        // because -o - opens no gzip to upload.
        let cli = Cli::try_parse_from(["bless", "--use-mongodb", "-o", "-", "--", "echo", "hi"]);
        assert!(cli.is_ok());
        let cli = cli.unwrap();
        assert!(cli.use_mongodb);
        assert_eq!(cli.output.as_deref(), Some("-"));
    }

    // SAFETY: caller holds MONGO_DEST_ENV. env::{set,remove}_var is unsafe
    // on rustc 1.87+.
    #[cfg(feature = "mongodb")]
    #[allow(unused_unsafe)]
    unsafe fn set_mongo_dest_env(db: Option<&str>, collection: Option<&str>) {
        unsafe {
            match db {
                Some(v) => std::env::set_var("MONGODB_DB", v),
                None => std::env::remove_var("MONGODB_DB"),
            }
            match collection {
                Some(v) => std::env::set_var("MONGODB_COLLECTION", v),
                None => std::env::remove_var("MONGODB_COLLECTION"),
            }
        }
    }

    #[cfg(feature = "mongodb")]
    #[test]
    fn mongo_dest_defaults() {
        let _guard = MONGO_DEST_ENV.lock().unwrap();
        // SAFETY: exclusive MONGO_DEST_ENV lock.
        unsafe {
            set_mongo_dest_env(None, None);
        }
        let cli = Cli::try_parse_from(["bless", "--use-mongodb", "--", "echo", "hi"]).unwrap();
        assert_eq!(cli.db, "bless");
        assert_eq!(cli.collection, "commands");
    }

    #[cfg(feature = "mongodb")]
    #[test]
    fn mongo_dest_flags() {
        let _guard = MONGO_DEST_ENV.lock().unwrap();
        // SAFETY: exclusive MONGO_DEST_ENV lock.
        unsafe {
            set_mongo_dest_env(Some("fromenv"), Some("fromenvcol"));
        }
        let cli = Cli::try_parse_from([
            "bless",
            "--db",
            "mydb",
            "--collection",
            "mycoll",
            "--",
            "echo",
            "hi",
        ])
        .unwrap();
        assert_eq!(cli.db, "mydb");
        assert_eq!(cli.collection, "mycoll");
        // SAFETY: exclusive MONGO_DEST_ENV lock.
        unsafe {
            set_mongo_dest_env(None, None);
        }
    }

    #[cfg(feature = "mongodb")]
    #[test]
    fn mongo_dest_env() {
        let _guard = MONGO_DEST_ENV.lock().unwrap();
        // SAFETY: exclusive MONGO_DEST_ENV lock.
        unsafe {
            set_mongo_dest_env(Some("envdb"), Some("envcoll"));
        }
        let cli = Cli::try_parse_from(["bless", "--use-mongodb", "--", "echo", "hi"]).unwrap();
        assert_eq!(cli.db, "envdb");
        assert_eq!(cli.collection, "envcoll");
        // SAFETY: exclusive MONGO_DEST_ENV lock.
        unsafe {
            set_mongo_dest_env(None, None);
        }
    }
}
