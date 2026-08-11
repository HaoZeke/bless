use std::error::Error;
use std::process::ExitCode;

fn main() -> ExitCode {
    match bless::run() {
        Ok(code) => code,
        Err(e) => {
            eprintln!("bless: {e}");
            if let Some(source) = e.source() {
                let extra = source.to_string();
                if !extra.is_empty() && !e.to_string().contains(extra.as_str()) {
                    eprintln!("bless: {source}");
                }
            }
            ExitCode::FAILURE
        }
    }
}
