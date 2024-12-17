use assert_cmd::prelude::*;
use predicates::prelude::*;
use std::process::Command;

#[test]
fn integration_test_no_args() {
    let mut cmd = Command::cargo_bin("supa-mdx-lint").unwrap();
    cmd.assert().failure().stderr(predicate::str::contains(
        "The following required arguments were not provided",
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
        .stdout(predicate::str::contains("Found 2 errors"));
}

#[test]
fn integration_test_directory_target() {
    let mut cmd = Command::cargo_bin("supa-mdx-lint").unwrap();
    cmd.arg("tests")
        .arg("--config")
        .arg("tests/supa-mdx-lint.config.toml");
    cmd.assert()
        .failure()
        .stdout(predicate::str::contains("4 sources linted"))
        .stdout(predicate::str::contains("Found 2 errors"));
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

#[test]
fn integration_test_globs() {
    let mut cmd = Command::cargo_bin("supa-mdx-lint").unwrap();
    cmd.arg("tests/good*.mdx")
        .arg("--config")
        .arg("tests/supa-mdx-lint.config.toml");
    cmd.assert()
        .success()
        .stdout(predicate::str::contains("2 sources linted"));
}

#[test]
fn integration_test_file_level_disables() {
    let mut cmd = Command::cargo_bin("supa-mdx-lint").unwrap();
    cmd.arg("test/nested/good003.mdx")
        .arg("--config")
        .arg("tests/supa-mdx-lint.config.toml");
    cmd.assert()
        .success()
        .stdout(predicate::str::contains("No errors or warnings found"));
}

#[test]
fn integration_test_multiple_targets() {
    let mut cmd = Command::cargo_bin("supa-mdx-lint").unwrap();
    cmd.arg("tests/good001.mdx")
        .arg("tests/bad001.mdx")
        .arg("--config")
        .arg("tests/supa-mdx-lint.config.toml");
    cmd.assert()
        .failure()
        .stdout(predicate::str::contains("2 sources linted"))
        .stdout(predicate::str::contains("Found 2 errors"));
}

#[test]
fn integration_test_rdf_no_extra_logs() {
    let mut cmd = Command::cargo_bin("supa-mdx-lint").unwrap();
    cmd.arg("tests/good001.mdx")
        .arg("--config")
        .arg("tests/supa-mdx-lint.config.toml")
        .arg("--format")
        .arg("rdf");
    cmd.assert().success().stdout(predicate::str::is_empty());
}
