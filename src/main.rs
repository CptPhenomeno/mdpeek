use std::path::Path;
use std::process::ExitCode;

use clap::Parser;
use readmd::cli::{Args, StartTarget};
use readmd::doc::Document;
use readmd::ui;

fn main() -> ExitCode {
    let config = match Args::parse().into_config() {
        Ok(config) => config,
        Err(err) => {
            eprintln!("readmd: {err}");
            return err.exit_code();
        }
    };
    // Placeholder until the start-up root and initial file logic lands (KB-25):
    // a file's parent is the root; a directory has no file to open yet.
    let (doc, rel_path) = match &config.target {
        StartTarget::File(path) => match Document::from_path(path) {
            Ok(doc) => (doc, relative_to_root(path)),
            Err(err) => {
                eprintln!("readmd: {}: {err}", path.display());
                return ExitCode::FAILURE;
            }
        },
        StartTarget::Directory(_) => (
            Document {
                empty: true,
                ..Document::default()
            },
            String::new(),
        ),
    };
    match ui::run(doc, rel_path) {
        Ok(()) => ExitCode::SUCCESS,
        Err(err) => {
            eprintln!("readmd: {err}");
            ExitCode::FAILURE
        }
    }
}

/// Path of `file` relative to its parent directory, the temporary root (KB-25).
fn relative_to_root(file: &Path) -> String {
    file.file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default()
}
