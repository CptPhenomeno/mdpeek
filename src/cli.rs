//! Argument parsing, path validation and exit codes.

use std::path::PathBuf;

use clap::Parser;

/// Command-line arguments of `readmd`.
#[derive(Debug, Parser)]
#[command(name = "readmd", version, about = "Terminal Markdown reader")]
pub struct Args {
    /// File to display.
    pub file: PathBuf,
}
