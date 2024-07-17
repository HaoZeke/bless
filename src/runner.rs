use log::{error, info};
use std::io::{self, BufRead, BufReader};
use std::process::{Command, Stdio};
use tokio::task;

pub async fn run_command(command: &str, args: &[&str]) -> Result<Vec<String>, io::Error> {
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
                let mut output_data = Vec::new();

                // Merge stdout and stderr
                if let Some(ref mut stdout) = child.stdout {
                    let reader = BufReader::new(stdout);
                    reader.lines().for_each(|line| {
                        if let Ok(line) = line {
                            info!("{}", line);
                            output_data.push(line);
                        }
                    });
                }

                if let Some(ref mut stderr) = child.stderr {
                    let reader = BufReader::new(stderr);
                    reader.lines().for_each(|line| {
                        if let Ok(line) = line {
                            error!("{}", line);
                            output_data.push(line);
                        }
                    });
                }

                // Wait for the process to exit and check for errors
                let status = child.wait()?;
                if !status.success() {
                    return Err(io::Error::new(
                        io::ErrorKind::Other,
                        format!("Command exited with status: {}", status),
                    ));
                }

                Ok(output_data)
            }
            Err(e) => Err(io::Error::new(io::ErrorKind::Other, e.to_string())),
        }
    })
    .await?
}
