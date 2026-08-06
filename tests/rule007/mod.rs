use std::{path::MAIN_SEPARATOR_STR, process::Command};

use assert_cmd::prelude::*;
use predicates::prelude::*;

fn assert_heading_error(fixture: &str) {
    let expected_location = format!("{}:3:1", fixture.replace('/', MAIN_SEPARATOR_STR));
    let mut cmd = Command::cargo_bin("supa-mdx-lint").unwrap();
    cmd.arg(fixture)
        .arg("--config")
        .arg("tests/rule007/supa-mdx-lint.config.toml");

    cmd.assert()
        .failure()
        .stdout(predicate::str::contains("1 error"))
        .stdout(predicate::str::contains(expected_location))
        .stdout(predicate::str::contains(
            "Admonitions cannot contain headings. Use the `title` prop for a callout title, or move the heading and section content outside the Admonition.",
        ));
}

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

#[test]
fn integration_test_rule007_reports_markdown_heading() {
    assert_heading_error("tests/rule007/markdown_heading.mdx");
}

#[test]
fn integration_test_rule007_reports_jsx_heading() {
    assert_heading_error("tests/rule007/jsx_heading.mdx");
}
