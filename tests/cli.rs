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
