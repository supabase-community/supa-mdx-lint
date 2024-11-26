use std::process::Command;

use assert_cmd::prelude::*;
use predicates::prelude::*;

#[test]
fn integration_test_rule003() {
    let mut cmd = Command::cargo_bin("supa-mdx-lint").unwrap();
    cmd.arg("tests/rule003/rule003.mdx")
        .arg("--config")
        .arg("tests/rule003/supa-mdx-lint.config.toml");
    cmd.assert()
        .failure()
        .stdout(predicate::str::contains("1 error"))
        .stdout(predicate::str::contains("Word not found in dictionary"));
}
