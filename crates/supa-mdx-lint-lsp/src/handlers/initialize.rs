use std::path::PathBuf;

use anyhow::Result;
use log::info;
use lsp_types::InitializeParams;
use percent_encoding::percent_decode_str;

use crate::config::LspSettings;
use crate::state::ServerState;

/// Extract file path from a URI
///
/// Handles percent-decoding (e.g., `%20` -> space) and Windows drive letter paths.
fn uri_to_path(uri: &lsp_types::Uri) -> Option<PathBuf> {
    let path_str = uri.path().as_str();

    // Handle file:// URIs
    let is_file_scheme = uri.scheme().map(|s| s.as_str()) == Some("file");
    if is_file_scheme {
        // Percent-decode the path (e.g., %20 -> space)
        let decoded = percent_decode_str(path_str).decode_utf8().ok()?;

        // On Windows, the path might start with / followed by drive letter
        #[cfg(windows)]
        {
            let path_str = decoded.strip_prefix('/').unwrap_or(&decoded);
            Some(PathBuf::from(path_str))
        }
        #[cfg(not(windows))]
        {
            Some(PathBuf::from(decoded.as_ref()))
        }
    } else {
        None
    }
}

/// Initialize server state from InitializeParams
#[allow(deprecated)] // root_uri is deprecated but still widely used
pub fn initialize(state: &mut ServerState, params: InitializeParams) -> Result<()> {
    // Extract workspace root from workspace_folders (preferred) or root_uri (deprecated)
    if let Some(folders) = params.workspace_folders {
        if let Some(folder) = folders.first() {
            if let Some(path) = uri_to_path(&folder.uri) {
                info!("Workspace root from folders: {}", path.display());
                state.workspace_root = Some(path);
            }
        }
    } else if let Some(root_uri) = params.root_uri {
        // Fallback to deprecated root_uri
        if let Some(path) = uri_to_path(&root_uri) {
            info!("Workspace root from root_uri: {}", path.display());
            state.workspace_root = Some(path);
        }
    }

    // Parse initialization options into LspSettings
    if let Some(options) = params.initialization_options {
        match serde_json::from_value::<LspSettings>(options) {
            Ok(settings) => {
                info!("LSP settings: {:?}", settings);
                state.settings = settings;
            }
            Err(e) => {
                info!("Failed to parse initialization options, using defaults: {}", e);
            }
        }
    }

    // Discover and load config file
    state.discover_config()?;

    Ok(())
}
