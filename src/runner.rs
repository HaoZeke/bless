use std::io::{self, BufRead, BufReader};
use std::process::{Command, Stdio};

pub fn run_command(command: &str, args: &[&str]) -> Result<Vec<String>, io::Error> {
    let child = Command::new(command)
        .args(args)
        .stderr(Stdio::piped()) // Redirect stderr to stdout
        .stdout(Stdio::piped()) // Ensure stdout is captured
        .spawn(); // Use spawn to start the command without waiting for it to finish

    // Handle potential error when spawning the command
    let mut child = match child {
        Ok(child) => child,
        Err(e) => return Err(io::Error::new(io::ErrorKind::Other, e.to_string())),
    };

    // Prepare to read from the command's stdout
    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| io::Error::new(io::ErrorKind::Other, "Failed to capture stdout"))?;
    let reader = BufReader::new(stdout);

    let mut output_data = Vec::new();

    for line in reader.lines() {
        let line = line?;
        println!("{}", line);
        output_data.push(line);
    }

    // Wait for the command to finish and check its exit status
    let status = child.wait()?;
    if !status.success() {
        let error_message = format!(
            "Command failed with exit status: {}",
            status.code().unwrap_or(-1)
        );
        output_data.push(error_message);
    }

    Ok(output_data)
}
