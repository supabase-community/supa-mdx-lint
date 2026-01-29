use log::{debug, info};
use lsp_types::DidChangeConfigurationParams;

use crate::config::LspSettings;
use crate::state::ServerState;

/// Handle workspace/didChangeConfiguration notification
pub fn handle_did_change_configuration(
    state: &mut ServerState,
    params: DidChangeConfigurationParams,
) {
    debug!("Configuration changed: {:?}", params.settings);

    // Try to extract supa-mdx-lint settings from the configuration
    if let Some(settings) = params.settings.get("supa-mdx-lint") {
        match serde_json::from_value::<LspSettings>(settings.clone()) {
            Ok(new_settings) => {
                info!("Updated LSP settings: {:?}", new_settings);
                state.settings = new_settings;
            }
            Err(e) => {
                debug!("Failed to parse configuration settings: {}", e);
            }
        }
    }
}
