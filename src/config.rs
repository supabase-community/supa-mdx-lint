use anyhow::Result;
use bon::bon;
use glob::{MatchOptions, Pattern};
use log::{debug, error, warn};
use std::{
    collections::{hash_map, HashMap, HashSet},
    env,
    path::{Path, PathBuf},
};

use crate::{
    errors::LintLevel,
    rules::{RuleRegistry, RuleSettings},
    utils::{
        path::{normalize_path, IsGlob},
        path_relative_from,
    },
    PhaseReady, PhaseSetup,
};

const IGNORE_GLOBS_KEY: &str = "ignore_patterns";

#[derive(Debug, Clone)]
pub struct ConfigDir(pub Option<PathBuf>);

impl ConfigDir {
    pub fn none() -> Self {
        Self(None)
    }

    pub fn new(path: PathBuf) -> Self {
        Self(Some(path))
    }
}

#[derive(Debug, Default)]
pub struct ConfigFileLocations(Option<HashMap<String, String>>);

impl ConfigFileLocations {
    fn insert(&mut self, key: &str, value: &Path) {
        let map = self.0.get_or_insert_with(HashMap::new);
        if !map.contains_key(key) {
            map.insert(
                key.to_string(),
                std::fs::canonicalize(value)
                    .map(|path| path.to_string_lossy().into_owned())
                    .unwrap_or(value.to_string_lossy().into_owned()),
            );
        }
    }

    fn iter(&self) -> ConfigFileLocationsIterator {
        ConfigFileLocationsIterator {
            inner: self.0.as_ref().map(|map| map.iter()),
        }
    }
}

struct ConfigFileLocationsIterator<'a> {
    inner: Option<hash_map::Iter<'a, String, String>>,
}

impl<'a> Iterator for ConfigFileLocationsIterator<'a> {
    type Item = (&'a String, &'a String);

    fn next(&mut self) -> Option<Self::Item> {
        self.inner.as_mut().and_then(|iter| iter.next())
    }
}

#[derive(Debug)]
pub struct Config<Phase> {
    pub(crate) rule_registry: RuleRegistry<Phase>,
    pub(crate) rule_specific_settings: HashMap<String, RuleSettings>,
    /// A list of globs to ignore.
    ignore_globs: HashSet<Pattern>,
    config_file_locations: ConfigFileLocations,
}

impl Default for Config<PhaseSetup> {
    fn default() -> Self {
        Self {
            rule_registry: RuleRegistry::<PhaseSetup>::new(),
            rule_specific_settings: HashMap::new(),
            ignore_globs: HashSet::new(),
            config_file_locations: ConfigFileLocations(None),
        }
    }
}

#[bon]
impl Config<PhaseSetup> {
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
    pub fn from_config_file<P: AsRef<Path>>(config_file: P) -> Result<Self> {
        let config_file = config_file.as_ref();
        let config_path = config_file.to_path_buf();
        let config_dir = config_path.parent().ok_or_else(|| {
            anyhow::anyhow!("Unable to determine parent directory of config file: {config_path:?}")
        })?;

        let config_content = std::fs::read_to_string(&config_path)
            .inspect_err(|_| error!("Failed to read config file at {config_path:?}"))?;
        let table: toml::Table = toml::from_str(&config_content)?;

        let mut file_locations = ConfigFileLocations::default();

        let parsed = Self::process_includes()
            .table(&table)
            .file_locations(&mut file_locations)
            .base_dir(config_dir)
            .current_file(config_file)
            .is_top_level(true)
            .call()
            .inspect_err(|_| {
                error!("Failed to parse config");
                debug!("Config file content:\n\t{config_content}")
            })?;

        let config_dir = ConfigDir(Some(config_dir.to_path_buf()));
        Self::from_serializable()
            .config(parsed)
            .config_dir(&config_dir)
            .config_file_locations(file_locations)
            .call()
    }

