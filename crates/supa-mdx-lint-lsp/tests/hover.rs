mod common;

use common::TestClient;
use serde_json::json;

/// Test that hover returns rule info when over a diagnostic
#[test]
fn test_hover_over_diagnostic() {
    let mut client = TestClient::new();
    client.initialize("file:///tmp/test-workspace", None);

    let uri = "file:///tmp/test-workspace/test.md";
    let text = "# hello world\n";

    client.open_document(uri, "markdown", text);

    // Wait for diagnostics first
    let diagnostics = client.wait_for_diagnostics(uri);
    assert!(!diagnostics.is_empty(), "Need diagnostics for hover test");

    // Hover over the heading (line 0, character 3 - in "hello")
    let hover = client.hover(uri, 0, 3);

    assert!(hover.is_some(), "Expected hover response over diagnostic");

    let hover = hover.unwrap();

    // Check that we got markdown content
    let contents = hover.get("contents").expect("Missing contents");

    // Content should be a MarkupContent or a string
    let value = if let Some(value) = contents.get("value").and_then(|v| v.as_str()) {
        value.to_string()
    } else if let Some(value) = contents.as_str() {
        value.to_string()
    } else {
        panic!("Unexpected hover contents format: {:?}", contents);
    };

    // Should contain rule name
    assert!(
        value.contains("Rule001"),
        "Hover should contain rule code, got: {}",
        value
    );

    // Should contain description
    assert!(
        value.contains("sentence case"),
        "Hover should contain rule description, got: {}",
        value
    );

    client.shutdown();
}

/// Test that hover returns null when not over a diagnostic
#[test]
fn test_hover_no_diagnostic() {
    let mut client = TestClient::new();
    client.initialize("file:///tmp/test-workspace", None);

    let uri = "file:///tmp/test-workspace/test.md";
    // Valid content with no diagnostics on line 2
    let text = "# Hello world\n\nSome valid content here.\n";

    client.open_document(uri, "markdown", text);

    // Wait for diagnostics (should be empty for valid content)
    let _ = client.wait_for_diagnostics(uri);

    // Hover over line 2 (the paragraph line) - should return null
    let hover = client.hover(uri, 2, 5);

    // Result should be null or empty
    assert!(
        hover.is_none() || hover == Some(json!(null)),
        "Expected no hover over non-diagnostic area, got: {:?}",
        hover
    );

    client.shutdown();
}

/// Test that hover returns correct range
#[test]
fn test_hover_range() {
    let mut client = TestClient::new();
    client.initialize("file:///tmp/test-workspace", None);

    let uri = "file:///tmp/test-workspace/test.md";
    let text = "# hello world\n";

    client.open_document(uri, "markdown", text);
    let _ = client.wait_for_diagnostics(uri);

    let hover = client.hover(uri, 0, 3);
    let hover = hover.expect("Expected hover response");

    // Hover should include a range
    if let Some(range) = hover.get("range") {
        let start = range.get("start").expect("Missing start");
        let end = range.get("end").expect("Missing end");

        // Range should be on line 0
        assert_eq!(start.get("line").and_then(|l| l.as_u64()), Some(0));
        assert_eq!(end.get("line").and_then(|l| l.as_u64()), Some(0));
    }

    client.shutdown();
}

/// Test hover after document change
#[test]
fn test_hover_after_change() {
    let mut client = TestClient::new();
    client.initialize("file:///tmp/test-workspace", None);

    let uri = "file:///tmp/test-workspace/test.md";

    // Start with invalid content
    client.open_document(uri, "markdown", "# hello world\n");
    let _ = client.wait_for_diagnostics(uri);

    // Hover should work initially
    let hover = client.hover(uri, 0, 3);
    assert!(hover.is_some(), "Expected hover before fix");

    // Fix the content
    client.change_document(uri, 2, "# Hello world\n");
    let _ = client.wait_for_diagnostics(uri);

    // Hover should now return null (no diagnostic)
    let hover = client.hover(uri, 0, 3);
    assert!(
        hover.is_none() || hover == Some(json!(null)),
        "Expected no hover after fix, got: {:?}",
        hover
    );

    client.shutdown();
}

/// Test hover on multiple diagnostics in same document
#[test]
fn test_hover_multiple_diagnostics() {
    let mut client = TestClient::new();
    client.initialize("file:///tmp/test-workspace", None);

    let uri = "file:///tmp/test-workspace/test.md";
    // Multiple headings with issues
    let text = "# hello world\n\nSome content.\n\n# another heading\n";

    client.open_document(uri, "markdown", text);
    let _ = client.wait_for_diagnostics(uri);

    // Hover over first heading
    let hover1 = client.hover(uri, 0, 3);
    assert!(hover1.is_some(), "Expected hover over first heading");

    // Hover over second heading (line 4)
    let hover2 = client.hover(uri, 4, 3);
    assert!(hover2.is_some(), "Expected hover over second heading");

    client.shutdown();
}
