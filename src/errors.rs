use markdown::mdast::Node;
use serde::{Deserialize, Serialize};

#[cfg(target_arch = "wasm32")]
use tsify::Tsify;

use crate::{
    document::{AdjustedPoint, Location},
    rules::RuleContext,
};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "UPPERCASE")]
pub enum LintLevel {
    Error,
    Warning,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct LintError {
    pub message: String,
    pub location: Location,
    pub fix: Option<Vec<LintFix>>,
}

#[cfg(target_arch = "wasm32")]
#[derive(Debug, Deserialize, Serialize, Tsify)]
#[tsify(into_wasm_abi)]
pub struct JsLintError {
    pub message: String,
    pub location: Location,
    pub fix: Option<Vec<JsLintFix>>,
}

#[cfg(target_arch = "wasm32")]
impl From<LintError> for JsLintError {
    fn from(value: LintError) -> Self {
        Self {
            message: value.message,
            location: value.location,
            fix: value.fix.map(|fixes| {
                fixes
                    .into_iter()
                    .map(|f| f.into())
                    .collect::<Vec<JsLintFix>>()
            }),
        }
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub enum LintFix {
    Insert(LintFixInsert),
    Delete(LintFixDelete),
    Replace(LintFixReplace),
}

#[cfg(target_arch = "wasm32")]
#[derive(Debug, Clone, Deserialize, Serialize, Tsify)]
#[tsify(into_wasm_abi)]
pub struct JsLintFix {
    _type: String,
    point: Option<AdjustedPoint>,
    location: Option<Location>,
    text: Option<String>,
}

#[cfg(target_arch = "wasm32")]
impl From<LintFix> for JsLintFix {
    fn from(value: LintFix) -> Self {
        match value {
            LintFix::Insert(lint_fix_insert) => JsLintFix {
                _type: "insert".to_string(),
                point: Some(lint_fix_insert.point),
                location: None,
                text: Some(lint_fix_insert.text),
            },
            LintFix::Delete(lint_fix_delete) => JsLintFix {
                _type: "delete".to_string(),
                point: None,
                location: Some(lint_fix_delete.location),
                text: None,
            },
            LintFix::Replace(lint_fix_replace) => JsLintFix {
                _type: "replace".to_string(),
                point: None,
                location: Some(lint_fix_replace.location),
                text: Some(lint_fix_replace.text),
            },
        }
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct LintFixInsert {
    /// Text is inserted in front of this point
    pub point: AdjustedPoint,
    pub text: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct LintFixDelete {
    pub location: Location,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct LintFixReplace {
    pub location: Location,
    pub text: String,
}

impl LintError {
    pub fn new(message: String, location: Location, fix: Option<Vec<LintFix>>) -> Self {
        Self {
            message,
            location,
            fix,
        }
    }

    pub fn from_node(node: &Node, context: &RuleContext, message: &str) -> Option<Self> {
        if let Some(position) = node.position() {
            let location = Location::from_position(position, context);
            Some(Self::new(message.into(), location, None))
        } else {
            None
        }
    }

    pub fn from_node_with_fix(
        node: &Node,
        context: &RuleContext,
        message: &str,
        fix: Vec<LintFix>,
    ) -> Option<Self> {
        let mut lint_error = Self::from_node(node, context, message)?;
        lint_error.fix = Some(fix);
        Some(lint_error)
    }
}
