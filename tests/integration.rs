use assert_cmd::Command;
use std::fs;
use tempfile::TempDir;

fn make_fixture() -> TempDir {
    let dir = TempDir::new().unwrap();
    fs::write(dir.path().join("main.rs"), "fn main() {\n    println!(\"hi\");\n}\n").unwrap();
    fs::write(dir.path().join("lib.py"), "def foo():\n    pass\n").unwrap();
    dir
}

#[test]
fn runs_on_directory() {
    let dir = make_fixture();
    Command::cargo_bin("clocst").unwrap()
        .arg(dir.path())
        .assert()
        .success();
}

#[test]
fn accepts_highlight_languages_flag() {
    let dir = make_fixture();
    Command::cargo_bin("clocst").unwrap()
        .args([dir.path().to_str().unwrap(), "--highlight-languages", "2"])
        .assert()
        .success();
}

#[test]
fn accepts_no_ignore_flag() {
    let dir = make_fixture();
    Command::cargo_bin("clocst").unwrap()
        .args([dir.path().to_str().unwrap(), "--no-ignore"])
        .assert()
        .success();
}

#[test]
fn accepts_depth_flag() {
    let dir = make_fixture();
    Command::cargo_bin("clocst").unwrap()
        .args([dir.path().to_str().unwrap(), "--depth", "1"])
        .assert()
        .success();
}

#[test]
fn accepts_number_flag() {
    let dir = make_fixture();
    Command::cargo_bin("clocst").unwrap()
        .args([dir.path().to_str().unwrap(), "-n", "1"])
        .assert()
        .success();
}

#[test]
fn exits_cleanly_on_empty_dir() {
    let dir = TempDir::new().unwrap();
    Command::cargo_bin("clocst").unwrap()
        .arg(dir.path())
        .assert()
        .success();
}

#[test]
fn renders_root_percentage_and_others_legend() {
    let dir = make_fixture();
    let assert = Command::cargo_bin("clocst").unwrap()
        .arg(dir.path())
        .assert()
        .success();

    let stdout = String::from_utf8_lossy(&assert.get_output().stdout);
    assert!(stdout.contains("100%"));
    assert!(stdout.contains("Others"));
}
