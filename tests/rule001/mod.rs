use std::process::Command;

use assert_cmd::prelude::*;
use predicates::prelude::*;

#[test]
fn integration_test_rule001_exception_including_initialism() {
    let mut cmd = Command::cargo_bin("supa-mdx-lint").unwrap();
    cmd.arg("tests/rule001/rule001.mdx")
        .arg("--config")
        .arg("tests/rule001/supa-mdx-lint.config.toml");

    cmd.assert()
        .success()
        .stdout(predicate::str::contains("No errors or warnings found"));
}
