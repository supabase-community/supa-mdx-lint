use std::array;

use anyhow::Result;
use bon::bon;
use log::debug;

use crate::{
    comments::{ConfigurationCommentCollection, LintDisables, LintTimeRuleConfigs},
    geometry::AdjustedOffset,
    parser::ParseResult,
    rope::Rope,
    rules::RuleFilter,
};

#[derive(Clone, Hash, PartialEq, Eq)]
pub(crate) struct ContextId([char; 12]);

impl ContextId {
    pub fn new() -> Self {
        Self(array::from_fn(|_| fastrand::alphanumeric()))
    }
}

impl std::fmt::Debug for ContextId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "ContextId({})", String::from_iter(self.0))
    }
}

pub(crate) struct Context<'ctx> {
    /// Key for caching purposes, so individual rules can cache file-level
    /// calculations.
    pub(crate) key: ContextId,
    pub(crate) parse_result: &'ctx ParseResult,
    pub(crate) check_only_rules: RuleFilter<'ctx>,
    pub(crate) disables: LintDisables<'ctx>,
    pub(crate) lint_time_rule_configs: LintTimeRuleConfigs<'ctx>,
}

#[bon]
impl<'ctx> Context<'ctx> {
    #[builder]
    pub(crate) fn new(
        parse_result: &'ctx ParseResult,
        check_only_rules: Option<&'ctx [&'ctx str]>,
    ) -> Result<Self> {
        let (lint_time_rule_configs, disables) =
            ConfigurationCommentCollection::from_parse_result(parse_result)
                .into_parts()
                .unwrap();
        debug!("Lint time rule configs: {:?}", lint_time_rule_configs);
        debug!("Disables: {:?}", disables);

        Ok(Self {
            key: ContextId::new(),
            parse_result,
            check_only_rules,
            disables,
            lint_time_rule_configs,
        })
    }

    pub(crate) fn rope(&self) -> &Rope {
        self.parse_result.rope()
    }

    pub fn content_start_offset(&self) -> AdjustedOffset {
        self.parse_result.content_start_offset()
    }
}
