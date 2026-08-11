use std::process::{ExitStatus, Stdio};

use log::{error, info, warn};
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::{Child, Command};

use crate::error::BlessError;

#[cfg(unix)]
use std::os::unix::process::ExitStatusExt;

pub fn exit_code_from_status(status: ExitStatus) -> u8 {
    if let Some(code) = status.code() {
        return code as u8;
    }

    #[cfg(unix)]
    if let Some(sig) = status.signal() {
        return 128 + sig as u8;
    }

    1
}

pub async fn run_command(command: &str, args: &[String]) -> Result<ExitStatus, BlessError> {
    let mut cmd = Command::new(command);
    cmd.args(args)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);

    #[cfg(unix)]
    {
        cmd.process_group(0);
    }

    let mut child = cmd.spawn()?;

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

    let status = tokio::select! {
        status = child.wait() => status?,
        _ = tokio::signal::ctrl_c() => {
            interrupt_child(&mut child).await;
            error!("Interrupted by signal");
            child.wait().await?
        }
    };

    stdout_task.abort();
    stderr_task.abort();
    let _ = stdout_task.await;
    let _ = stderr_task.await;

    Ok(status)
}

#[cfg(unix)]
async fn interrupt_child(child: &mut Child) {
    if let Some(pid) = child.id() {
        // process_group(0) makes the child's pid the process group id.
        let pgid = pid as i32;
        // SAFETY: `-pgid` is the child's process group. kill(2) with a
        // negative pid targets that group; ESRCH is ignored if it is gone.
        unsafe {
            libc::kill(-pgid, libc::SIGINT);
            libc::kill(-pgid, libc::SIGKILL);
        }
    }
    let _ = child.kill().await;
}

#[cfg(windows)]
async fn interrupt_child(child: &mut Child) {
    let _ = child.kill().await;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(unix)]
    use std::os::unix::process::ExitStatusExt;

    #[cfg(unix)]
    #[test]
    fn maps_unix_signal_to_128_plus_n() {
        let term = ExitStatus::from_raw(libc::SIGTERM);
        assert_eq!(exit_code_from_status(term), 128 + libc::SIGTERM as u8);

        let kill = ExitStatus::from_raw(libc::SIGKILL);
        assert_eq!(exit_code_from_status(kill), 128 + libc::SIGKILL as u8);

        let int = ExitStatus::from_raw(libc::SIGINT);
        assert_eq!(exit_code_from_status(int), 128 + libc::SIGINT as u8);
    }

    #[cfg(unix)]
    #[test]
    fn maps_unix_wait_status_exit_code() {
        let status = ExitStatus::from_raw(42 << 8);
        assert_eq!(exit_code_from_status(status), 42);

        let status = ExitStatus::from_raw(0);
        assert_eq!(exit_code_from_status(status), 0);
    }
}
