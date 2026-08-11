#[cfg(test)]
mod tests {
    use std::sync::{Mutex, Once};

    use bless::runner::run_command;

    static CAPTURED: Mutex<Vec<String>> = Mutex::new(Vec::new());
    static LOGGER: Once = Once::new();
    #[cfg(feature = "mongodb")]
    static MONGO_DEST_ENV: Mutex<()> = Mutex::new(());

    struct CaptureLogger;

    impl log::Log for CaptureLogger {
        fn enabled(&self, _metadata: &log::Metadata) -> bool {
            true
        }

        fn log(&self, record: &log::Record) {
            CAPTURED.lock().unwrap().push(record.args().to_string());
        }

        fn flush(&self) {}
    }

    fn init_capture_logger() {
        LOGGER.call_once(|| {
            let _ = log::set_boxed_logger(Box::new(CaptureLogger));
            log::set_max_level(log::LevelFilter::Info);
        });
    }

    fn captured_lines() -> Vec<String> {
        CAPTURED.lock().unwrap().clone()
    }

    #[tokio::test]
    async fn test_successful_command() {
        let result = run_command("echo", &["hello".into()]).await;
        assert!(result.is_ok(), "Expected command to succeed");
        let status = result.unwrap();
        assert!(status.success());
    }

    #[tokio::test]
    async fn test_true_command() {
        let result = run_command("true", &[]).await;
        assert!(result.is_ok(), "Expected true to succeed");
        let status = result.unwrap();
        assert!(status.success());
        assert_eq!(bless::runner::exit_code_from_status(status), 0);
    }

    #[tokio::test]
    async fn test_printf_burst_is_drained() {
        init_capture_logger();
        let args = [
            "-c".into(),
            "i=1; while [ \"$i\" -le 500 ]; do printf 'bless-drain-burst-%s\\n' \"$i\"; i=$((i+1)); done".into(),
        ];
        let status = run_command("bash", &args).await.expect("printf burst");
        assert!(status.success());
        let prefix = "bless-drain-burst-";
        let mut got: Vec<u32> = captured_lines()
            .into_iter()
            .filter_map(|line| line.strip_prefix(prefix)?.parse().ok())
            .collect();
        got.sort_unstable();
        got.dedup();
        assert_eq!(got, (1..=500).collect::<Vec<_>>());
    }

    #[tokio::test]
    async fn test_grandchild_holding_pipe_does_not_hang() {
        let args = ["-c".into(), "sleep 10 & echo done".into()];
        let fut = run_command("bash", &args);
        let result = tokio::time::timeout(std::time::Duration::from_secs(5), fut).await;
        assert!(
            result.is_ok(),
            "run_command must return after the child exits even if a grandchild holds a pipe"
        );
        let status = result.unwrap().expect("run_command should succeed");
        assert!(status.success());
    }

    #[tokio::test]
    async fn test_failing_command() {
        let result = run_command("false", &[]).await;
        assert!(
            result.is_ok(),
            "Command should return Ok with non-zero status"
        );
        let status = result.unwrap();
        assert!(!status.success());
    }

    #[tokio::test]
    async fn test_nonexistent_command() {
        let result = run_command("nonexistent_command_xyz", &[]).await;
        assert!(result.is_err(), "Expected error for nonexistent command");
    }

