use anyhow::Result;
use bon::bon;
use log::{debug, error, warn};
use markdown::mdast::Node;
use regex::Regex;
use serde::Deserialize;
use std::{collections::HashMap, fmt::Debug};

#[cfg(test)]
use serde::Serialize;

use crate::{
    errors::{LintError, LintLevel},
    geometry::AdjustedOffset,
    parser::{LintDisables, ParseResult},
    rope::Rope,
};

mod rule001_heading_case;
mod rule002_admonition_types;
mod rule003_spelling;
mod rule004_exclude_words;

pub use rule001_heading_case::Rule001HeadingCase;
pub use rule002_admonition_types::Rule002AdmonitionTypes;
pub use rule003_spelling::Rule003Spelling;
pub use rule004_exclude_words::Rule004ExcludeWords;

fn get_all_rules() -> Vec<Box<dyn Rule>> {
    vec![
        Box::new(Rule001HeadingCase::default()),
        Box::new(Rule002AdmonitionTypes::default()),
        Box::new(Rule003Spelling::default()),
        Box::new(Rule004ExcludeWords::default()),
    ]
}

pub(crate) trait Rule: Debug + Send + RuleName {
    fn default_level(&self) -> LintLevel;
    fn setup(&mut self, _settings: Option<&mut RuleSettings>) {}
    fn check(&self, ast: &Node, context: &RuleContext, level: LintLevel) -> Option<Vec<LintError>>;
}

pub(crate) trait RuleName {
    fn name(&self) -> &'static str;
}

impl dyn Rule {
    pub fn get_level(&self, configured_level: Option<LintLevel>) -> LintLevel {
        configured_level.unwrap_or(self.default_level())
    }
}

#[derive(Clone, Debug)]
pub(crate) struct RuleSettings(toml::Value);

#[derive(Default)]
pub(crate) struct RegexSettings {
    pub(crate) beginning: Option<RegexBeginning>,
    /// Regex should only match if it matches up to the end of the word.
    pub(crate) ending: Option<RegexEnding>,
}

pub(crate) enum RegexBeginning {
    VeryBeginning,
    WordBoundary,
}

pub(crate) enum RegexEnding {
    WordBoundary,
}

impl RuleSettings {
    pub fn new(table: impl Into<toml::Table>) -> Self {
        Self(toml::Value::Table(table.into()))
    }

    #[cfg(test)]
    pub fn has_key(&self, key: &str) -> bool {
        self.0
            .as_table()
            .map(|table| table.contains_key(key))
            .unwrap_or(false)
    }

    #[cfg(test)]
    fn from_key_value(key: &str, value: toml::Value) -> Self {
        let mut table = toml::Table::new();
        table.insert(key.to_string(), value);
        Self::new(table)
    }

    #[cfg(test)]
    fn with_array_of_strings(key: &str, values: Vec<&str>) -> Self {
        let array = values
            .into_iter()
            .map(|s| toml::Value::String(s.to_string()))
            .collect();
        Self::from_key_value(key, toml::Value::Array(array))
    }

    fn get_array_of_strings(&self, key: &str) -> Option<Vec<String>> {
        let table = &self.0;
        if let Some(toml::Value::Array(array)) = table.get(key) {
            let mut vec = Vec::new();
            for value in array {
                if let toml::Value::String(string) = value {
                    vec.push(string.to_lowercase());
                }
            }

            if vec.is_empty() {
                return None;
            } else {
                return Some(vec);
            }
        }

        None
    }

