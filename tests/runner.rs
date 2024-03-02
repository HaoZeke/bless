#[cfg(test)]
mod tests {
    use bless::runner::run_command;
    #[tokio::test]
    async fn test_successful_command() -> std::io::Result<()> {
        let output_data = run_command("ls", &[]);
        assert!(!output_data.await.is_err());
        Ok(())
    }

    #[tokio::test]
    async fn test_failing_command() -> std::io::Result<()> {
        let output_data = run_command("non_existent_command", &[]);
        assert!(output_data.await.is_err());
        Ok(())
    }
    #[tokio::test]
    async fn test_command_output_accuracy() -> std::io::Result<()> {
        let output_data = run_command("echo", &["Hello, World!"]).await.unwrap();
        assert_eq!(output_data, vec!["Hello, World!"]);
        Ok(())
    }
    #[tokio::test]
    async fn test_command_with_stdout_and_stderr() -> std::io::Result<()> {
        let output_data = run_command("bash", &["-c", "echo out && echo err 1>&2"])
            .await
            .unwrap();
        let expected = vec!["out", "err"];
        assert!(
            output_data.iter().eq(expected.iter()),
            "The command output did not match the expected stdout and stderr"
        );
        Ok(())
    }
    #[tokio::test]
    async fn test_long_running_command() -> std::io::Result<()> {
        // Using `sleep` command to simulate a long-running operation. Adjust based on your environment.
        let output_data = run_command("sleep", &["1"]).await.unwrap();

        // Verify the command had no output but completed successfully.
        assert!(
            output_data.is_empty(),
            "Expected no output from the sleep command"
        );
        Ok(())
    }
}