    #[tokio::test]
    async fn test_exit_code_passthrough() {
        let result = run_command("bash", &["-c".into(), "exit 42".into()]).await;
        assert!(result.is_ok());
        let status = result.unwrap();
        assert_eq!(status.code(), Some(42));
        assert_eq!(bless::runner::exit_code_from_status(status), 42);
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn test_signal_death_maps_to_128_plus_signal() {
        let result = run_command("bash", &["-c".into(), "kill -s TERM $$".into()]).await;
        assert!(result.is_ok());
        let status = result.unwrap();
        assert_eq!(status.code(), None);
        assert_eq!(bless::runner::exit_code_from_status(status), 128 + 15);
    }

    #[tokio::test]
    async fn test_command_with_stdout_and_stderr() {
        // run_command logs stdout as INFO and stderr as WARN via the log crate
        // This test verifies the command completes successfully
        let result = run_command("bash", &["-c".into(), "echo out && echo err 1>&2".into()]).await;
        assert!(result.is_ok());
        assert!(result.unwrap().success());
    }

    #[tokio::test]
    async fn test_cli_parse_basic() {
        use bless::cli::Cli;
        use clap::Parser;

        let cli = Cli::try_parse_from(["bless", "--label", "test", "--", "echo", "hi"]);
        assert!(cli.is_ok());
        let cli = cli.unwrap();
        assert_eq!(cli.label, "test");
        assert_eq!(cli.command, vec!["echo", "hi"]);
        assert!(!cli.no_timestamp);
        assert!(!cli.split);
        #[cfg(feature = "mongodb")]
        assert!(!cli.force_gridfs);
    }

    #[tokio::test]
    async fn test_cli_parse_all_flags() {
        use bless::cli::Cli;
        use clap::Parser;

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
    #[tokio::test]
    async fn test_cli_parse_force_gridfs() {
        use bless::cli::Cli;
        use clap::Parser;

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
    #[tokio::test]
    async fn test_cli_rejects_mongodb_flags_without_feature() {
        use bless::cli::Cli;
        use clap::Parser;

        assert!(Cli::try_parse_from(["bless", "--use-mongodb", "--", "echo", "hi"]).is_err());
        assert!(Cli::try_parse_from(["bless", "--force-gridfs", "--", "echo", "hi"]).is_err());
        assert!(Cli::try_parse_from(["bless", "--db", "x", "--", "echo", "hi"]).is_err());
        assert!(Cli::try_parse_from(["bless", "--collection", "x", "--", "echo", "hi"]).is_err());
    }

    #[tokio::test]
    async fn test_cli_requires_command() {
        use bless::cli::Cli;
        use clap::Parser;

        let cli = Cli::try_parse_from(["bless"]);
        assert!(cli.is_err());
    }

    #[cfg(feature = "serve")]
    #[tokio::test]
    async fn test_cli_serve_without_command() {
        use bless::cli::Cli;
        use clap::Parser;

        let cli = Cli::try_parse_from(["bless", "--serve", ":9000"]);
        assert!(cli.is_ok());
        let cli = cli.unwrap();
        assert_eq!(cli.serve.as_deref(), Some(":9000"));
        assert!(cli.command.is_empty());
        assert!(cli.remote.is_none());
    }

    #[cfg(feature = "serve")]
    #[tokio::test]
    async fn test_cli_serve_conflicts_with_remote() {
        use bless::cli::Cli;
        use clap::Parser;

        let cli = Cli::try_parse_from([
            "bless", "--serve", ":9000", "--remote", ":9001", "--", "true",
        ]);
        assert!(cli.is_err());
    }

    #[cfg(all(feature = "serve", feature = "mongodb"))]
    #[tokio::test]
    async fn test_cli_serve_conflicts_with_mongodb() {
        use bless::cli::Cli;
        use clap::Parser;

        let cli = Cli::try_parse_from(["bless", "--serve", ":9000", "--use-mongodb"]);
        assert!(cli.is_err());
    }

    #[cfg(feature = "serve")]
    #[tokio::test]
    async fn test_cli_local_requires_remote() {
        use bless::cli::Cli;
        use clap::Parser;

        let cli = Cli::try_parse_from(["bless", "--local", "--", "true"]);
        assert!(cli.is_err());
    }

    #[cfg(feature = "serve")]
    #[tokio::test]
    async fn test_cli_remote_requires_command() {
        use bless::cli::Cli;
        use clap::Parser;

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
    #[tokio::test]
    async fn test_cli_remote_without_local_skips_default_gzip() {
        use bless::cli::Cli;
        use clap::Parser;

        let cli = Cli::try_parse_from(["bless", "--remote", ":9", "--", "true"]).unwrap();
        assert_eq!(cli.gzip_output(), Some("-"));
        assert!(cli.output.is_none());
    }

    #[cfg(feature = "serve")]
    #[tokio::test]
    async fn test_cli_remote_with_local_uses_default_gzip() {
        use bless::cli::Cli;
        use clap::Parser;

        let cli =
            Cli::try_parse_from(["bless", "--remote", ":9", "--local", "--", "true"]).unwrap();
        assert_eq!(cli.gzip_output(), None);
        assert!(cli.local);
    }

    #[cfg(feature = "serve")]
    #[tokio::test]
    async fn test_cli_remote_with_dash_o_keeps_path() {
        use bless::cli::Cli;
        use clap::Parser;

        let cli =
            Cli::try_parse_from(["bless", "--remote", ":9", "-o", "x.gz", "--", "true"]).unwrap();
        assert_eq!(cli.gzip_output(), Some("x.gz"));
        assert!(!cli.local);
    }

    #[cfg(feature = "mongodb")]
    #[tokio::test]
    async fn test_cli_parses_mongodb_with_dash_output() {
        use bless::cli::Cli;
        use clap::Parser;

        // Clap accepts the combination; persist rejects it as BlessError::Config
        // because -o - opens no gzip to upload.
        let cli = Cli::try_parse_from(["bless", "--use-mongodb", "-o", "-", "--", "echo", "hi"]);
        assert!(cli.is_ok());
        let cli = cli.unwrap();
        assert!(cli.use_mongodb);
        assert_eq!(cli.output.as_deref(), Some("-"));
    }

    #[test]
    fn binary_passthrough_exit_42() {
        let dir = tempfile::tempdir().unwrap();
        let out = dir.path().join("run.log.gz");
        let status = std::process::Command::new(env!("CARGO_BIN_EXE_bless"))
            .args(["-o", out.to_str().unwrap(), "--", "bash", "-c", "exit 42"])
            .status()
            .expect("spawn bless");
        assert_eq!(status.code(), Some(42));
    }

    #[test]
    fn binary_self_error_exits_1_and_prefixes_stderr() {
        let output = std::process::Command::new(env!("CARGO_BIN_EXE_bless"))
            .args(["-o", "/no/such/dir/x.log.gz", "--", "true"])
            .output()
            .expect("spawn bless");
        assert_eq!(output.status.code(), Some(1));
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(
            stderr.starts_with("bless:"),
            "stderr should start with bless:, got {stderr:?}"
        );
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
    #[tokio::test]
    async fn test_cli_mongo_dest_defaults() {
        use bless::cli::Cli;
        use clap::Parser;

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
    #[tokio::test]
    async fn test_cli_mongo_dest_flags() {
        use bless::cli::Cli;
        use clap::Parser;

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
    #[tokio::test]
    async fn test_cli_mongo_dest_env() {
        use bless::cli::Cli;
        use clap::Parser;

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