    #[builder]
    fn process_includes(
        table: &toml::Table,
        file_locations: &mut ConfigFileLocations,
        base_dir: &Path,
        current_file: &Path,
        #[builder(default)] is_top_level: bool,
    ) -> Result<toml::Table> {
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

                    file_locations.insert(key, include_path.as_path());

                    let table: toml::Table = toml::from_str(&include_content)?;
                    toml::Value::Table(
                        Self::process_includes()
                            .table(&table)
                            .file_locations(file_locations)
                            .base_dir(base_dir)
                            .current_file(include_path.as_path())
                            .call()
                            .map_err(|e| {
                                anyhow::anyhow!(
                                    "Failed to parse include file from path {:?}: {}",
                                    include_path,
                                    e
                                )
                            })?,
                    )
                }
                toml::Value::Table(table) => {
                    if is_top_level {
                        file_locations.insert(key, current_file);
                    }
                    toml::Value::Table(
                        Self::process_includes()
                            .table(table)
                            .file_locations(file_locations)
                            .base_dir(base_dir)
                            .current_file(current_file)
                            .call()?,
                    )
                }
                _ => {
                    if is_top_level {
                        file_locations.insert(key, current_file);
                    }
                    value.clone()
                }
            };

            processed_table.insert(key.clone(), processed_value);
        }

        Ok(processed_table)
    }

    #[builder]
    pub fn from_serializable<T: serde::Serialize>(
        config: T,
        config_dir: &ConfigDir,
        #[builder(default = ConfigFileLocations::default())]
        config_file_locations: ConfigFileLocations,
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
            config_file_locations,
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

    #[allow(clippy::type_complexity)]
    fn process_config_table(
        mut registry: RuleRegistry<PhaseSetup>,
        table: toml::Table,
        config_dir: &ConfigDir,
    ) -> Result<(
        RuleRegistry<PhaseSetup>,
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
                            let glob_str = normalize_path(&glob, IsGlob(true));
                            match Pattern::new(&glob_str) {
                                Ok(glob) => {
                                    ignore_globs.insert(glob);
                                }
                                Err(err) => {
                                    warn!("Failed to parse ignore pattern {glob_str}: {err:?}");
                                }
                            }
                        }
                    });
                }
                toml::Value::Boolean(false) if registry.is_valid_rule(&key) => {
                    filtered_rules.insert(key.clone());
                }
                toml::Value::Table(table) if registry.is_valid_rule(&key) => {
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
}

impl TryFrom<Config<PhaseSetup>> for Config<PhaseReady> {
    type Error = anyhow::Error;

    fn try_from(mut old_config: Config<PhaseSetup>) -> Result<Self> {
        let ready_registry = old_config
            .rule_registry
            .setup(&mut old_config.rule_specific_settings)?;
        Ok(Self {
            rule_registry: ready_registry,
            rule_specific_settings: old_config.rule_specific_settings,
            ignore_globs: old_config.ignore_globs,
            config_file_locations: old_config.config_file_locations,
        })
    }
}

impl<RuleRegistryState> Config<RuleRegistryState> {
    pub(crate) fn is_lintable(&self, path: impl AsRef<Path>) -> bool {
        let path = path.as_ref();
        path.is_dir() || path.extension().map_or(false, |ext| ext == "mdx")
    }

    pub(crate) fn is_ignored(&self, path: impl AsRef<Path>) -> bool {
        let path = path.as_ref();
        let path = if path.is_relative() {
            let current_dir = env::current_dir().unwrap();
            &current_dir.join(path)
        } else {
            path
        };
        let path_str = normalize_path(path, IsGlob(false));
        debug!("Checking if {path_str} is ignored");

        let is_ignored = self.ignore_globs.iter().any(|pattern| {
            pattern.matches_with(
                &path_str,
                MatchOptions {
                    case_sensitive: true,
                    require_literal_separator: true,
                    require_literal_leading_dot: false,
                },
            )
        });
        debug!(
            "Path {path_str} is {}ignored",
            if is_ignored { "" } else { "not " }
        );
        is_ignored
    }
}

#[derive(Debug, Default)]
pub struct ConfigMetadata {
    pub config_file_locations: Option<HashMap<String, String>>,
}

