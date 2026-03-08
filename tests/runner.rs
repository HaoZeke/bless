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
    async fn test_cli_requires_command() {
        use bless::cli::Cli;
        use clap::Parser;

        let cli = Cli::try_parse_from(["bless"]);
        assert!(cli.is_err());
    }
}
