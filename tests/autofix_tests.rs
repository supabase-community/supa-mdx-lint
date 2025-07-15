use std::{fs, process::Command};

use assert_cmd::prelude::*;
use tempfile::TempDir;

#[test]
fn test_autofix_good_file_is_noop() {
    let tempdir = TempDir::new().unwrap();
    let good_file = r#"# Nothing wrong with this

Nothing to see here, everything is nice and dandy."#;
    fs::write(tempdir.path().join("good.mdx"), good_file).unwrap();

    let mut cmd = Command::cargo_bin("supa-mdx-lint").unwrap();
    cmd.arg(tempdir.path().join("good.mdx"))
        .arg("--config")
        .arg("tests/supa-mdx-lint.config.toml")
        .arg("--fix");
    cmd.assert().success();

    let result = fs::read_to_string(tempdir.path().join("good.mdx")).unwrap();
    assert_eq!(result, good_file);
}

#[test]
fn test_autofix_bad_file() {
    let tempdir = TempDir::new().unwrap();
    let bad_file = r#"# This Is Bad

This is bad, and should be fixed."#;
    fs::write(tempdir.path().join("bad.mdx"), bad_file).unwrap();

    let mut cmd = Command::cargo_bin("supa-mdx-lint").unwrap();
    cmd.arg(tempdir.path().join("bad.mdx"))
        .arg("--config")
        .arg("tests/supa-mdx-lint.config.toml")
        .arg("--fix");
    cmd.assert().success();

    let result = fs::read_to_string(tempdir.path().join("bad.mdx")).unwrap();
    assert_eq!(
        result,
        r#"# This is bad

This is bad, and should be fixed."#
    );
}

#[test]
fn test_autofix_unicode() {
    let tempdir = TempDir::new().unwrap();
    let bad_file = r#"# This Is 🔴错误 Bad

This is bad, and should be fixed."#;
    fs::write(tempdir.path().join("bad.mdx"), bad_file).unwrap();

    let mut cmd = Command::cargo_bin("supa-mdx-lint").unwrap();
    cmd.arg(tempdir.path().join("bad.mdx"))
        .arg("--config")
        .arg("tests/supa-mdx-lint.config.toml")
        .arg("--fix");
    cmd.assert().success();

    let result = fs::read_to_string(tempdir.path().join("bad.mdx")).unwrap();
    println!("{}", result);
    assert_eq!(
        result,
        r#"# This is 🔴错误 bad

This is bad, and should be fixed."#
    );
}

#[test]
fn test_autofix_with_frontmatter() {
    let tempdir = TempDir::new().unwrap();
    let bad_file = r#"---
title: Something
---

# This Is Bad

This is bad, and should be fixed."#;
    fs::write(tempdir.path().join("bad.mdx"), bad_file).unwrap();

    let mut cmd = Command::cargo_bin("supa-mdx-lint").unwrap();
    cmd.arg(tempdir.path().join("bad.mdx"))
        .arg("--config")
        .arg("tests/supa-mdx-lint.config.toml")
        .arg("--fix");
    cmd.assert().success();

    let result = fs::read_to_string(tempdir.path().join("bad.mdx")).unwrap();
    println!("{}", result);
    assert_eq!(
        result,
        r#"---
title: Something
---

# This is bad

This is bad, and should be fixed."#
    );
}

#[test]
fn test_autofix_rule005_admonition_newlines() {
    let tempdir = TempDir::new().unwrap();
    let bad_file = r#"# Test admonition newlines

<Admonition type="caution">
This content is missing newlines around it.
</Admonition>

<Admonition type="info">

This one is missing the closing newline.
</Admonition>

<Admonition type="warning">
This one is missing the opening newline.

</Admonition>"#;
    fs::write(tempdir.path().join("bad.mdx"), bad_file).unwrap();

    let mut cmd = Command::cargo_bin("supa-mdx-lint").unwrap();
    cmd.arg(tempdir.path().join("bad.mdx"))
        .arg("--config")
        .arg("tests/supa-mdx-lint.config.toml")
        .arg("--fix");
    cmd.assert().success();

    let result = fs::read_to_string(tempdir.path().join("bad.mdx")).unwrap();
    assert_eq!(
        result,
        r#"# Test admonition newlines

<Admonition type="caution">

This content is missing newlines around it.

</Admonition>

<Admonition type="info">

This one is missing the closing newline.

</Admonition>

<Admonition type="warning">

This one is missing the opening newline.

</Admonition>"#
    );
}

#[test]
fn test_autofix_rule005_admonition_newlines_with_frontmatter() {
    let tempdir = TempDir::new().unwrap();
    let bad_file = r#"---
title: Test Document
description: Testing admonition auto-fix with frontmatter
author: Test Author
---

# Test admonition newlines with frontmatter

<Admonition type="caution">
This content is missing newlines around it.
</Admonition>

<Admonition type="info">

This one is missing the closing newline.
</Admonition>

<Admonition type="warning">
This one is missing the opening newline.

</Admonition>"#;
    fs::write(tempdir.path().join("bad.mdx"), bad_file).unwrap();

    let mut cmd = Command::cargo_bin("supa-mdx-lint").unwrap();
    cmd.arg(tempdir.path().join("bad.mdx"))
        .arg("--config")
        .arg("tests/supa-mdx-lint.config.toml")
        .arg("--fix");
    cmd.assert().success();

    let result = fs::read_to_string(tempdir.path().join("bad.mdx")).unwrap();
    assert_eq!(
        result,
        r#"---
title: Test Document
description: Testing admonition auto-fix with frontmatter
author: Test Author
---

# Test admonition newlines with frontmatter

<Admonition type="caution">

This content is missing newlines around it.

</Admonition>

<Admonition type="info">

This one is missing the closing newline.

</Admonition>

<Admonition type="warning">

This one is missing the opening newline.

</Admonition>"#
    );
}

#[test]
fn test_autofix_admonition_and_word_replace_offset_bug() {
    let tempdir = TempDir::new().unwrap();
    let bad_file = r#"# Test

<Admonition type=\"caution\">
This is the content.
</Admonition>

Some text to be replaced."#;
    fs::write(tempdir.path().join("bad.mdx"), bad_file).unwrap();

    let mut cmd = Command::cargo_bin("supa-mdx-lint").unwrap();
    cmd.arg(tempdir.path().join("bad.mdx"))
        .arg("--config")
        .arg("tests/supa-mdx-lint.config.toml")
        .arg("--fix");
    cmd.assert().success();

    let result = fs::read_to_string(tempdir.path().join("bad.mdx")).unwrap();
    // The expected output assumes both fixes are applied at the correct locations
    assert_eq!(
        result,
        r#"# Test

<Admonition type=\"caution\">

This is the content.

</Admonition>

Some text to be replaced."#
    );
}
