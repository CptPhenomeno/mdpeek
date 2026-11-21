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

#[test]
fn workflows_are_valid() {
    let wf = root().join(".github/workflows");
    let files = ["check.yml", "deploy.yml"].map(|f| wf.join(f));
    if Command::new("actionlint").arg("-version").output().is_ok() {
        let out = Command::new("actionlint").args(&files).output().unwrap();
        assert!(
            out.status.success(),
            "{}",
            String::from_utf8_lossy(&out.stdout)
        );
        return;
    }
    let script = "import sys,yaml\nfor f in sys.argv[1:]:\n d=yaml.safe_load(open(f))\n assert 'jobs' in d and (True in d or 'on' in d), f\n";
    let out = Command::new("python3")
        .args(["-I", "-c", script])
        .args(&files)
        .output();
    match out {
        Ok(o) => assert!(o.status.success(), "{}", String::from_utf8_lossy(&o.stderr)),
        Err(_) => eprintln!("no actionlint or python3: skipped"),
    }
}
