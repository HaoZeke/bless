#[cfg(test)]
mod tests {
    use bless::runner::run_command;
    use tokio::task;

    #[tokio::test]
    async fn test_successful_command() -> std::io::Result<()> {
        let result = run_command("ls", &[]).await;
        assert!(result.is_ok(), "Expected command to succeed, but it failed");
        Ok(())
    }

    #[tokio::test]
    async fn test_failing_command() -> std::io::Result<()> {
        let result = run_command("non_existent_command", &[]).await;
        assert!(
            result.is_err(),
            "Expected command to fail, but it succeeded"
        );
        Ok(())
    }

    #[tokio::test]
    async fn test_command_output_accuracy() -> std::io::Result<()> {
        let result = task::spawn_blocking(|| {
            let output = std::process::Command::new("echo")
                .arg("Hello, World!")
                .output()
                .expect("Failed to execute command");
            String::from_utf8(output.stdout).expect("Invalid UTF-8")
        })
        .await
        .expect("Failed to spawn blocking task");

        assert_eq!(
            result.trim(),
            "Hello, World!",
            "The command output was not as expected"
        );
        Ok(())
    }

    #[tokio::test]
    async fn test_command_with_stdout_and_stderr() -> std::io::Result<()> {
        let result = task::spawn_blocking(|| {
            let output = std::process::Command::new("bash")
                .arg("-c")
                .arg("echo out && echo err 1>&2")
                .output()
                .expect("Failed to execute command");
            let stdout = String::from_utf8(output.stdout).expect("Invalid UTF-8");
            let stderr = String::from_utf8(output.stderr).expect("Invalid UTF-8");
            (stdout, stderr)
        })
        .await
        .expect("Failed to spawn blocking task");

        let (stdout, stderr) = result;
        assert_eq!(stdout.trim(), "out", "The stdout was not as expected");
        assert_eq!(stderr.trim(), "err", "The stderr was not as expected");
        Ok(())
    }

    #[tokio::test]
    async fn test_long_running_command() -> std::io::Result<()> {
        // Using `sleep` command to simulate a long-running operation.
        let result = run_command("sleep", &["1"]).await;

        // Verify the command completed successfully.
        assert!(
            result.is_ok(),
            "Expected sleep command to complete successfully"
        );
        Ok(())
    }
}
