//! `make release <version>` (`.github/scripts/release.sh`): bumps the
//! version, merges develop into main and tags it, never pushes (KB-104).

#![allow(clippy::unwrap_used, clippy::expect_used)] // panics are fine in tests

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

fn script() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(".github/scripts/release.sh")
}

fn git(dir: &Path, args: &[&str]) -> String {
    let out = Command::new("git")
        .args(args)
        .current_dir(dir)
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "git {args:?}: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8(out.stdout).unwrap().trim().to_owned()
}

fn configure(dir: &Path) {
    for (k, v) in [
        ("user.name", "Test"),
        ("user.email", "test@example.com"),
        ("commit.gpgsign", "false"),
        ("tag.gpgsign", "false"),
        ("init.defaultBranch", "main"),
    ] {
        git(dir, &["config", k, v]);
    }
}

const MANIFEST: &str = "[package]\nname = \"demo\"\nversion = \"0.1.0\"\n\n[dependencies]\nother = { version = \"0.1.0\" }\n";
const LOCK: &str = "version = 4\n\n[[package]]\nname = \"demo\"\nversion = \"0.1.0\"\n\n[[package]]\nname = \"other\"\nversion = \"0.1.0\"\n";
const CHANGELOG: &str = "# Changelog\n\n## [Unreleased]\n\n### Fixed\n\n- A bug.\n\n## [0.1.0] - 2026-01-01\n\n- First.\n";

/// A clone with `main` and `develop` in sync with a bare origin; develop is
/// one commit ahead of main.
struct Repo {
    _tmp: tempfile::TempDir,
    work: PathBuf,
    origin: PathBuf,
}

fn repo(changelog: &str) -> Repo {
    let tmp = tempfile::tempdir().unwrap();
    let origin = tmp.path().join("origin.git");
    let work = tmp.path().join("work");
    std::fs::create_dir_all(&work).unwrap();
    git(
        tmp.path(),
        &["init", "-q", "--bare", "-b", "main", "origin.git"],
    );
    git(&work, &["init", "-q", "-b", "main"]);
    configure(&work);
    git(
        &work,
        &["remote", "add", "origin", origin.to_str().unwrap()],
    );
    std::fs::write(work.join("Cargo.toml"), MANIFEST).unwrap();
    std::fs::write(work.join("Cargo.lock"), LOCK).unwrap();
    std::fs::write(work.join("CHANGELOG.md"), changelog).unwrap();
    git(&work, &["add", "."]);
    git(&work, &["commit", "-qm", "init"]);
    git(&work, &["tag", "-a", "v0.1.0", "-m", "0.1.0"]);
    git(&work, &["switch", "-qc", "develop"]);
    std::fs::write(work.join("feature.txt"), "x\n").unwrap();
    git(&work, &["add", "."]);
    git(&work, &["commit", "-qm", "feature"]);
    git(
        &work,
        &["push", "-q", "origin", "main", "develop", "v0.1.0"],
    );
    git(&work, &["branch", "-q", "-u", "origin/develop"]);
    Repo {
        _tmp: tmp,
        work,
        origin,
    }
}

fn release(dir: &Path, args: &[&str], check: &str) -> Output {
    Command::new("sh")
        .arg(script())
        .args(args)
        .env("RELEASE_CHECK", check)
        .current_dir(dir)
        .output()
        .unwrap()
}

fn read(dir: &Path, file: &str) -> String {
    std::fs::read_to_string(dir.join(file)).unwrap()
}

fn today() -> String {
    let out = Command::new("date").arg("+%F").output().unwrap();
    String::from_utf8(out.stdout).unwrap().trim().to_owned()
}

#[test]
fn release_bumps_merges_into_main_and_tags_without_pushing() {
    let r = repo(CHANGELOG);
    let origin_before = git(&r.origin, &["show-ref"]);
    let out = release(&r.work, &["0.2.0"], "true");
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(out.status.success(), "{stderr}");

    assert_eq!(git(&r.work, &["branch", "--show-current"]), "develop");
    assert!(
        read(&r.work, "Cargo.toml").contains("[package]\nname = \"demo\"\nversion = \"0.2.0\"\n")
    );
    assert!(read(&r.work, "Cargo.toml").contains("other = { version = \"0.1.0\" }"));
    let lock = read(&r.work, "Cargo.lock");
    assert!(
        lock.contains("name = \"demo\"\nversion = \"0.2.0\"\n"),
        "{lock}"
    );
    assert!(
        lock.contains("name = \"other\"\nversion = \"0.1.0\"\n"),
        "{lock}"
    );
    assert!(lock.starts_with("version = 4\n"), "{lock}");
    assert!(read(&r.work, "CHANGELOG.md").contains(&format!(
        "## [Unreleased]\n\n## [0.2.0] - {}\n\n### Fixed\n\n- A bug.\n",
        today()
    )));

    assert_eq!(
        git(&r.work, &["log", "-1", "--format=%s", "develop"]),
        "chore(release): 0.2.0"
    );
    assert_eq!(
        git(&r.work, &["rev-parse", "v0.2.0^{commit}"]),
        git(&r.work, &["rev-parse", "main"])
    );
    assert_eq!(
        git(&r.work, &["rev-parse", "main^2"]),
        git(&r.work, &["rev-parse", "develop"])
    );
    assert_eq!(
        git(&r.work, &["rev-parse", "main^{tree}"]),
        git(&r.work, &["rev-parse", "develop^{tree}"])
    );
    assert_eq!(git(&r.work, &["cat-file", "-t", "v0.2.0"]), "tag");

    assert_eq!(
        git(&r.origin, &["show-ref"]),
        origin_before,
        "nothing is pushed"
    );
    assert!(
        String::from_utf8_lossy(&out.stdout)
            .contains("git push --atomic origin develop main v0.2.0"),
        "{}",
        String::from_utf8_lossy(&out.stdout)
    );
}

