use std::{fs, process::Command};

use assert_cmd::prelude::*;
use predicates::prelude::*;
use tempfile::TempDir;

#[test]
fn integration_test_rule006_cli_errors() {
    let mut cmd = Command::cargo_bin("supa-mdx-lint").unwrap();
    cmd.arg("tests/rule006/rule006.mdx")
        .arg("--config")
        .arg("tests/rule006/supa-mdx-lint.config.toml");
    
    // Should find 4 errors:
    // 1. [Documentation](https://supabase.com/docs/auth)
    // 2. ![Logo](https://supabase.com/images/logo.png)  
    // 3. [https://supabase.com](https://supabase.com/docs/guides)
    // 4. [Home](https://supabase.com/)
    cmd.assert()
        .failure()
        .stdout(predicate::str::contains("4 errors"))
        .stdout(predicate::str::contains("Use relative URL '/docs/auth' instead of absolute URL 'https://supabase.com/docs/auth'"))
        .stdout(predicate::str::contains("Use relative URL '/images/logo.png' instead of absolute URL 'https://supabase.com/images/logo.png'"))
        .stdout(predicate::str::contains("Use relative URL '/docs/guides' instead of absolute URL 'https://supabase.com/docs/guides'"))
        .stdout(predicate::str::contains("Use relative URL '/' instead of absolute URL 'https://supabase.com/'"));
}

#[test]
fn integration_test_rule006_fix_mode() {
    let tempdir = TempDir::new().unwrap();
    
    // Create a test file with absolute URLs
    let test_content = r#"# Test URLs

Links to fix:
- [Documentation](https://supabase.com/docs/auth)
- ![Logo](https://supabase.com/images/logo.png)
- [Display URL](https://supabase.com/docs/guides)
- [Home](https://supabase.com/)

External URLs to keep unchanged:
- [Google](https://google.com/search) 
- [Example](https://example.com/test)

Already relative URLs:
- [Local](/local/path)
"#;

    let expected_content = r#"# Test URLs

Links to fix:
- [Documentation](/docs/auth)
- ![Logo](/images/logo.png)
- [Display URL](/docs/guides)
- [Home](/)

External URLs to keep unchanged:
- [Google](https://google.com/search) 
- [Example](https://example.com/test)

Already relative URLs:
- [Local](/local/path)
"#;

    fs::write(tempdir.path().join("test.mdx"), test_content).unwrap();

    let mut cmd = Command::cargo_bin("supa-mdx-lint").unwrap();
    cmd.arg(tempdir.path().join("test.mdx"))
        .arg("--config")
        .arg("tests/rule006/supa-mdx-lint.config.toml")
        .arg("--fix");
    
    cmd.assert().success();

    let result = fs::read_to_string(tempdir.path().join("test.mdx")).unwrap();
    assert_eq!(result, expected_content);
}

#[test]
fn integration_test_rule006_edge_cases() {
    let tempdir = TempDir::new().unwrap();
    
    // Test edge case where URL appears in both display text and href
    let test_content = r#"# Edge Cases

URL in display text and href:
[https://supabase.com](https://supabase.com/docs/auth)

Image with URL in alt and src:
![https://supabase.com](https://supabase.com/logo.png)

URL only in display text (should not change):
[https://supabase.com](https://example.com/external)
"#;

    let expected_content = r#"# Edge Cases

URL in display text and href:
[https://supabase.com](/docs/auth)

Image with URL in alt and src:
![https://supabase.com](/logo.png)

URL only in display text (should not change):
[https://supabase.com](https://example.com/external)
"#;

    fs::write(tempdir.path().join("edge_cases.mdx"), test_content).unwrap();

    let mut cmd = Command::cargo_bin("supa-mdx-lint").unwrap();
    cmd.arg(tempdir.path().join("edge_cases.mdx"))
        .arg("--config")
        .arg("tests/rule006/supa-mdx-lint.config.toml")
        .arg("--fix");
    
    cmd.assert().success();

    let result = fs::read_to_string(tempdir.path().join("edge_cases.mdx")).unwrap();
    assert_eq!(result, expected_content);
}

#[test]
fn integration_test_rule006_no_config() {
    let tempdir = TempDir::new().unwrap();
    
    // Create config without base_url - rule should not trigger
    let config_content = r#"Rule001HeadingCase = false
Rule002AdmonitionTypes = false
Rule003Spelling = false  
Rule004ExcludeWords = false
Rule005AdmonitionNewlines = false

[Rule006NoAbsoluteUrls]
# No base_url configured
"#;
    
    let test_content = r#"# Test
[Link](https://supabase.com/docs/auth)
"#;

    fs::write(tempdir.path().join("config.toml"), config_content).unwrap();
    fs::write(tempdir.path().join("test.mdx"), test_content).unwrap();

    let mut cmd = Command::cargo_bin("supa-mdx-lint").unwrap();
    cmd.arg(tempdir.path().join("test.mdx"))
        .arg("--config")
        .arg(tempdir.path().join("config.toml"));
    
    // Should pass with no errors since no base_url is configured
    cmd.assert()
        .success()
        .stdout(predicate::str::contains("No errors or warnings found"));
}