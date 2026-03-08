use crate::error::BlessError;
use log::{error, info, warn};
use std::process::ExitStatus;
use std::process::Stdio;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::Command;

pub async fn run_command(command: &str, args: &[String]) -> Result<ExitStatus, BlessError> {
    let mut child = Command::new(command)
        .args(args)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;

    let stdout = child.stdout.take().expect("stdout piped");
    let stderr = child.stderr.take().expect("stderr piped");

    let stdout_task = tokio::spawn(async move {
        let reader = BufReader::new(stdout);
        let mut lines = reader.lines();
        while let Ok(Some(line)) = lines.next_line().await {
            info!("{}", line);
        }
    });

    let stderr_task = tokio::spawn(async move {
        let reader = BufReader::new(stderr);
        let mut lines = reader.lines();
        while let Ok(Some(line)) = lines.next_line().await {
            warn!("{}", line);
        }
    });

    tokio::select! {
        _ = async {
            let _ = stdout_task.await;
            let _ = stderr_task.await;
        } => {}
        _ = tokio::signal::ctrl_c() => {
            // On ctrl-c, kill the child process
            let _ = child.kill().await;
            error!("Interrupted by signal");
        }
    }

    let status = child.wait().await?;
    Ok(status)
}