/// Runs a refused release and checks that nothing changed.
fn refused(r: &Repo, args: &[&str], check: &str, reason: &str) {
    let develop = git(&r.work, &["rev-parse", "develop"]);
    let main = git(&r.work, &["rev-parse", "main"]);
    let tags = git(&r.work, &["tag"]);
    let out = release(&r.work, args, check);
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(!out.status.success(), "{args:?} should fail");
    assert!(
        stderr.contains(reason),
        "{args:?}: expected {reason:?} in {stderr}"
    );
    assert_eq!(git(&r.work, &["rev-parse", "develop"]), develop);
    assert_eq!(git(&r.work, &["rev-parse", "main"]), main);
    assert_eq!(git(&r.work, &["tag"]), tags);
    assert_eq!(git(&r.work, &["status", "--porcelain"]), "");
}

#[test]
fn release_refuses_a_missing_or_malformed_version() {
    let r = repo(CHANGELOG);
    refused(&r, &[], "true", "usage: make release <version>");
    refused(&r, &["v0.2.0"], "true", "0.2.0 style");
    refused(&r, &["0.2"], "true", "0.2.0 style");
}

#[test]
fn release_refuses_a_version_that_is_not_greater() {
    let r = repo(CHANGELOG);
    refused(&r, &["0.1.0"], "true", "is not greater than 0.1.0");
    refused(&r, &["0.0.9"], "true", "is not greater than 0.1.0");
}

#[test]
fn release_refuses_an_existing_tag() {
    let r = repo(CHANGELOG);
    git(&r.work, &["tag", "-a", "v0.3.0", "-m", "x", "main"]);
    refused(&r, &["0.3.0"], "true", "tag v0.3.0 already exists");
}

#[test]
fn release_refuses_another_branch_or_a_dirty_tree() {
    let r = repo(CHANGELOG);
    git(&r.work, &["switch", "-q", "main"]);
    refused(&r, &["0.2.0"], "true", "run it on develop");
    git(&r.work, &["switch", "-q", "develop"]);
    std::fs::write(r.work.join("feature.txt"), "changed\n").unwrap();
    let out = release(&r.work, &["0.2.0"], "true");
    assert!(!out.status.success());
    assert!(String::from_utf8_lossy(&out.stderr).contains("uncommitted changes"));
    assert!(git(&r.work, &["tag"]).lines().all(|t| t != "v0.2.0"));
}

#[test]
fn release_refuses_branches_out_of_sync_with_origin() {
    let r = repo(CHANGELOG);
    std::fs::write(r.work.join("feature.txt"), "local\n").unwrap();
    git(&r.work, &["commit", "-qam", "not pushed"]);
    refused(
        &r,
        &["0.2.0"],
        "true",
        "develop differs from origin/develop",
    );
}

#[test]
fn release_refuses_a_main_with_commits_not_in_develop() {
    let r = repo(CHANGELOG);
    git(&r.work, &["switch", "-q", "main"]);
    std::fs::write(r.work.join("hotfix.txt"), "x\n").unwrap();
    git(&r.work, &["add", "."]);
    git(&r.work, &["commit", "-qm", "hotfix"]);
    git(&r.work, &["push", "-q", "origin", "main"]);
    git(&r.work, &["switch", "-q", "develop"]);
    refused(
        &r,
        &["0.2.0"],
        "true",
        "main has commits that are not in develop",
    );
}

#[test]
fn release_refuses_an_empty_unreleased_section() {
    let r = repo("# Changelog\n\n## [Unreleased]\n\n## [0.1.0] - 2026-01-01\n\n- First.\n");
    refused(
        &r,
        &["0.2.0"],
        "true",
        "[Unreleased] in CHANGELOG.md is empty",
    );
}

#[test]
fn release_with_a_failing_check_restores_the_files() {
    let r = repo(CHANGELOG);
    refused(&r, &["0.2.0"], "false", "make check failed");
    assert_eq!(read(&r.work, "Cargo.toml"), MANIFEST);
    assert_eq!(read(&r.work, "CHANGELOG.md"), CHANGELOG);
}

/// `make release 9.9.9`: the version is a plain argument, not a target.
#[test]
fn make_release_takes_the_version_as_an_argument() {
    let out = Command::new("make")
        .args(["-n", "release", "9.9.9"])
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .output()
        .unwrap();
    let recipe = String::from_utf8_lossy(&out.stdout);
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    // `-n` also echoes the silent no-op recipe of the version word (`:`).
    assert_eq!(
        recipe.lines().next(),
        Some("sh .github/scripts/release.sh 9.9.9"),
        "{recipe}"
    );
}
