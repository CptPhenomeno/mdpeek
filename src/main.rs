use std::path::Path;
use std::process::ExitCode;

use clap::Parser;
use readmd::cli::Args;
use readmd::doc::Document;
use readmd::{tree, ui};

fn main() -> ExitCode {
    let config = match Args::parse().into_config() {
        Ok(config) => config,
        Err(err) => {
            eprintln!("readmd: {err}");
            return err.exit_code();
        }
    };
    let start = match tree::start(&config.target) {
        Ok(start) => start,
        Err(err) => {
            eprintln!("readmd: {err}");
            return ExitCode::FAILURE;
        }
    };
    let (doc, rel_path) = match &start.initial_file {
        Some(path) => match Document::from_path(path) {
            Ok(doc) => (doc, relative_to_root(path, &start.root)),
            Err(err) => {
                eprintln!("readmd: {}: {err}", path.display());
                return ExitCode::FAILURE;
            }
        },
        None => (
            Document {
                empty: true,
                ..Document::default()
            },
            String::new(),
        ),
    };
    match ui::run(doc, rel_path, config.protocol, config.text_diagrams) {
        Ok(()) => ExitCode::SUCCESS,
        Err(err) => {
            eprintln!("readmd: {err}");
            ExitCode::FAILURE
        }
    }
}

/// Path of `file` relative to the start-up root, for the header.
fn relative_to_root(file: &Path, root: &Path) -> String {
    file.strip_prefix(root)
        .unwrap_or(file)
        .to_string_lossy()
        .into_owned()
}