    fn get_array_of_regexes(
        &self,
        key: &str,
        settings: Option<&RegexSettings>,
    ) -> Option<Vec<Regex>> {
        let table = &self.0;
        if let Some(toml::Value::Array(array)) = table.get(key) {
            let mut vec = Vec::new();
            for value in array {
                if let toml::Value::String(pattern) = value {
                    let mut pattern = pattern.to_string();
                    if let Some(settings) = settings {
                        match settings.beginning {
                            Some(RegexBeginning::VeryBeginning) => {
                                if !pattern.starts_with('^') {
                                    pattern = format!("^{}", pattern);
                                }
                            }
                            Some(RegexBeginning::WordBoundary) => {
                                if !pattern.starts_with("\\b")
                                    && !pattern.starts_with("\\s")
                                    && !pattern.starts_with("^")
                                {
                                    pattern = format!("(?:^|\\s|\\b){}", pattern);
                                }
                            }
                            None => {}
                        }
                        #[allow(clippy::single_match)]
                        match settings.ending {
                            Some(RegexEnding::WordBoundary) => {
                                if !pattern.ends_with("\\b")
                                    && !pattern.ends_with("\\s")
                                    && !pattern.ends_with("$")
                                {
                                    pattern = format!(r#"{}(?:\s|\b|$|[.,!?'"-])"#, pattern);
                                }
                            }
                            None => {}
                        }
                    }

                    if let Ok(regex) = Regex::new(&pattern) {
                        vec.push(regex);
                    } else {
                        warn!("Encountered invalid regex pattern in rule settings: {pattern}")
                    }
                }
            }
            if vec.is_empty() {
                None
            } else {
                // Sort regexes by length, so the longest match is tried first.
                //
                // This ensures, for example, that if two exceptions "Supabase"
                // and "Supabase Auth" are defined, the "Supabase Auth"
                // exception will trigger first, preventing "Auth" from being
                // matched as a false positive.
                //
                // Note that this is not a perfect solution, as the order of
                // matched pattern lengths is not guaranteed to be the same as
                // the order of regex pattern lengths. For example, the regex
                // "a{35}" is shorter than "abcdefg", but will match a longer
                // result. However, since we're unlikely to see regexes defined
                // this way in exception files, we're just going to ignore this
                // issue for now.
                vec.sort_by_key(|b| std::cmp::Reverse(b.as_str().len()));
                Some(vec)
            }
        } else {
            None
        }
    }

    #[cfg(test)]
    fn with_serializable<T: Serialize>(key: &str, value: &T) -> Self {
        Self::from_key_value(key, toml::Value::try_from(value).unwrap())
    }

    // TODO: global config should not keep carrying around the rule-level configs after the rules are set up, because the rules could mutate it
    fn get_deserializable<T: for<'de> Deserialize<'de>>(&mut self, key: &str) -> Option<T> {
        if let toml::Value::Table(ref mut table) = self.0 {
            if let Some(value) = table.remove(key) {
                if let Ok(item) = value.try_into() {
                    return Some(item);
                }
            }
        }
        None
    }
}

pub(crate) type RuleFilter<'filter> = Option<&'filter [&'filter str]>;

pub(crate) struct RuleContext<'ctx> {
    parse_result: ParseResult,
    check_only_rules: RuleFilter<'ctx>,
    disables: LintDisables,
}

#[bon]
impl<'ctx> RuleContext<'ctx> {
    #[builder]
    pub(crate) fn new(
        parse_result: ParseResult,
        check_only_rules: Option<&'ctx [&'ctx str]>,
    ) -> Result<Self> {
        let mut ctx = Self {
            parse_result,
            check_only_rules,
            disables: Default::default(),
        };

        let disables = LintDisables::new(ctx.ast(), &ctx).inspect_err(|err| {
            error!("Error parsing disable directives from file: {}", err);
        })?;
        debug!("Disables: {:?}", disables);
        ctx.disables = disables;

        Ok(ctx)
    }

    #[cfg(test)]
    pub(crate) fn new_parse_only_for_testing(parse_result: ParseResult) -> Self {
        Self {
            parse_result,
            check_only_rules: Default::default(),
            disables: Default::default(),
        }
    }

    pub(crate) fn ast(&self) -> &Node {
        &self.parse_result.ast
    }

    pub(crate) fn rope(&self) -> &Rope {
        &self.parse_result.rope
    }

    pub fn content_start_offset(&self) -> &AdjustedOffset {
        &self.parse_result.content_start_offset
    }
}

#[derive(Debug)]
pub(crate) struct RuleRegistry {
    state: RuleRegistryState,
    rules: Vec<Box<dyn Rule>>,
    configured_levels: HashMap<String, LintLevel>,
}

#[derive(Debug)]
enum RuleRegistryState {
    PreSetup,
    Ready,
}

impl RuleRegistry {
    pub fn new() -> Self {
        Self {
            state: RuleRegistryState::PreSetup,
            rules: get_all_rules(),
            configured_levels: Default::default(),
        }
    }

    pub fn is_valid_rule(&self, rule_name: &str) -> bool {
        self.rules.iter().any(|rule| rule.name() == rule_name)
    }

    pub fn deactivate_rule(&mut self, rule_name: &str) {
        self.rules.retain(|rule| rule.name() != rule_name);
    }

    pub fn save_configured_level(&mut self, rule_name: &str, level: LintLevel) {
        self.configured_levels.insert(rule_name.to_string(), level);
    }

    pub fn get_configured_level(&self, rule_name: &str) -> Option<LintLevel> {
        self.configured_levels.get(rule_name).cloned()
    }

    #[cfg(test)]
    pub fn is_rule_active(&self, rule_name: &str) -> bool {
        self.rules.iter().any(|rule| rule.name() == rule_name)
    }

    #[cfg(test)]
    pub fn deactivate_all_but(&mut self, rule_name: &str) {
        self.rules.retain(|rule| rule.name() == rule_name)
    }

