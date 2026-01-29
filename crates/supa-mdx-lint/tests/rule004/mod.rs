use std::process::Command;

use assert_cmd::prelude::*;
use predicates::prelude::*;

#[test]
fn integration_test_rule004() {
    let mut cmd = Command::cargo_bin("supa-mdx-lint").unwrap();
    cmd.arg("tests/rule004/rule004.mdx")
        .arg("--config")
        .arg("tests/rule004/supa-mdx-lint.config.toml");
    cmd.assert()
        .failure()
        .stdout(predicate::str::contains("2 errors"))
        .stdout(predicate::str::contains(
            "Don't use the following filler words",
        ));
}
