use anyhow::Result;
use glob::Pattern;
use log::{debug, error, warn};
use std::{
    collections::{HashMap, HashSet},
    env,
    path::{Path, PathBuf},
};

use crate::{
    errors::LintLevel,
    rules::{RuleRegistry, RuleSettings},
};

const IGNORE_GLOBS_KEY: &str = "ignore_patterns";

#[derive(Debug, Clone)]
pub struct ConfigDir(pub Option<PathBuf>);

#[derive(Debug)]
pub struct Config {
    pub(crate) rule_registry: RuleRegistry,
    pub(crate) rule_specific_settings: HashMap<String, RuleSettings>,
    /// A list of globs to ignore.
    ignore_globs: HashSet<Pattern>,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            rule_registry: RuleRegistry::new(),
            rule_specific_settings: HashMap::new(),
            ignore_globs: HashSet::new(),
        }
    }
}

impl Config {
    /// Read the rule configuration from a TOML file.
    ///
    /// The configuration file is a TOML file that contains a table of rule
    /// settings. Each rule has a unique name, and each rule can have a set of
    /// settings that are specific to that rule.
    ///
    /// The setting named `level` is reserved for setting the rule's severity level.
    ///
    /// Rules can be turned off by setting the rule to `false`.
    ///
    /// The configuration file can also include other files using the `include()`
    /// function. This allows for modular configuration, where each rule can be
    /// defined in a separate file, and then included into the main configuration
    /// file.
    ///
    /// Example:
    ///
    /// ```toml
    /// [Rule001SomeRule]
    /// level = "error"
    /// option1 = true
    /// option2 = "value"
    ///
    /// Rule002SomeOtherRule = "include('some_other_rule.toml')"
    ///
    /// Rule003NotApplied = false
    /// ```
    #[cfg(not(target_arch = "wasm32"))]
    pub fn from_config_file<P: AsRef<Path>>(config_file: P) -> Result<Self> {
        let config_path = config_file.as_ref().to_path_buf();
        let config_dir = config_path.parent().ok_or_else(|| {
            anyhow::anyhow!("Unable to determine parent directory of config file: {config_path:?}")
        })?;

        let config_content = std::fs::read_to_string(&config_path)
            .inspect_err(|_| error!("Failed to read config file at {config_path:?}"))?;
        let parsed = Self::process_includes(&config_content, config_dir).inspect_err(|_| {
            error!("Failed to parse config");
            debug!("Config file content:\n\t{config_content}")
        })?;

        let config_dir = ConfigDir(Some(config_dir.to_path_buf()));
        Self::from_serializable(parsed, &config_dir)
    }

    #[cfg(not(target_arch = "wasm32"))]
    fn process_includes(raw_str: &str, base_dir: &Path) -> Result<toml::Table> {
        let table: toml::Table = toml::from_str(raw_str)?;
        let mut processed_table = toml::Table::new();

        for (key, value) in table {
            let processed_value = match value {
                toml::Value::String(s) if s.starts_with("include('") && s.ends_with("')") => {
                    // Extract the path from include('path')
                    let path_str = s[9..s.len() - 2].to_string();
                    let include_path = base_dir.join(path_str);

                    let include_content = std::fs::read_to_string(&include_path).map_err(|e| {
                        anyhow::anyhow!(
                            "Failed to read include file at path {:?}: {}",
                            include_path,
                            e
                        )
                    })?;
                    toml::from_str(&include_content).map_err(|e| {
                        anyhow::anyhow!(
                            "Failed to parse include file from path {:?}: {}",
                            include_path,
                            e
                        )
                    })?
                }
                _ => value,
            };

            processed_table.insert(key, processed_value);
        }

        Ok(processed_table)
    }

    pub fn from_serializable<T: serde::Serialize>(
        config: T,
        config_dir: &ConfigDir,
    ) -> Result<Self> {
        let registry = RuleRegistry::new();
        let value = toml::Value::try_from(config)?;
        let table = Self::validate_config_structure(value)?;

        let (registry, rule_settings, ignore_globs) =
            Self::process_config_table(registry, table, config_dir)?;

        Ok(Self {
            rule_registry: registry,
            rule_specific_settings: rule_settings,
            ignore_globs,
        })
    }

    fn validate_config_structure(value: toml::Value) -> Result<toml::Table> {
        match value {
            toml::Value::Table(table) => Ok(table),
            _ => Err(anyhow::anyhow!(
                "Invalid configuration. Must be serializable to an object."
            )),
        }
    }

