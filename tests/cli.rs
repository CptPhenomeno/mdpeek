#![allow(clippy::unwrap_used, clippy::expect_used)] // panics are fine in tests

use assert_cmd::Command;

#[test]
fn version_prints_crate_name_and_version() {
    let out = Command::cargo_bin("readmd")
        .unwrap()
        .arg("--version")
        .output()
        .unwrap();
    assert!(out.status.success());
    let text = String::from_utf8(out.stdout).unwrap();
    assert_eq!(text.trim(), format!("readmd {}", env!("CARGO_PKG_VERSION")));
}

fn run(args: &[&std::ffi::OsStr]) -> std::process::Output {
    Command::cargo_bin("readmd")
        .unwrap()
        .args(args)
        .output()
        .unwrap()
}

fn tmpdir() -> tempfile::TempDir {
    tempfile::tempdir().unwrap()
}

#[test]
fn missing_path_exits_2_and_names_path() {
    let dir = tmpdir();
    let p = dir.path().join("gone.md");
    let out = run(&[p.as_os_str()]);
    assert_eq!(out.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&out.stderr).contains("gone.md"));
}

#[test]
fn unsupported_extension_exits_2_and_lists_supported() {
    let dir = tmpdir();
    let p = dir.path().join("a.txt");
    std::fs::write(&p, "x").unwrap();
    let out = run(&[p.as_os_str()]);
    assert_eq!(out.status.code(), Some(2));
    let err = String::from_utf8_lossy(&out.stderr);
    for e in [".md", ".markdown", ".mmd", ".mermaid"] {
        assert!(err.contains(e), "{err}");
    }
}

#[cfg(unix)]
#[test]
fn unreadable_file_exits_2() {
    use std::os::unix::fs::PermissionsExt;
    let dir = tmpdir();
    let p = dir.path().join("a.md");
    std::fs::write(&p, "x").unwrap();
    std::fs::set_permissions(&p, std::fs::Permissions::from_mode(0o000)).unwrap();
    if std::fs::File::open(&p).is_ok() {
        return; // running as root: permissions are not enforced
    }
    let out = run(&[p.as_os_str()]);
    std::fs::set_permissions(&p, std::fs::Permissions::from_mode(0o644)).unwrap();
    assert_eq!(out.status.code(), Some(2));
}
