use std::io::{self, BufRead, BufReader, Write};
use clap::{App, Arg};
use flate2::write::GzEncoder;
use flate2::Compression;
use std::process::{Command, Stdio};
use std::sync::{Arc, Mutex};
use std::thread;
use uuid::Uuid;

fn main() -> io::Result<()> {
    let matches = App::new("bless")
        .version("0.0.1")
        .author("Rohit Goswami <rgoswami@ieee.org>")
        .about("Runs a command and logs output")
        .arg(
            Arg::with_name("command")
                .help("The command to run")
                .required(true)
                .multiple(true)
                .last(true),
        )
        .get_matches_from(wild::args());

    if let Some(command_args) = matches.values_of("command") {
        let command_vec: Vec<&str> = command_args.collect();
        let (command, args) = command_vec.split_first().unwrap();

        let run_uuid = Uuid::new_v4();
        println!("Run UUID: {}", run_uuid);

        let out_file = std::fs::File::create(format!("{}.out.gz", run_uuid)).unwrap();
        let encoder = GzEncoder::new(out_file, Compression::default());
        let encoder = Arc::new(Mutex::new(encoder));

        let mut child = Command::new(command)
            .args(args)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()?;

        let stdout = child.stdout.take().unwrap();
        let stderr = child.stderr.take().unwrap();

        let encoder_clone = encoder.clone();
        let handle_out = thread::spawn(move || {
            let reader = BufReader::new(stdout);
            for line in reader.lines() {
                let line = line.unwrap();
                let mut encoder = encoder_clone.lock().unwrap();
                writeln!(encoder, "{}", line).unwrap();
                println!("{}", line);
            }
        });

        let encoder_clone = encoder.clone();
        let handle_err = thread::spawn(move || {
            let reader = BufReader::new(stderr);
            for line in reader.lines() {
                let line = line.unwrap();
                let mut encoder = encoder_clone.lock().unwrap();
                writeln!(encoder, "{}", line).unwrap();
                eprintln!("{}", line);
            }
        });

        handle_out.join().unwrap();
        handle_err.join().unwrap();

        let encoder = Arc::try_unwrap(encoder)
            .expect("Arc::try_unwrap failed")
            .into_inner()
            .unwrap();
        encoder.finish().unwrap();
        child.wait()?; // Wait for the process to exit
    }

    Ok(())
}
