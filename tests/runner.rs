#[cfg(test)]
mod tests {
    use bless::runner::run_command;

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
    async fn test_grandchild_holding_pipe_does_not_hang() {
        let fut = run_command(
            "bash",
            &[
                "-c".into(),
                "sleep 10 & echo done".into(),
            ],
        );
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

    #[tokio::test]
    async fn test_cli_requires_command() {
        use bless::cli::Cli;
        use clap::Parser;

        let cli = Cli::try_parse_from(["bless"]);
        assert!(cli.is_err());
    }
}
