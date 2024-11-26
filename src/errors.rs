use std::fmt::Display;

use anyhow::Result;
use markdown::mdast::Node;
use serde::{Deserialize, Serialize};

use crate::{
    fix::LintFix,
    geometry::{AdjustedPoint, AdjustedRange, DenormalizedLocation},
    rules::RuleContext,
};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "UPPERCASE")]
pub enum LintLevel {
    Error,
    Warning,
}

impl Display for LintLevel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            LintLevel::Error => write!(f, "ERROR"),
            LintLevel::Warning => write!(f, "WARN"),
        }
    }
}

impl TryFrom<&str> for LintLevel {
    type Error = anyhow::Error;

    fn try_from(value: &str) -> Result<Self> {
        let value = value.trim().to_lowercase();
        match value.as_str() {
            "error" => Ok(Self::Error),
            "warn" => Ok(Self::Warning),
            _ => Err(anyhow::anyhow!("Invalid lint level: {value}")),
        }
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct LintError {
    pub rule: String,
    pub level: LintLevel,
    pub message: String,
    pub location: DenormalizedLocation,
    pub fix: Option<Vec<LintFix>>,
}

impl LintError {
    pub fn new(
        rule: impl AsRef<str>,
        message: String,
        level: LintLevel,
        location: AdjustedRange,
        fix: Option<Vec<LintFix>>,
        context: &RuleContext,
    ) -> Self {
        let start = AdjustedPoint::from_adjusted_offset(&location.start, context.rope());
        let end = AdjustedPoint::from_adjusted_offset(&location.end, context.rope());
        let location = DenormalizedLocation {
            offset_range: location,
            start,
            end,
        };

        Self {
            rule: rule.as_ref().into(),
            level,
            message,
            location,
            fix,
        }
    }

    pub fn from_node(
        node: &Node,
        context: &RuleContext,
        rule: impl AsRef<str>,
        message: &str,
        level: LintLevel,
    ) -> Option<Self> {
        if let Some(position) = node.position() {
            let location = AdjustedRange::from_unadjusted_position(position, context);
            Some(Self::new(
                rule,
                message.into(),
                level,
                location,
                None,
                context,
            ))
        } else {
            None
        }
    }

    pub fn from_node_with_fix(
        node: &Node,
        context: &RuleContext,
        rule: impl AsRef<str>,
        message: &str,
        level: LintLevel,
        fix: Vec<LintFix>,
    ) -> Option<Self> {
        let mut lint_error = Self::from_node(node, context, rule, message, level)?;
        lint_error.fix = Some(fix);
        Some(lint_error)
    }
}
