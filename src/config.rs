use anyhow::Result;
use std::{
    collections::{HashMap, HashSet},
    path::Path,
};

use crate::rules::{RuleRegistry, RuleSettings};

pub struct Config {
    rule_registry: RuleRegistry,
    rule_specific_settings: HashMap<String, RuleSettings>,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            rule_registry: RuleRegistry::new(),
            rule_specific_settings: HashMap::new(),
        }
    }
}

impl Config {
    pub fn from_config_file<P: AsRef<Path>>(config_file: P) -> Result<Self> {
        let config_content = std::fs::read_to_string(config_file)?;
        let parsed: toml::Table = toml::from_str(&config_content)?;

        Self::from_serializable(parsed)
    }

    pub fn from_serializable<T: serde::Serialize>(config: T) -> Result<Self> {
        let registry = RuleRegistry::new();
        let value = toml::Value::try_from(config)?;
        let table = Self::validate_config_structure(value)?;

        let (registry, rule_settings) = Self::process_config_table(registry, table)?;

        Ok(Self {
            rule_registry: registry,
            rule_specific_settings: rule_settings,
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
    ) -> Result<(RuleRegistry, HashMap<String, RuleSettings>)> {
        let mut filtered_rules: HashSet<String> = HashSet::new();
        let mut rule_specific_settings = HashMap::new();

        for (key, value) in table {
            match value {
                toml::Value::Boolean(false) if RuleRegistry::is_valid_rule(&key) => {
                    filtered_rules.insert(key.clone());
                }
                toml::Value::Table(table) if RuleRegistry::is_valid_rule(&key) => {
                    rule_specific_settings.insert(key.clone(), RuleSettings::new(table.clone()));
                }
                _ => {}
            }
        }

        filtered_rules.iter().for_each(|rule_name| {
            registry.deactivate_rule(rule_name);
        });

        Ok((registry, rule_specific_settings))
    }

    pub(crate) fn get_rule_settings(&self, rule_name: &str) -> Option<&RuleSettings> {
        self.rule_specific_settings.get(rule_name)
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;
    use tempfile::NamedTempFile;

    use super::*;

    const VALID_RULE_NAME: &str = "Rule001HeadingCase";

    fn create_temp_config_file(content: &str) -> NamedTempFile {
        let file = NamedTempFile::new().unwrap();
        std::fs::write(&file, content).unwrap();
        file
    }

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
        let config = Config::from_serializable(config_json).unwrap();
        assert!(config.rule_specific_settings.contains_key(VALID_RULE_NAME));
        assert!(config.rule_registry.is_rule_active(VALID_RULE_NAME));
    }

    #[test]
    fn test_from_serializable_invalid() {
        let invalid_config = vec![1, 2, 3]; // Not a table/object
        assert!(Config::from_serializable(invalid_config).is_err());
    }
}