    pub fn setup(&mut self, settings: &mut HashMap<String, RuleSettings>) -> Result<()> {
        match self.state {
            RuleRegistryState::PreSetup => {
                for rule in &mut self.rules {
                    let rule_settings = settings.get_mut(rule.name());
                    rule.setup(rule_settings);
                }
                self.state = RuleRegistryState::Ready;
                Ok(())
            }
            RuleRegistryState::Ready => Err(anyhow::anyhow!(
                "Cannot set up rule registry if it is already set up"
            )),
        }
    }

    pub fn run(&self, context: &RuleContext) -> Result<Vec<LintError>> {
        match self.state {
            RuleRegistryState::PreSetup => Err(anyhow::anyhow!(
                "Cannot run rule registry in pre-setup state"
            )),
            RuleRegistryState::Ready => {
                let mut errors = Vec::new();
                self.check_node(&context.parse_result.ast, context, &mut errors);
                Ok(errors)
            }
        }
    }

    fn check_node(&self, ast: &Node, context: &RuleContext, errors: &mut Vec<LintError>) {
        for rule in &self.rules {
            if let Some(filter) = &context.check_only_rules {
                if !filter.contains(&rule.name()) {
                    continue;
                }
            }

            let rule_level = rule.get_level(self.get_configured_level(rule.name()));
            if let Some(rule_errors) = rule.check(ast, context, rule_level) {
                debug!("Rule errors: {:#?}", rule_errors);
                let filtered_errors: Vec<LintError> = rule_errors
                    .into_iter()
                    .filter(|err| {
                        !context.disables.is_rule_disabled_for_location(
                            rule.name(),
                            &err.location,
                            context,
                        )
                    })
                    .collect();
                errors.extend(filtered_errors);
            }
        }

        if let Some(children) = ast.children() {
            for child in children {
                self.check_node(child, context, errors);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    };

    use crate::parser::parse;

    use super::*;
    use markdown::mdast::Node;
    use supa_mdx_macros::RuleName;

    #[derive(Clone, Default, Debug, RuleName)]
    struct MockRule {
        check_count: Arc<AtomicUsize>,
    }

    impl Rule for MockRule {
        fn default_level(&self) -> LintLevel {
            LintLevel::Error
        }

        fn check(
            &self,
            _ast: &Node,
            _context: &RuleContext,
            _level: LintLevel,
        ) -> Option<Vec<LintError>> {
            self.check_count.fetch_add(1, Ordering::Relaxed);
            None
        }
    }

    #[derive(Clone, Default, Debug, RuleName)]
    struct MockRule2 {
        check_count: Arc<AtomicUsize>,
    }

    impl Rule for MockRule2 {
        fn default_level(&self) -> LintLevel {
            LintLevel::Error
        }

        fn check(
            &self,
            _ast: &Node,
            _context: &RuleContext,
            _level: LintLevel,
        ) -> Option<Vec<LintError>> {
            self.check_count.fetch_add(1, Ordering::Relaxed);
            None
        }
    }

    #[test]
    fn test_check_node_with_filter() {
        let mock_rule_1 = MockRule::default();
        let mock_rule_2 = MockRule2::default();
        let check_count_1 = mock_rule_1.check_count.clone();
        let check_count_2 = mock_rule_2.check_count.clone();

        let registry = RuleRegistry {
            state: RuleRegistryState::Ready,
            rules: vec![Box::new(mock_rule_1), Box::new(mock_rule_2)],
            configured_levels: Default::default(),
        };

        let mdx = "text";
        let parse_result = parse(mdx).unwrap();
        let context = RuleContext::builder()
            .parse_result(parse_result)
            .check_only_rules(&["MockRule"])
            .build()
            .unwrap();

        let mut errors = Vec::new();
        registry.check_node(context.ast(), &context, &mut errors);

        assert!(check_count_1.load(Ordering::Relaxed) > 1);
        assert_eq!(check_count_2.load(Ordering::Relaxed), 0);
    }

    #[test]
    fn test_check_node_without_filter() {
        let mock_rule_1 = MockRule::default();
        let mock_rule_2 = MockRule2::default();
        let check_count_1 = mock_rule_1.check_count.clone();
        let check_count_2 = mock_rule_2.check_count.clone();

        let registry = RuleRegistry {
            state: RuleRegistryState::Ready,
            rules: vec![Box::new(mock_rule_1), Box::new(mock_rule_2)],
            configured_levels: Default::default(),
        };

        let mdx = "test";
        let parse_result = parse(mdx).unwrap();
        let context = RuleContext::builder()
            .parse_result(parse_result)
            .build()
            .unwrap();

        let mut errors = Vec::new();
        registry.check_node(context.ast(), &context, &mut errors);

        assert!(check_count_1.load(Ordering::Relaxed) > 1);
        assert!(check_count_2.load(Ordering::Relaxed) > 1);
    }
}
