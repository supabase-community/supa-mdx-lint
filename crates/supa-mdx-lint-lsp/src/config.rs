use serde::Deserialize;

/// LSP-specific settings, passed via initializationOptions
#[derive(Debug, Default, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LspSettings {
    /// When to lint: "save" or "type"
    #[serde(default)]
    pub lint_on: LintOn,
}

#[derive(Debug, Clone, Default, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum LintOn {
    Save,
    #[default]
    Type,
}
