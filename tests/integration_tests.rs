use assert_cmd::prelude::*;
use predicates::prelude::*;
use std::process::Command;

#[test]
fn integration_test_no_args() {
    let mut cmd = Command::cargo_bin("supa-mdx-lint").unwrap();
    cmd.assert().failure().stderr(predicate::str::contains(
        "the following required arguments were not provided",
    ));
}

#[test]
fn integration_test_good_file_target() {
    let mut cmd = Command::cargo_bin("supa-mdx-lint").unwrap();
    cmd.arg("tests/good001.mdx")
        .arg("--config")
        .arg("tests/supa-mdx-lint.config.toml");
    cmd.assert()
        .success()
        .stdout(predicate::str::contains("No errors or warnings found"));
}

#[test]
fn integration_test_bad_file_target() {
    let mut cmd = Command::cargo_bin("supa-mdx-lint").unwrap();
    cmd.arg("tests/bad001.mdx")
        .arg("--config")
        .arg("tests/supa-mdx-lint.config.toml");
    cmd.assert()
        .failure()
        .stdout(predicate::str::contains("Found 1 error"));
}

#[test]
fn integration_test_directory_target() {
    let mut cmd = Command::cargo_bin("supa-mdx-lint").unwrap();
    cmd.arg("tests")
        .arg("--config")
        .arg("tests/supa-mdx-lint.config.toml");
    cmd.assert()
        .failure()
        .stdout(predicate::str::contains("Found 1 error"));
}

#[test]
fn integration_test_silent() {
    let mut cmd = Command::cargo_bin("supa-mdx-lint").unwrap();
    cmd.arg("tests/good001.mdx")
        .arg("--config")
        .arg("tests/supa-mdx-lint.config.toml")
        .arg("--silent");
    cmd.assert().success().stdout(predicate::str::is_empty());
}