    fn process_config_table(
        mut registry: RuleRegistry,
        table: toml::Table,
        config_dir: &ConfigDir,
    ) -> Result<(
        RuleRegistry,
        HashMap<String, RuleSettings>,
        HashSet<Pattern>,
    )> {
        let mut filtered_rules: HashSet<String> = HashSet::new();
        let mut rule_specific_settings = HashMap::new();
        let mut ignore_globs = HashSet::<Pattern>::new();

        for (key, value) in table {
            match value {
                toml::Value::Array(arr) if key == IGNORE_GLOBS_KEY => {
                    arr.into_iter().for_each(|glob| {
                        if let toml::Value::String(glob) = glob {
                            let root_dir = match config_dir.0 {
                                Some(ref dir) => dir,
                                None => &std::env::current_dir().unwrap(),
                            };
                            let glob = root_dir.join(glob);
                            match Pattern::new(&glob.to_string_lossy()) {
                                Ok(glob) => {
                                    ignore_globs.insert(glob);
                                }
                                Err(err) => {
                                    warn!("Failed to parse ignore pattern {glob:?}: {err:?}");
                                }
                            }
                        }
                    });
                }
                toml::Value::Boolean(false) if RuleRegistry::is_valid_rule(&key) => {
                    filtered_rules.insert(key.clone());
                }
                toml::Value::Table(table) if RuleRegistry::is_valid_rule(&key) => {
                    let level = table.get("level");
                    if let Some(toml::Value::String(level)) = level.as_ref() {
                        match TryInto::<LintLevel>::try_into(level.as_str()) {
                            Ok(level) => {
                                registry.save_configured_level(&key, level);
                            }
                            Err(err) => {
                                warn!("{err}")
                            }
                        }
                    }

                    rule_specific_settings.insert(key.clone(), RuleSettings::new(table.clone()));
                }
                _ => {}
            }
        }

        filtered_rules.iter().for_each(|rule_name| {
            registry.deactivate_rule(rule_name);
        });

        Ok((registry, rule_specific_settings, ignore_globs))
    }

    pub fn is_ignored(&self, path: &Path) -> bool {
        let path = if path.is_relative() {
            let current_dir = env::current_dir().unwrap();
            &current_dir.join(path)
        } else {
            path
        };
        debug!("Checking if path {path:?} is ignored");

        let is_ignored = self
            .ignore_globs
            .iter()
            .any(|pattern| pattern.matches_path(path));
        debug!(
            "Path {path:?} is {}ignored",
            if is_ignored { "" } else { "not " }
        );
        is_ignored
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    use serde_json::json;

    #[cfg(not(target_arch = "wasm32"))]
    use tempfile::NamedTempFile;

    const VALID_RULE_NAME: &str = "Rule001HeadingCase";

    #[cfg(not(target_arch = "wasm32"))]
    fn create_temp_config_file(content: &str) -> NamedTempFile {
        let file = NamedTempFile::new().unwrap();
        std::fs::write(&file, content).unwrap();
        file
    }

    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn test_from_config_file_valid() {
        let content = format!(
            r#"
[{VALID_RULE_NAME}]
option1 = true
option2 = "value"
"#
        );
        let file = create_temp_config_file(&content);
        let config = Config::from_config_file(file.path()).unwrap();
        assert!(config.rule_specific_settings.contains_key(VALID_RULE_NAME));
        assert!(config.rule_registry.is_rule_active(VALID_RULE_NAME));
    }

    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn test_config_with_includes() -> Result<()> {
        let temp_dir = tempfile::tempdir()?;

        let included_content = r#"
option1 = true
option2 = "value"
"#;
        let included_path = temp_dir.path().join("heading_sentence_case.toml");
        fs::write(&included_path, included_content)?;

        let main_content = format!(
            r#"
{VALID_RULE_NAME} = "include('heading_sentence_case.toml')"
"#
        );
        let main_config_path = temp_dir.path().join("config.toml");
        fs::write(&main_config_path, main_content)?;

        let config = Config::from_config_file(main_config_path)?;

        assert!(config.rule_specific_settings.contains_key(VALID_RULE_NAME));
        let rule_settings = config.rule_specific_settings.get(VALID_RULE_NAME).unwrap();
        assert!(rule_settings.has_key("option1"));
        assert!(rule_settings.has_key("option2"));

        Ok(())
    }

    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn test_ignores_invalid_rule_name() {
        let content = r#"
[RuleInvalidlyNamed]
option1 = true
option2 = "value"
"#;
        let file = create_temp_config_file(content);
        let config = Config::from_config_file(file.path()).unwrap();
        assert!(!config
            .rule_specific_settings
            .contains_key("RuleInvalidlyNamed"));
        assert!(config.rule_registry.is_rule_active(VALID_RULE_NAME));
    }

    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn test_from_config_file_invalid() {
        let content = "invalid toml content";
        let file = create_temp_config_file(content);
        assert!(Config::from_config_file(file.path()).is_err());
    }

    #[test]
    fn test_from_serializable_valid() {
        let config_json = json!({
            VALID_RULE_NAME: {
                "option1": true,
                "option2": "value"
            },
        });
        let config = Config::from_serializable(config_json, &ConfigDir(None)).unwrap();
        assert!(config.rule_specific_settings.contains_key(VALID_RULE_NAME));
        assert!(config.rule_registry.is_rule_active(VALID_RULE_NAME));
    }

    #[test]
    fn test_config_deactivate_rule() {
        let config_json = json!({
            VALID_RULE_NAME: false
        });
        let config = Config::from_serializable(config_json, &ConfigDir(None)).unwrap();
        assert!(!config.rule_registry.is_rule_active(VALID_RULE_NAME));
    }

    #[test]
    fn test_from_serializable_invalid() {
        let invalid_config = vec![1, 2, 3]; // Not a table/object
        assert!(Config::from_serializable(invalid_config, &ConfigDir(None)).is_err());
    }
}
