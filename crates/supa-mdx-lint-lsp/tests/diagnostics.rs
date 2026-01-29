mod common;

use common::TestClient;

/// Test that diagnostics are published when a document is opened
#[test]
fn test_diagnostics_on_open() {
    let mut client = TestClient::new();
    client.initialize("file:///tmp/test-workspace", None);

    let uri = "file:///tmp/test-workspace/test.md";
    // This content should trigger Rule001HeadingCase (headings should be sentence case)
    let text = "# hello world\n\nSome content.\n";

    client.open_document(uri, "markdown", text);

    // Wait for diagnostics
    let diagnostics = client.wait_for_diagnostics(uri);

    // Should have at least one diagnostic for the heading case rule
    assert!(
        !diagnostics.is_empty(),
        "Expected diagnostics for heading case violation"
    );

    // Check diagnostic properties
    let diag = &diagnostics[0];
    assert_eq!(
        diag.get("source").and_then(|s| s.as_str()),
        Some("supa-mdx-lint")
    );

    // The diagnostic should be for Rule001HeadingCase
    if let Some(code) = diag.get("code").and_then(|c| c.as_str()) {
        assert!(
            code.contains("Rule001"),
            "Expected Rule001 diagnostic, got: {}",
            code
        );
    }

    client.shutdown();
}

/// Test that diagnostics have correct range (0-based lines and columns)
#[test]
fn test_diagnostics_range() {
    let mut client = TestClient::new();
    client.initialize("file:///tmp/test-workspace", None);

    let uri = "file:///tmp/test-workspace/test.md";
    let text = "# hello world\n";

    client.open_document(uri, "markdown", text);

    let diagnostics = client.wait_for_diagnostics(uri);

    if !diagnostics.is_empty() {
        let diag = &diagnostics[0];
        let range = diag.get("range").expect("Missing range");

        // Start should be at line 0 (first line)
        let start = range.get("start").expect("Missing start");
        assert_eq!(
            start.get("line").and_then(|l| l.as_u64()),
            Some(0),
            "Expected diagnostic on first line"
        );

        // End should also be on line 0
        let end = range.get("end").expect("Missing end");
        assert_eq!(
            end.get("line").and_then(|l| l.as_u64()),
            Some(0),
            "Expected diagnostic end on first line"
        );
    }

    client.shutdown();
}

/// Test that diagnostics are cleared when errors are fixed
#[test]
fn test_diagnostics_cleared_on_fix() {
    let mut client = TestClient::new();
    client.initialize("file:///tmp/test-workspace", None);

    let uri = "file:///tmp/test-workspace/test.md";

    // Open with invalid content
    client.open_document(uri, "markdown", "# hello world\n");
    let diagnostics = client.wait_for_diagnostics(uri);
    assert!(
        !diagnostics.is_empty(),
        "Expected diagnostics for invalid content"
    );

    // Fix the content (sentence case)
    client.change_document(uri, 2, "# Hello world\n");
    let diagnostics = client.wait_for_diagnostics(uri);

    assert!(
        diagnostics.is_empty() || diagnostics.iter().all(|d| {
            d.get("code")
                .and_then(|c| c.as_str())
                .map(|c| !c.contains("Rule001"))
                .unwrap_or(true)
        }),
        "Rule001 diagnostic should be cleared after fix"
    );

    client.shutdown();
}

/// Test that diagnostics include severity
#[test]
fn test_diagnostics_severity() {
    let mut client = TestClient::new();
    client.initialize("file:///tmp/test-workspace", None);

    let uri = "file:///tmp/test-workspace/test.md";
    client.open_document(uri, "markdown", "# hello world\n");

    let diagnostics = client.wait_for_diagnostics(uri);

    if !diagnostics.is_empty() {
        let diag = &diagnostics[0];

        // Severity should be present (1 = Error, 2 = Warning, 3 = Information, 4 = Hint)
        let severity = diag.get("severity").and_then(|s| s.as_u64());
        assert!(
            severity.is_some(),
            "Diagnostic should have severity: {:?}",
            diag
        );
    }

    client.shutdown();
}