impl From<&Config<PhaseReady>> for ConfigMetadata {
    fn from(config: &Config<PhaseReady>) -> Self {
        let current_directory = std::env::current_dir().unwrap();

        let locations = &config.config_file_locations;
        let mut map: Option<HashMap<String, String>> = None;

        locations.iter().for_each(|(key, value)| {
            let normalized_path = PathBuf::from(value);
            let normalized_path =
                path_relative_from(normalized_path.as_path(), current_directory.as_path())
                    .unwrap_or(normalized_path);
            map.get_or_insert_with(HashMap::new)
                .insert(key.clone(), normalized_path.to_string_lossy().to_string());
        });

        Self {
            config_file_locations: map,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    use serde_json::json;

    use tempfile::NamedTempFile;

    const VALID_RULE_NAME: &str = "Rule001HeadingCase";
    const VALID_RULE_NAME_2: &str = "Rule003Spelling";

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
        let config = Config::from_serializable()
            .config(config_json)
            .config_dir(&ConfigDir(None))
            .call()
            .unwrap();
        assert!(config.rule_specific_settings.contains_key(VALID_RULE_NAME));
        assert!(config.rule_registry.is_rule_active(VALID_RULE_NAME));
    }

    #[test]
    fn test_config_deactivate_rule() {
        let config_json = json!({
            VALID_RULE_NAME: false
        });
        let config = Config::from_serializable()
            .config(config_json)
            .config_dir(&ConfigDir(None))
            .call()
            .unwrap();
        assert!(!config.rule_registry.is_rule_active(VALID_RULE_NAME));
    }

    #[test]
    fn test_from_serializable_invalid() {
        let invalid_config = vec![1, 2, 3]; // Not a table/object
        assert!(Config::from_serializable()
            .config(invalid_config)
            .config_dir(&ConfigDir(None))
            .call()
            .is_err());
    }

    #[test]
    fn test_config_tracks_file_locations_single_file() {
        let content = format!(
            r#"
    [{VALID_RULE_NAME}]
    option1 = true
    option2 = "value"
    "#
        );
        let file = create_temp_config_file(&content);
        let config = Config::from_config_file(file.path()).unwrap();

        let metadata = ConfigMetadata::from(&Config::try_from(config).unwrap());
        let locations = metadata.config_file_locations.unwrap();

        assert!(locations.len() == 1);
        assert!(locations.get(VALID_RULE_NAME).is_some());
    }

    #[test]
    fn test_config_tracks_file_locations_with_includes() {
        let temp_dir = tempfile::tempdir().unwrap();

        // Create include file
        let included_content = r#"
    option1 = true
    option2 = "value"
    "#;
        let included_path = temp_dir.path().join("rule_settings.toml");
        fs::write(&included_path, included_content).unwrap();

        // Create main config that includes the above file
        let main_content = format!(
            r#"
    {VALID_RULE_NAME} = "include('rule_settings.toml')"

    [{VALID_RULE_NAME_2}]
    option3 = false
    "#
        );
        let main_config_path = temp_dir.path().join("config.toml");
        fs::write(&main_config_path, &main_content).unwrap();

        let config = Config::from_config_file(&main_config_path).unwrap();
        let metadata = ConfigMetadata::from(&Config::try_from(config).unwrap());
        let locations = metadata.config_file_locations.unwrap();

        assert!(locations.len() == 2);
        assert!(locations
            .get(VALID_RULE_NAME)
            .unwrap()
            .contains("rule_settings.toml"));
        assert!(locations
            .get(VALID_RULE_NAME_2)
            .unwrap()
            .contains("config.toml"));
    }

    #[test]
    // Known bug where the relative path calculation doesn't work on Windows
    #[cfg(not(target_os = "windows"))]
    fn test_config_locations_normalized() {
        let temp_dir = tempfile::tempdir().unwrap();
        let original_dir = env::current_dir().unwrap();

        // Create a nested directory structure
        let project_dir = temp_dir.path().join("project");
        let config_dir = project_dir.join("configs");
        let rules_dir = project_dir.join("rules");
        fs::create_dir_all(&config_dir).unwrap();
        fs::create_dir_all(&rules_dir).unwrap();

        // Create rule config in rules directory
        let rule_content = r#"
    option1 = true
    option2 = "value"
    "#;
        let rule_path = rules_dir.join("rule_config.toml");
        fs::write(&rule_path, rule_content).unwrap();

        // Create main config that includes the rule file
        let main_content = format!(
            r#"
    {VALID_RULE_NAME} = "include('../rules/rule_config.toml')"

    [{VALID_RULE_NAME_2}]
    option3 = false
    "#
        );
        let main_config_path = config_dir.join("main.toml");
        fs::write(&main_config_path, &main_content).unwrap();

        // Change current directory to the project root
        env::set_current_dir(&project_dir).unwrap();

        // Parse config
        let config = Config::from_config_file(&main_config_path).unwrap();
        let metadata = ConfigMetadata::from(&Config::try_from(config).unwrap());
        let locations = metadata.config_file_locations.unwrap();

        assert!(locations.len() == 2);
        assert!(locations.get(VALID_RULE_NAME).unwrap() == "rules/rule_config.toml");
        assert!(locations.get(VALID_RULE_NAME_2).unwrap() == "configs/main.toml");

        // Restore original directory
        env::set_current_dir(original_dir).unwrap();
    }
}
