use anyhow::Result;
use markdown::mdast::Node;
use once_cell::sync::Lazy;
use regex::Regex;
use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
};

use crate::{errors::LintError, parser::ParseResult};

pub mod rule001_heading_case;

use rule001_heading_case::Rule001HeadingCase;

#[allow(clippy::type_complexity)]
static ALL_RULES: Lazy<Vec<Arc<Mutex<Box<dyn Rule>>>>> = Lazy::new(|| {
    vec![Arc::new(Mutex::new(
        Box::new(Rule001HeadingCase::default()),
    ))]
});

pub trait Rule: Send + Sync + RuleName {
    fn setup(&mut self, _settings: Option<&RuleSettings>) {}
    fn check(&self, ast: &Node, context: &RuleContext) -> Option<Vec<LintError>>;
}

pub trait RuleName {
    fn name(&self) -> &'static str;
}

#[derive(Clone, Debug)]
pub struct RuleSettings(toml::Value);

pub enum RegexSetting {
    MatchBeginning,
}

impl RuleSettings {
    pub fn new(table: toml::Table) -> Self {
        Self(toml::Value::Table(table))
    }

    pub fn get_array_of_regexes(
        &self,
        key: &str,
        setting: Option<RegexSetting>,
    ) -> Option<Vec<Regex>> {
        let table = &self.0;
        if let Some(toml::Value::Array(array)) = table.get(key) {
            let mut vec = Vec::new();
            for value in array {
                if let toml::Value::String(pattern) = value {
                    let pattern = if let Some(ref setting) = setting {
                        match setting {
                            RegexSetting::MatchBeginning => format!("^{}\\b", pattern),
                        }
                    } else {
                        pattern.to_string()
                    };

                    if let Ok(regex) = Regex::new(&pattern) {
                        vec.push(regex);
                    }
                    // Silently ignore invalid regex patterns
                }
            }
            if vec.is_empty() {
                None
            } else {
                vec.sort_by_key(|b| std::cmp::Reverse(b.as_str().len()));
                Some(vec)
            }
        } else {
            None
        }
    }
}

pub struct RuleContext {
    parse_result: ParseResult,
}

impl RuleContext {
    pub fn new(parse_result: ParseResult) -> Self {
        Self { parse_result }
    }

    pub fn frontmatter_lines(&self) -> usize {
        self.parse_result.frontmatter_lines
    }
}

pub struct RuleRegistry {
    state: RuleRegistryState,
    rules: Vec<Arc<Mutex<Box<dyn Rule>>>>,
}

enum RuleRegistryState {
    PreSetup,
    Setup,
}

impl RuleRegistry {
    pub fn new() -> Self {
        let rules = ALL_RULES.clone();
        Self {
            state: RuleRegistryState::PreSetup,
            rules,
        }
    }

    pub fn is_valid_rule(rule_name: &str) -> bool {
        ALL_RULES
            .iter()
            .any(|rule| rule.lock().unwrap().name() == rule_name)
    }

    pub fn deactivate_rule(&mut self, rule_name: &str) {
        self.rules
            .retain(|rule| rule.lock().unwrap().name() != rule_name);
    }

    #[cfg(test)]
    pub fn is_rule_active(&self, rule_name: &str) -> bool {
        self.rules
            .iter()
            .any(|rule| rule.lock().unwrap().name() == rule_name)
    }

    pub fn setup(&mut self, settings: &HashMap<String, RuleSettings>) -> Result<()> {
        match self.state {
            RuleRegistryState::PreSetup => {
                for rule in &mut self.rules {
                    let mut rule = rule.lock().unwrap();
                    let rule_settings = settings.get(rule.name());
                    rule.setup(rule_settings);
                }
                self.state = RuleRegistryState::Setup;
                Ok(())
            }
            RuleRegistryState::Setup => Err(anyhow::anyhow!(
                "Cannot setup rule registry if it is already set up"
            )),
        }
    }

    pub fn run(&self, context: &RuleContext) -> Result<Vec<LintError>> {
        match self.state {
            RuleRegistryState::PreSetup => Err(anyhow::anyhow!(
                "Cannot run rule registry in pre-setup state"
            )),
            RuleRegistryState::Setup => {
                let mut errors = Vec::new();
                self.check_node(&context.parse_result.ast, context, &mut errors);
                Ok(errors)
            }
        }
    }

    fn check_node(&self, ast: &Node, context: &RuleContext, errors: &mut Vec<LintError>) {
        for rule in &self.rules {
            if let Some(rule_errors) = rule.lock().unwrap().check(ast, context) {
                errors.extend(rule_errors);
            }
        }

        if let Some(children) = ast.children() {
            for child in children {
                self.check_node(child, context, errors);
            }
        }
    }
}
