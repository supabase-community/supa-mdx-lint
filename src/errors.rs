use std::{fmt::Display, ops::Range};

use anyhow::Result;
use bon::bon;
use markdown::mdast::Node;
use serde::{Deserialize, Serialize};

use crate::{
    context::Context,
    fix::LintCorrection,
    geometry::{AdjustedPoint, AdjustedRange, DenormalizedLocation},
    utils::Offsets,
};

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "UPPERCASE")]
pub enum LintLevel {
    Warning,
    #[default]
    Error,
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
    pub(crate) rule: String,
    pub(crate) level: LintLevel,
    pub(crate) message: String,
    pub(crate) location: DenormalizedLocation,
    pub(crate) fix: Option<Vec<LintCorrection>>,
    pub(crate) suggestions: Option<Vec<LintCorrection>>,
}

impl Offsets for LintError {
    fn start(&self) -> usize {
        self.location.offset_range.start.into()
    }

    fn end(&self) -> usize {
        self.location.offset_range.end.into()
    }
}

#[bon]
impl LintError {
    #[builder]
    #[allow(clippy::needless_lifetimes)]
    pub(crate) fn new<'ctx>(
        rule: impl AsRef<str>,
        message: impl Into<String>,
        level: LintLevel,
        location: AdjustedRange,
        fix: Option<Vec<LintCorrection>>,
        suggestions: Option<Vec<LintCorrection>>,
        context: &Context<'ctx>,
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

    pub fn level(&self) -> LintLevel {
        self.level
    }

    pub fn message(&self) -> &str {
        &self.message
    }

    pub fn offset_range(&self) -> Range<usize> {
        self.location.offset_range.to_usize_range()
    }

    pub fn combined_suggestions(&self) -> Option<Vec<&LintCorrection>> {
        match (self.fix.as_ref(), self.suggestions.as_ref()) {
            (None, None) => None,
            (fix, suggestions) => {
                let mut combined = Vec::new();
                if let Some(f) = fix {
                    combined.extend(f.iter());
                }
                if let Some(s) = suggestions {
                    combined.extend(s.iter());
                }
                Some(combined)
            }
        }
    }

    #[builder]
    #[allow(clippy::needless_lifetimes)]
    pub(crate) fn from_node<'ctx>(
        /// The AST node to generate the error location from.
        node: &Node,
        context: &Context<'ctx>,
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
    pub(crate) fn from_raw_location(
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
