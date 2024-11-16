use std::process::Command;

use assert_cmd::prelude::*;
use predicates::prelude::*;

#[test]
fn integration_test_rule002() {
    let mut cmd = Command::cargo_bin("supa-mdx-lint").unwrap();
    cmd.arg("tests/rule002/rule002.mdx")
        .arg("--config")
        .arg("tests/rule002/supa-mdx-lint.config.toml");
    cmd.assert()
        .failure()
        .stdout(predicate::str::contains("2 errors"));
}
