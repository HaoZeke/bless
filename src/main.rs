use clap::{App, Arg};
use flate2::write::GzEncoder;
use flate2::Compression;
use std::io::{BufRead, BufReader, Write};
use duct::cmd;
use std::sync::{Arc, Mutex};
use uuid::Uuid;

fn main() -> std::io::Result<()> {
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

        // Use duct to create a subprocess, merging stderr into stdout
        let reader = cmd(*command, args).stderr_to_stdout().reader()?;
        let reader = BufReader::new(reader);

        // Process output in real-time
        reader.lines().for_each(|line| {
            let line = line.unwrap();
            println!("{}", line); // Print to stdout
            let mut encoder = encoder.lock().unwrap();
            writeln!(encoder, "{}", line).unwrap(); // Write to gzip file
        });

        // Finish encoding
        let encoder = Arc::try_unwrap(encoder)
            .expect("Arc::try_unwrap failed")
            .into_inner()
            .unwrap();
        encoder.finish()?;
    }

    Ok(())
}
