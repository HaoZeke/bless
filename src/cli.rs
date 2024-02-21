use clap::{App, Arg, ArgMatches};

pub fn build_cli() -> ArgMatches {
    App::new("bless")
        .version("0.0.1")
        .author("Rohit Goswami <rgoswami@ieee.org>")
        .about("Runs a command and logs output, stores in MongoDB or a file")
        .arg(
            Arg::with_name("command")
                .help("The command to run")
                .required(true)
                .multiple(true)
                .last(true),
        )
        .arg(
            Arg::with_name("label")
                .long("label")
                .help("Label for the run")
                .takes_value(true),
        )
        .arg(
            Arg::with_name("use_mongodb")
                .long("use-mongodb")
                .help("Store output in MongoDB instead of a file"),
        )
        .get_matches()
}
