mod common;

use common::TestClient;
use serde_json::json;

#[test]
fn test_initialize_returns_capabilities() {
    let mut client = TestClient::new();

    let result = client.initialize("file:///tmp/test-workspace", None);

    // Check server info
    let server_info = result.get("serverInfo").expect("Missing serverInfo");
    assert_eq!(
        server_info.get("name").and_then(|n| n.as_str()),
        Some("supa-mdx-lint-lsp")
    );
    assert!(server_info.get("version").is_some());

    // Check capabilities
    let capabilities = result.get("capabilities").expect("Missing capabilities");

    // Should have text document sync
    let text_doc_sync = capabilities
        .get("textDocumentSync")
        .expect("Missing textDocumentSync");
    assert!(text_doc_sync.get("openClose").and_then(|v| v.as_bool()) == Some(true));
    assert!(text_doc_sync.get("change").and_then(|v| v.as_i64()) == Some(1)); // FULL sync

    // Should have hover provider
    assert_eq!(
        capabilities.get("hoverProvider"),
        Some(&json!(true)),
        "Expected hover provider to be true"
    );

    client.shutdown();
}

#[test]
fn test_initialize_with_init_options() {
    let mut client = TestClient::new();

    let result = client.initialize(
        "file:///tmp/test-workspace",
        Some(json!({
            "lintOn": "save"
        })),
    );

    // Server should accept the init options without error
    assert!(result.get("capabilities").is_some());

    client.shutdown();
}

#[test]
fn test_initialize_with_workspace_folders() {
    let mut client = TestClient::new();

    // Use workspace_folders instead of root_uri
    let params = json!({
        "processId": std::process::id(),
        "capabilities": {},
        "workspaceFolders": [{
            "uri": "file:///tmp/test-workspace",
            "name": "test"
        }]
    });

    let response = client.request("initialize", params);
    assert!(
        response.error.is_none(),
        "Initialize failed: {:?}",
        response.error
    );

    client.notify("initialized", json!({}));
    client.shutdown();
}

#[test]
fn test_shutdown_and_exit() {
    let mut client = TestClient::new();
    client.initialize("file:///tmp/test-workspace", None);

    // Shutdown should succeed
    let response = client.request("shutdown", json!(null));
    assert!(
        response.error.is_none(),
        "Shutdown failed: {:?}",
        response.error
    );

    // Exit notification
    client.notify("exit", json!(null));

    // Give the process time to exit
    std::thread::sleep(std::time::Duration::from_millis(100));
}
