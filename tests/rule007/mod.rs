use std::process::Command;

use assert_cmd::prelude::*;
use predicates::prelude::*;

#[test]
fn integration_test_rule007_reports_headings_in_admonitions() {
    let mut cmd = Command::cargo_bin("supa-mdx-lint").unwrap();
    cmd.arg("tests/rule007/rule007.mdx")
        .arg("--config")
        .arg("tests/rule007/supa-mdx-lint.config.toml");

    cmd.assert()
        .failure()
        .stdout(predicate::str::contains("2 errors"))
        .stdout(predicate::str::contains(
            "Admonitions cannot contain headings. Use the `title` prop for a callout title, or move the heading and section content outside the Admonition.",
        ));
}
