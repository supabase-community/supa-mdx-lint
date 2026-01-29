mod common;

use common::TestClient;
use serde_json::json;
use std::fs;
use tempfile::TempDir;

/// Test that the server handles configuration changes notification
/// Note: This test just verifies the notification is handled without error
#[test]
fn test_did_change_configuration() {
    let mut client = TestClient::new();
    client.initialize("file:///tmp/test-workspace", None);

    // Send configuration change notification with proper nesting
    client.notify(
        "workspace/didChangeConfiguration",
        json!({
            "settings": {
                "supa-mdx-lint": {
                    "lintOn": "save"
                }
            }
        }),
    );

    // The notification should be handled without error
    // Just verify basic operation continues to work
    let uri = "file:///tmp/test-workspace/test.md";
    client.open_document(uri, "markdown", "# hello world\n");
    let diagnostics = client.wait_for_diagnostics(uri);

    // Should still get diagnostics on open
    assert!(
        !diagnostics.is_empty(),
        "Should still get diagnostics on document open"
    );

    client.shutdown();
}

/// Test initialization with workspace that has a config file
#[test]
fn test_config_discovery_in_workspace() {
    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    let workspace_path = temp_dir.path();

    // Create a config file that sets Rule001 to warning level
    let config_content = r#"
[Rule001HeadingCase]
level = "warn"
"#;
    fs::write(
        workspace_path.join("supa-mdx-lint.config.toml"),
        config_content,
    )
    .expect("Failed to write config");

    let workspace_uri = format!("file://{}", workspace_path.display());

    let mut client = TestClient::new();
    client.initialize(&workspace_uri, None);

    // Open a document - config should be discovered and applied
    let uri = format!("file://{}/test.md", workspace_path.display());
    client.open_document(&uri, "markdown", "# hello world\n");

    let diagnostics = client.wait_for_diagnostics(&uri);

    // Should have diagnostics (config sets Rule001 to warn level)
    assert!(
        !diagnostics.is_empty(),
        "Expected diagnostics with config file"
    );

    // Check that severity is WARNING (2) since config sets it to warn
    let diag = &diagnostics[0];
    let severity = diag.get("severity").and_then(|s| s.as_u64());
    assert_eq!(
        severity,
        Some(2),
        "Expected WARNING severity from config, got: {:?}",
        severity
    );

    client.shutdown();
}

/// Test that invalid init options don't crash the server
#[test]
fn test_invalid_init_options() {
    let mut client = TestClient::new();

    // Pass invalid init options
    let result = client.initialize(
        "file:///tmp/test-workspace",
        Some(json!({
            "invalidOption": true,
            "lintOn": 12345  // Wrong type
        })),
    );

    // Server should still initialize (use defaults)
    assert!(
        result.get("capabilities").is_some(),
        "Server should initialize with invalid options"
    );

    client.shutdown();
}

/// Test workspace without config file uses defaults
#[test]
fn test_no_config_file_uses_defaults() {
    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    let workspace_uri = format!("file://{}", temp_dir.path().display());

    let mut client = TestClient::new();
    client.initialize(&workspace_uri, None);

    let uri = format!("file://{}/test.md", temp_dir.path().display());
    client.open_document(&uri, "markdown", "# hello world\n");

    let diagnostics = client.wait_for_diagnostics(&uri);

    // Should still get diagnostics with default settings
    assert!(
        !diagnostics.is_empty(),
        "Expected diagnostics with default config"
    );

    // Default severity for Rule001 is ERROR (1)
    let diag = &diagnostics[0];
    let severity = diag.get("severity").and_then(|s| s.as_u64());
    assert_eq!(
        severity,
        Some(1),
        "Expected ERROR severity by default, got: {:?}",
        severity
    );

    client.shutdown();
}

/// Test settings from initializationOptions are accepted
#[test]
fn test_lint_on_from_init_options() {
    let mut client = TestClient::new();

    // Initialize with lintOn=save (server should accept this)
    let result = client.initialize(
        "file:///tmp/test-workspace",
        Some(json!({
            "lintOn": "save"
        })),
    );

    // Server should initialize successfully
    assert!(result.get("capabilities").is_some());

    // Verify basic operation works
    let uri = "file:///tmp/test-workspace/test.md";
    client.open_document(uri, "markdown", "# hello world\n");
    let diagnostics = client.wait_for_diagnostics(uri);

    // Should still get diagnostics on open regardless of lintOn setting
    assert!(
        !diagnostics.is_empty(),
        "Should get diagnostics on document open"
    );

    client.shutdown();
}

/// Test config file with rule disabled
#[test]
fn test_config_disables_rule() {
    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    let workspace_path = temp_dir.path();

    // Create a config file that disables Rule001
    // Rules are at top level, not in a [rules] section
    let config_content = r#"
Rule001HeadingCase = false
"#;
    fs::write(
        workspace_path.join("supa-mdx-lint.config.toml"),
        config_content,
    )
    .expect("Failed to write config");

    let workspace_uri = format!("file://{}", workspace_path.display());

    let mut client = TestClient::new();
    client.initialize(&workspace_uri, None);

    let uri = format!("file://{}/test.md", workspace_path.display());
    client.open_document(&uri, "markdown", "# hello world\n");

    let diagnostics = client.wait_for_diagnostics(&uri);

    // Should have no Rule001 diagnostics since it's disabled
    let rule001_diags: Vec<_> = diagnostics
        .iter()
        .filter(|d| {
            d.get("code")
                .and_then(|c| c.as_str())
                .map(|c| c.contains("Rule001"))
                .unwrap_or(false)
        })
        .collect();

    assert!(
        rule001_diags.is_empty(),
        "Rule001 should be disabled by config, but got: {:?}",
        rule001_diags
    );

    client.shutdown();
}
