use std::process::ExitCode;

use clap::Parser;
use readmd::cli::{Args, StartTarget};
use readmd::ui;

fn main() -> ExitCode {
    let config = match Args::parse().into_config() {
        Ok(config) => config,
        Err(err) => {
            eprintln!("readmd: {err}");
            return err.exit_code();
        }
    };
    // Placeholder until the start-up root and initial file logic lands (KB-25).
    let lines = match &config.target {
        StartTarget::File(path) => match std::fs::read(path) {
            Ok(bytes) => String::from_utf8_lossy(&bytes)
                .lines()
                .map(str::to_owned)
                .collect(),
            Err(err) => {
                eprintln!("readmd: {}: {err}", path.display());
                return ExitCode::FAILURE;
            }
        },
        StartTarget::Directory(_) => Vec::new(),
    };
    match ui::run(lines) {
        Ok(()) => ExitCode::SUCCESS,
        Err(err) => {
            eprintln!("readmd: {err}");
            ExitCode::FAILURE
        }
    }
}
