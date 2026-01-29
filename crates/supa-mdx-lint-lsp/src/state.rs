use std::collections::HashMap;
use std::path::PathBuf;

use anyhow::Result;
use log::{info, warn};
use lsp_types::Uri;
use supa_mdx_lint::{Config, LintOutput, Linter};

use crate::config::LspSettings;

/// State maintained by the LSP server
pub struct ServerState {
    /// Open document contents, keyed by URI
    pub documents: HashMap<Uri, DocumentState>,

    /// Workspace root path
    pub workspace_root: Option<PathBuf>,

    /// Path to discovered config file (if any)
    pub config_path: Option<PathBuf>,

    /// The linter instance (rebuilt when config changes)
    pub linter: Option<Linter>,

    /// LSP-specific settings
    pub settings: LspSettings,
}

/// State for a single open document
pub struct DocumentState {
    /// Current document content
    pub content: String,

    /// Most recent lint output (needed for hover to find rule info)
    pub lint_output: Vec<LintOutput>,
}

impl ServerState {
    pub fn new() -> Self {
        Self {
            documents: HashMap::new(),
            workspace_root: None,
            config_path: None,
            linter: None,
            settings: LspSettings::default(),
        }
    }

    /// Discover and load config file from workspace root
    pub fn discover_config(&mut self) -> Result<()> {
        let Some(workspace_root) = &self.workspace_root else {
            warn!("No workspace root set, cannot discover config");
            // Create linter with default config
            self.linter = Some(Linter::builder().build()?);
            return Ok(());
        };

        let workspace_root = workspace_root.canonicalize().unwrap_or(workspace_root.clone());
        self.workspace_root = Some(workspace_root.clone());

        let config_path = workspace_root.join("supa-mdx-lint.config.toml");
        if config_path.exists() {
            info!("Found config file at: {}", config_path.display());
            self.config_path = Some(config_path.clone());

            let config = Config::from_config_file(&config_path)?;
            self.linter = Some(Linter::builder().config(config).build()?);
        } else {
            info!("No config file found at: {}", config_path.display());
            // Create linter with default config
            self.linter = Some(Linter::builder().build()?);
        }

        Ok(())
    }

    /// Reload config and return list of document URIs that need re-linting
    pub fn reload_config(&mut self) -> Result<Vec<Uri>> {
        self.discover_config()?;

        // Return all open document URIs so they can be re-linted
        Ok(self.documents.keys().cloned().collect())
    }
}

impl Default for ServerState {
    fn default() -> Self {
        Self::new()
    }
}
