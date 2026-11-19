use std::process::ExitCode;

use clap::Parser;
use readmd::{cli::Args, ui};

fn main() -> ExitCode {
    let args = Args::parse();
    let bytes = match std::fs::read(&args.file) {
        Ok(bytes) => bytes,
        Err(err) => {
            eprintln!("readmd: {}: {err}", args.file.display());
            return ExitCode::FAILURE;
        }
    };
    let lines = String::from_utf8_lossy(&bytes)
        .lines()
        .map(str::to_owned)
        .collect();
    match ui::run(lines) {
        Ok(()) => ExitCode::SUCCESS,
        Err(err) => {
            eprintln!("readmd: {err}");
            ExitCode::FAILURE
        }
    }
}
