use log::{error, info};
use std::io::{self, BufRead, BufReader};
use std::process::{Command, Stdio};
use tokio::task;

pub async fn run_command(command: &str, args: &[&str]) -> Result<(), io::Error> {
    // Clone `command` and `args` to satisfy 'static lifetime requirements.
    let command = command.to_string();
    let args = args
        .iter()
        .map(|&arg| arg.to_string())
        .collect::<Vec<String>>();

    task::spawn_blocking(move || {
        let process = Command::new(&command)
            .args(&args)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn();

        match process {
            Ok(mut child) => {
                let stdout = child.stdout.take().expect("Failed to capture stdout");
                let stderr = child.stderr.take().expect("Failed to capture stderr");

                let stdout_reader = BufReader::new(stdout);
                let stderr_reader = BufReader::new(stderr);

                let stdout_handle = std::thread::spawn(move || {
                    for line in stdout_reader.lines() {
                        match line {
                            Ok(line) => info!("{}", line),
                            Err(e) => error!("Error reading stdout: {}", e),
                        }
                    }
                });

                let stderr_handle = std::thread::spawn(move || {
                    for line in stderr_reader.lines() {
                        match line {
                            Ok(line) => error!("{}", line),
                            Err(e) => error!("Error reading stderr: {}", e),
                        }
                    }
                });

                let _ = stdout_handle.join();
                let _ = stderr_handle.join();

                let status = child.wait()?;
                if !status.success() {
                    return Err(io::Error::new(
                        io::ErrorKind::Other,
                        format!("Command exited with status: {}", status),
                    ));
                }

                Ok(())
            }
            Err(e) => Err(io::Error::new(io::ErrorKind::Other, e.to_string())),
        }
    })
    .await?
}
