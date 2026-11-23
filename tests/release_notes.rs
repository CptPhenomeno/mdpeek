//! Delivery tooling: the release-notes script, the Makefile `dist` naming and
//! the GitHub workflow files.

#![allow(clippy::unwrap_used, clippy::expect_used)] // panics are fine in tests

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn notes(dir: &Path, args: &[&str]) -> Output {
    Command::new("sh")
        .arg(root().join(".github/scripts/release-notes.sh"))
        .args(args)
        .current_dir(dir)
        .output()
        .unwrap()
}

const CHANGELOG: &str = "# Changelog\n\n## [Unreleased]\n\n## [0.2.0] - 2026-02-01\n\n### Added\n\n- Two.\n\n## [0.1.0] - 2026-01-01\n\n### Added\n\n- One.\n";

#[test]
fn notes_print_only_the_section_of_the_tag() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("CHANGELOG.md"), CHANGELOG).unwrap();
    let out = notes(dir.path(), &["v0.2.0"]);
    assert!(out.status.success());
    assert_eq!(
        String::from_utf8(out.stdout).unwrap(),
        "## [0.2.0] - 2026-02-01\n\n### Added\n\n- Two.\n\n"
    );
}

#[test]
fn notes_without_a_tag_fail_with_usage() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("CHANGELOG.md"), CHANGELOG).unwrap();
    let out = notes(dir.path(), &[]);
    assert!(!out.status.success());
    assert!(String::from_utf8_lossy(&out.stderr).contains("usage: release-notes.sh <tag>"));
}

#[test]
fn dist_names_the_binary_for_the_host() {
    let out = Command::new("make")
        .args(["--no-print-directory", "-s", "dist-name"])
        .current_dir(root())
        .output()
        .unwrap();
    assert!(out.status.success());
    let name = String::from_utf8(out.stdout).unwrap();
    let os = match std::env::consts::OS {
        "macos" => "darwin",
        o => o,
    };
    let arch = match std::env::consts::ARCH {
        "x86_64" => "amd64",
        "aarch64" => "arm64",
        a => a,
    };
    assert_eq!(
        name.trim(),
        format!("dist/readmd-{}-{os}-{arch}", env!("CARGO_PKG_VERSION"))
    );
    let dry = Command::new("make")
        .args(["-n", "dist"])
        .current_dir(root())
        .output()
        .unwrap();
    assert!(String::from_utf8_lossy(&dry.stdout).contains(name.trim()));
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum Validator {
    Actionlint,
    PyYaml,
}

const YAML_SCRIPT: &str = "import sys,yaml\nfor f in sys.argv[1:]:\n d=yaml.safe_load(open(f))\n assert 'jobs' in d and (True in d or 'on' in d), f\n";

/// Actionlint wins, then PyYAML; `None` means nothing can validate.
fn pick_validator(has_actionlint: bool, has_yaml: bool) -> Option<Validator> {
    if has_actionlint {
        Some(Validator::Actionlint)
    } else if has_yaml {
        Some(Validator::PyYaml)
    } else {
        None
    }
}

fn has_pyyaml() -> bool {
    Command::new("python3")
        .args(["-I", "-c", "import yaml"])
        .output()
        .is_ok_and(|o| o.status.success())
}

fn has_actionlint() -> bool {
    Command::new("actionlint").arg("-version").output().is_ok()
}

fn detect_validator() -> Option<Validator> {
    pick_validator(has_actionlint(), has_pyyaml())
}

/// Validates the files; with no validator prints the reason to `err` and skips.
fn check_workflows(
    validator: Option<Validator>,
    files: &[std::path::PathBuf],
    err: &mut impl std::io::Write,
) -> Result<(), String> {
    let out = match validator {
        None => {
            writeln!(
                err,
                "no actionlint or python3 with PyYAML: workflow check skipped"
            )
            .unwrap();
            return Ok(());
        }
        Some(Validator::Actionlint) => Command::new("actionlint").args(files).output(),
        Some(Validator::PyYaml) => Command::new("python3")
            .args(["-I", "-c", YAML_SCRIPT])
            .args(files)
            .output(),
    }
    .map_err(|e| e.to_string())?;
    if out.status.success() {
        Ok(())
    } else {
        Err(format!(
            "{}{}",
            String::from_utf8_lossy(&out.stdout),
            String::from_utf8_lossy(&out.stderr)
        ))
    }
}

fn workflow_files() -> Vec<std::path::PathBuf> {
    let wf = root().join(".github/workflows");
    ["check.yml", "deploy.yml"].map(|f| wf.join(f)).to_vec()
}

#[test]
fn workflows_are_valid() {
    let mut err = std::io::stderr();
    check_workflows(detect_validator(), &workflow_files(), &mut err).unwrap();
}

#[test]
fn no_validator_skips_with_reason_on_stderr() {
    let mut buf = Vec::new();
    assert!(check_workflows(None, &workflow_files(), &mut buf).is_ok());
    assert!(String::from_utf8(buf).unwrap().contains("skipped"));
}

#[test]
fn python_without_yaml_counts_as_no_validator() {
    assert_eq!(pick_validator(false, false), None);
    assert_eq!(pick_validator(true, false), Some(Validator::Actionlint));
    assert_eq!(pick_validator(false, true), Some(Validator::PyYaml));
}

fn broken_workflow_fails(v: Validator) {
    let dir = std::env::temp_dir().join(format!("wf-broken-{v:?}-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let bad = dir.join("bad.yml");
    std::fs::write(&bad, "name: x\n").unwrap();
    let mut err = Vec::new();
    let broken = check_workflows(Some(v), &[bad], &mut err);
    std::fs::remove_dir_all(&dir).unwrap();
    assert!(broken.is_err());
    assert!(check_workflows(Some(v), &workflow_files(), &mut err).is_ok());
}

#[test]
fn actionlint_validates_both_workflows() {
    if has_actionlint() {
        broken_workflow_fails(Validator::Actionlint);
    } else {
        eprintln!("actionlint not available: skipped");
    }
}

#[test]
fn pyyaml_validates_both_workflows() {
    if has_pyyaml() {
        broken_workflow_fails(Validator::PyYaml);
    } else {
        eprintln!("PyYAML not available: skipped");
    }
}
