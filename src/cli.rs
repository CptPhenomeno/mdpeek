//! Argument parsing, path validation and exit codes.

use std::fmt;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use clap::{Parser, ValueEnum};

/// File extensions the reader can open (compared case-insensitively).
pub const SUPPORTED_EXTENSIONS: [&str; 4] = ["md", "markdown", "mmd", "mermaid"];

/// Exit code for an invalid path or unsupported file (SPEC §3.1).
pub const EXIT_INVALID_INPUT: u8 = 2;

/// Graphics protocol selection (`--protocol`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, ValueEnum)]
pub enum ProtocolChoice {
    /// Detect the protocol from the terminal.
    #[default]
    Auto,
    Kitty,
    Sixel,
    Iterm2,
    Halfblocks,
}

/// Raw command-line arguments of `mdpeek`.
#[derive(Debug, Parser)]
#[command(name = "mdpeek", version, about = "Terminal Markdown reader")]
pub struct Args {
    /// File (.md, .markdown, .mmd, .mermaid) or directory; defaults to the current directory.
    pub path: Option<PathBuf>,
    /// Force a graphics protocol.
    #[arg(long, value_enum, default_value_t = ProtocolChoice::Auto)]
    pub protocol: ProtocolChoice,
    /// Start with diagram text mode on.
    #[arg(long)]
    pub text_diagrams: bool,
}

/// Where the viewer starts.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StartTarget {
    File(PathBuf),
    Directory(PathBuf),
}

/// Validated start-up configuration.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Config {
    pub target: StartTarget,
    pub protocol: ProtocolChoice,
    pub text_diagrams: bool,
}

/// A command-line input that cannot be used.
#[derive(Debug)]
pub enum CliError {
    NotFound(PathBuf),
    Unreadable(PathBuf, std::io::Error),
    UnsupportedExtension(PathBuf),
}

impl CliError {
    /// Process exit code for this error.
    pub fn exit_code(&self) -> ExitCode {
        ExitCode::from(EXIT_INVALID_INPUT)
    }
}

impl fmt::Display for CliError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NotFound(p) => write!(f, "{}: no such file or directory", p.display()),
            Self::Unreadable(p, e) => write!(f, "{}: cannot read: {e}", p.display()),
            Self::UnsupportedExtension(p) => write!(
                f,
                "{}: unsupported file type (supported extensions: {})",
                p.display(),
                SUPPORTED_EXTENSIONS
                    .iter()
                    .map(|e| format!(".{e}"))
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
        }
    }
}

impl std::error::Error for CliError {}

pub fn has_supported_extension(path: &Path) -> bool {
    path.extension().and_then(|e| e.to_str()).is_some_and(|e| {
        SUPPORTED_EXTENSIONS
            .iter()
            .any(|s| s.eq_ignore_ascii_case(e))
    })
}

impl Args {
    /// Validate the path and build the start-up configuration.
    pub fn into_config(self) -> Result<Config, CliError> {
        let path = self.path.unwrap_or_else(|| PathBuf::from("."));
        let meta = match fs::metadata(&path) {
            Ok(m) => m,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                return Err(CliError::NotFound(path));
            }
            Err(e) => return Err(CliError::Unreadable(path, e)),
        };
        let target = if meta.is_dir() {
            fs::read_dir(&path).map_err(|e| CliError::Unreadable(path.clone(), e))?;
            StartTarget::Directory(path)
        } else {
            if !has_supported_extension(&path) {
                return Err(CliError::UnsupportedExtension(path));
            }
            fs::File::open(&path).map_err(|e| CliError::Unreadable(path.clone(), e))?;
            StartTarget::File(path)
        };
        Ok(Config {
            target,
            protocol: self.protocol,
            text_diagrams: self.text_diagrams,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn config<const N: usize>(argv: [&str; N]) -> Result<Config, CliError> {
        Args::try_parse_from(std::iter::once("mdpeek").chain(argv))
            .expect("valid arguments")
            .into_config()
    }

    fn tmp() -> tempfile::TempDir {
        tempfile::tempdir().unwrap()
    }

    #[test]
    fn no_path_defaults_to_current_directory() {
        let c = config([]).unwrap();
        assert_eq!(c.target, StartTarget::Directory(PathBuf::from(".")));
        assert_eq!(c.protocol, ProtocolChoice::Auto);
        assert!(!c.text_diagrams);
    }

    #[test]
    fn protocol_accepts_known_values_and_rejects_others() {
        for (s, p) in [
            ("auto", ProtocolChoice::Auto),
            ("kitty", ProtocolChoice::Kitty),
            ("sixel", ProtocolChoice::Sixel),
            ("iterm2", ProtocolChoice::Iterm2),
            ("halfblocks", ProtocolChoice::Halfblocks),
        ] {
            assert_eq!(config(["--protocol", s]).unwrap().protocol, p);
        }
        assert!(Args::try_parse_from(["mdpeek", "--protocol", "bogus"]).is_err());
    }

    #[test]
    fn text_diagrams_flag_sets_text_mode() {
        assert!(config(["--text-diagrams"]).unwrap().text_diagrams);
    }

    #[test]
    fn uppercase_extension_is_accepted() {
        let dir = tmp();
        let f = dir.path().join("A.MD");
        fs::write(&f, "# x").unwrap();
        let c = config([f.to_str().unwrap()]).unwrap();
        assert_eq!(c.target, StartTarget::File(f));
    }

    #[test]
    fn directory_yields_directory_target() {
        let dir = tmp();
        let d = dir.path().to_path_buf();
        let c = config([d.to_str().unwrap()]).unwrap();
        assert_eq!(c.target, StartTarget::Directory(d));
    }

    #[test]
    fn missing_path_is_not_found() {
        let dir = tmp();
        let p = dir.path().join("nope.md");
        assert!(matches!(
            config([p.to_str().unwrap()]),
            Err(CliError::NotFound(_))
        ));
    }

    #[test]
    fn unsupported_extension_is_rejected() {
        let dir = tmp();
        let f = dir.path().join("a.txt");
        fs::write(&f, "x").unwrap();
        let err = config([f.to_str().unwrap()]).unwrap_err();
        assert!(matches!(err, CliError::UnsupportedExtension(_)));
        assert!(err.to_string().contains(".markdown"));
    }
}
