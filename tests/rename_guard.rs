//! Guard for the mdpeek rename (SPEC §6.3): the old name survives only in the
//! history documents.

use std::process::Command;

/// The old name, assembled at run time so this file does not match itself.
fn old_name() -> String {
    concat!("read", "md").to_string()
}

#[test]
fn tracked_files_mentioning_the_old_name_are_only_changelog_and_spec() {
    let root = env!("CARGO_MANIFEST_DIR");
    let out = match Command::new("git")
        .args(["grep", "-il", &old_name()])
        .current_dir(root)
        .output()
    {
        Ok(o) => o,
        Err(e) => {
            eprintln!("skipped: git is unavailable ({e})");
            return;
        }
    };
    let code = out.status.code();
    if code != Some(0) && code != Some(1) {
        eprintln!("skipped: not a git work tree or git grep failed");
        return;
    }
    let found: Vec<String> = String::from_utf8_lossy(&out.stdout)
        .lines()
        .map(str::to_string)
        .collect();
    let unexpected: Vec<&String> = found
        .iter()
        .filter(|f| *f != "CHANGELOG.md" && *f != "SPEC.md")
        .collect();
    assert!(
        unexpected.is_empty(),
        "old name still mentioned in: {unexpected:?}"
    );
}
