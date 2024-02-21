use duct::cmd;
use std::io::{BufRead, BufReader};

pub fn run_command(command: &str, args: &[&str]) -> Vec<String> {
    let reader = cmd(command, args).stderr_to_stdout().reader().unwrap();
    let reader = BufReader::new(reader);

    let mut output_data = Vec::new();
    reader.lines().for_each(|line| {
        let line = line.unwrap();
        println!("{}", line); // Print to stdout
        output_data.push(line);
    });

    output_data
}