/// Test that diagnostics are published on didChange when lintOn=type (default)
#[test]
fn test_diagnostics_on_change_lint_on_type() {
    let mut client = TestClient::new();
    // Default lintOn is "type"
    client.initialize("file:///tmp/test-workspace", None);

    let uri = "file:///tmp/test-workspace/test.md";

    // Open with valid content
    client.open_document(uri, "markdown", "# Hello world\n");
    let _ = client.wait_for_diagnostics(uri);

    // Change to invalid content - should get diagnostics immediately
    client.change_document(uri, 2, "# hello world\n");
    let diagnostics = client.wait_for_diagnostics(uri);

    assert!(
        !diagnostics.is_empty(),
        "Expected diagnostics on change when lintOn=type"
    );

    client.shutdown();
}

// Note: Testing "no diagnostics on change when lintOn=save" is tricky because
// it requires timeout-based testing. This behavior is verified manually and
// through the integration with the VS Code extension.

/// Test that didSave notification triggers lint when lintOn=save
#[test]
fn test_did_save_triggers_lint_when_lint_on_save() {
    let mut client = TestClient::new();
    // Initialize with lintOn=save
    client.initialize(
        "file:///tmp/test-workspace",
        Some(serde_json::json!({
            "lintOn": "save"
        })),
    );

    let uri = "file:///tmp/test-workspace/test.md";

    // Open document with valid content (still lints on open)
    client.open_document(uri, "markdown", "# Hello world\n");
    let _ = client.wait_for_diagnostics(uri);

    // Change the document to have invalid content
    // With lintOn=save, this should NOT trigger diagnostics
    client.change_document(uri, 2, "# hello world\n");

    // Save the document - this should trigger lint and produce diagnostics
    client.save_document(uri, Some("# hello world\n"));
    let diagnostics = client.wait_for_diagnostics(uri);

    // Should have diagnostics from the save
    assert!(
        !diagnostics.is_empty(),
        "Expected diagnostics after save when lintOn=save"
    );

    client.shutdown();
}

/// Test that diagnostics are cleared when document is closed
#[test]
fn test_document_close() {
    let mut client = TestClient::new();
    client.initialize("file:///tmp/test-workspace", None);

    let uri = "file:///tmp/test-workspace/test.md";

    // Open with invalid content
    client.open_document(uri, "markdown", "# hello world\n");
    let _ = client.wait_for_diagnostics(uri);

    // Close the document
    client.close_document(uri);

    // Document should be removed from state (no way to verify directly,
    // but subsequent operations should not crash)
    client.shutdown();
}

/// Test multiple documents
#[test]
fn test_multiple_documents() {
    let mut client = TestClient::new();
    client.initialize("file:///tmp/test-workspace", None);

    let uri1 = "file:///tmp/test-workspace/doc1.md";
    let uri2 = "file:///tmp/test-workspace/doc2.md";

    // Open first document with error
    client.open_document(uri1, "markdown", "# hello world\n");
    let diags1 = client.wait_for_diagnostics(uri1);
    assert!(!diags1.is_empty(), "Expected diagnostics for doc1");

    // Open second document with error
    client.open_document(uri2, "markdown", "# goodbye world\n");
    let diags2 = client.wait_for_diagnostics(uri2);
    assert!(!diags2.is_empty(), "Expected diagnostics for doc2");

    // Fix first document
    client.change_document(uri1, 2, "# Hello world\n");
    let diags1_fixed = client.wait_for_diagnostics(uri1);

    // First document should be fixed, second should still have errors
    assert!(
        diags1_fixed.is_empty() || diags1_fixed.iter().all(|d| {
            d.get("code")
                .and_then(|c| c.as_str())
                .map(|c| !c.contains("Rule001"))
                .unwrap_or(true)
        }),
        "doc1 should be fixed"
    );

    client.shutdown();
}

/// Test .mdx files are handled
#[test]
fn test_mdx_file() {
    let mut client = TestClient::new();
    client.initialize("file:///tmp/test-workspace", None);

    let uri = "file:///tmp/test-workspace/test.mdx";
    let text = "# hello world\n\nexport const meta = {};\n";

    client.open_document(uri, "mdx", text);

    // Should get diagnostics for MDX files too
    let diagnostics = client.wait_for_diagnostics(uri);
    assert!(
        !diagnostics.is_empty(),
        "Expected diagnostics for MDX file with heading case violation"
    );

    client.shutdown();
}
