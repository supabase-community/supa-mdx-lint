use std::fmt::Display;

use anyhow::Result;
use bon::bon;
use markdown::mdast::Node;
use serde::{Deserialize, Serialize};

use crate::{
    fix::LintCorrection,
    geometry::{AdjustedPoint, AdjustedRange, DenormalizedLocation},
    rules::RuleContext,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
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
    pub fix: Option<Vec<LintCorrection>>,
    pub suggestions: Option<Vec<LintCorrection>>,
}

#[bon]
impl LintError {
    #[builder]
    pub fn new<'ctx>(
        rule: impl AsRef<str>,
        message: impl Into<String>,
        level: LintLevel,
        location: AdjustedRange,
        fix: Option<Vec<LintCorrection>>,
        suggestions: Option<Vec<LintCorrection>>,
        context: &RuleContext<'ctx>,
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
            message: message.into(),
            location,
            fix,
            suggestions,
        }
    }

    #[builder]
    pub fn from_node<'ctx>(
        /// The AST node to generate the error location from.
        node: &Node,
        context: &RuleContext<'ctx>,
        /// The rule name.
        rule: impl AsRef<str>,
        message: &str,
        level: LintLevel,
        fix: Option<Vec<LintCorrection>>,
        suggestions: Option<Vec<LintCorrection>>,
    ) -> Option<Self> {
        if let Some(position) = node.position() {
            let location = AdjustedRange::from_unadjusted_position(position, context);
            Some(
                Self::builder()
                    .location(location)
                    .context(context)
                    .rule(rule)
                    .message(message)
                    .level(level)
                    .maybe_fix(fix)
                    .maybe_suggestions(suggestions)
                    .build(),
            )
        } else {
            None
        }
    }

    #[builder]
    pub fn from_raw_location<'ctx>(
        rule: impl AsRef<str>,
        message: impl Into<String>,
        level: LintLevel,
        location: DenormalizedLocation,
        fix: Option<Vec<LintCorrection>>,
        suggestions: Option<Vec<LintCorrection>>,
    ) -> Self {
        Self {
            rule: rule.as_ref().into(),
            level,
            message: message.into(),
            location,
            fix,
            suggestions,
        }
    }
}
