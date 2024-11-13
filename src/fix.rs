use std::{cmp::Ordering, fs};

use anyhow::Result;
use log::{debug, error, trace};
use serde::{Deserialize, Serialize};

#[cfg(target_arch = "wasm32")]
use tsify::{self, Tsify};

use crate::{
    app_error::AppError,
    document::{AdjustedPoint, Location},
    output::LintOutput,
    parser::extract_frontmatter,
    Linter,
};

#[derive(Debug, Clone, Eq, PartialEq, Deserialize, Serialize)]
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
impl From<&LintFix> for JsLintFix {
    fn from(value: &LintFix) -> Self {
        match value {
            LintFix::Insert(lint_fix_insert) => JsLintFix {
                _type: "insert".to_string(),
                point: Some(lint_fix_insert.point.clone()),
                location: None,
                text: Some(lint_fix_insert.text.clone()),
            },
            LintFix::Delete(lint_fix_delete) => JsLintFix {
                _type: "delete".to_string(),
                point: None,
                location: Some(lint_fix_delete.location.clone()),
                text: None,
            },
            LintFix::Replace(lint_fix_replace) => JsLintFix {
                _type: "replace".to_string(),
                point: None,
                location: Some(lint_fix_replace.location.clone()),
                text: Some(lint_fix_replace.text.clone()),
            },
        }
    }
}

#[derive(Debug, Clone, Eq, PartialEq, Deserialize, Serialize)]
pub struct LintFixInsert {
    /// Text is inserted in front of this point
    pub point: AdjustedPoint,
    pub text: String,
}

#[derive(Debug, Clone, Eq, PartialEq, Deserialize, Serialize)]
pub struct LintFixDelete {
    pub location: Location,
}

#[derive(Debug, Clone, Eq, PartialEq, Deserialize, Serialize)]
pub struct LintFixReplace {
    pub location: Location,
    pub text: String,
}

impl PartialOrd for LintFix {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for LintFix {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        match (self, other) {
            (LintFix::Insert(insert_a), LintFix::Insert(insert_b)) => {
                insert_a.point.cmp(&insert_b.point)
            }
            (LintFix::Insert(insert), LintFix::Delete(delete)) => {
                if delete.location.start().le(&insert.point)
                    && delete.location.end().gt(&insert.point)
                {
                    // The delete wraps the insert, so only one of these fixes
                    // should take place. Represent this as equality.
                    return Ordering::Equal;
                }

                // The two don't overlap, so the delete is either fully before
                // or fully after the insert. We can arbitrarily choose between
                // the start and the end point for comparison.
                delete.location.start().cmp(&insert.point)
            }
            (LintFix::Insert(insert), LintFix::Replace(replace)) => {
                if replace.location.start().le(&insert.point)
                    && replace.location.end().gt(&insert.point)
                {
                    // The replace wraps the insert, so only one of these fixes
                    // should take place. Represent this as equality.
                    return Ordering::Equal;
                }

                // The two don't overlap, so the replace is either fully before
                // or fully after the insert. We can arbitrarily choose between
                // the start and the end point for comparison.
                replace.location.start().cmp(&insert.point)
            }
            (LintFix::Delete(_), LintFix::Insert(_)) => other.cmp(self).reverse(),
            (LintFix::Delete(delete_a), LintFix::Delete(delete_b)) => {
                let flip = delete_a.location.start().gt(delete_b.location.start());
                if flip {
                    return other.cmp(self).reverse();
                }

                if delete_a.location.end().gt(delete_b.location.start()) {
                    // The deletes overlap either fully or partially, so only
                    // one overall fix should take place. Represent this as
                    // equality.
                    return Ordering::Equal;
                }

                Ordering::Less
            }
            (LintFix::Delete(delete), LintFix::Replace(replace)) => {
                let flip = delete.location.start().gt(replace.location.start());
                if flip {
                    return other.cmp(self).reverse();
                }

                if delete.location.end().gt(replace.location.start()) {
                    // The deletes overlap either fully or partially, so only
                    // one overall fix should take place. Represent this as
                    // equality.
                    return Ordering::Equal;
                }

                Ordering::Less
            }
            (LintFix::Replace(_), LintFix::Insert(_)) => other.cmp(self).reverse(),
            (LintFix::Replace(replace), LintFix::Delete(delete)) => {
                let flip = replace.location.start().gt(delete.location.start());
                if flip {
                    return other.cmp(self).reverse();
                }

                if replace.location.end().gt(delete.location.start()) {
                    // The ranges overlap either fully or partially, so only
                    // one overall fix should take place. Represent this as
                    // equality.
                    return Ordering::Equal;
                }

                Ordering::Less
            }
            (LintFix::Replace(replace_a), LintFix::Replace(replace_b)) => {
                let flip = replace_a.location.start().gt(replace_b.location.start());
                if flip {
                    return other.cmp(self).reverse();
                }

                if replace_a.location.end().gt(replace_b.location.start()) {
                    // The ranges overlap either fully or partially, so only
                    // one overall fix should take place. Represent this as
                    // equality.
                    return Ordering::Equal;
                }

                Ordering::Less
            }
        }
    }
}

impl LintFix {
    /// Given two conflicting fixes, choose one to apply, or create a new fix
    /// that merges the two. Returns `None` if the's not clear which one to
    /// apply.
    ///
    /// Should only be called after checking that the fixes do in fact conflict.
    fn choose_or_merge(self, other: Self) -> Option<Self> {
        match (self, other) {
            (LintFix::Insert(_), LintFix::Insert(_)) => {
                // The fixes conflict and it's not clear which one to apply.
                // Inserting multiple alternate texts in the same place is
                // likely a mistake.
                None
            }
            (LintFix::Insert(_), LintFix::Delete(delete)) => {
                // The delete overlaps the insert, so apply the delete.
                Some(LintFix::Delete(delete))
            }
            (LintFix::Insert(_), LintFix::Replace(replace)) => {
                // The replace overlaps the insert, so apply the replace.
                Some(LintFix::Replace(replace))
            }
            (LintFix::Delete(delete), LintFix::Insert(_)) => {
                // The delete overlaps the insert, so apply the delete.
                Some(LintFix::Delete(delete))
            }
            (LintFix::Delete(delete_a), LintFix::Delete(delete_b)) => {
                // The deletes overlap, so merge them.
                Some(LintFix::Delete(LintFixDelete {
                    location: Location::merge(delete_a.location, delete_b.location),
                }))
            }
            (LintFix::Delete(delete), LintFix::Replace(replace)) => {
                // If one completely overlaps the other, apply it. Otherwise,
                // return None.
                if delete.location.start().lt(replace.location.start())
                    && delete.location.end().gt(replace.location.end())
                {
                    // The delete wraps the replace, so apply the delete.
                    Some(LintFix::Delete(delete))
                } else if replace.location.start().lt(delete.location.start())
                    && replace.location.end().gt(delete.location.end())
                {
                    // The replace wraps the delete, so apply the replace.
                    Some(LintFix::Replace(replace))
                } else {
                    None
                }
            }
            (LintFix::Replace(replace), LintFix::Insert(_)) => {
                // The replace overlaps the insert, so apply the replace.
                Some(LintFix::Replace(replace))
            }
            (LintFix::Replace(replace), LintFix::Delete(delete)) => {
                // If one completely overlaps the other, apply it. Otherwise,
                // return None.
                if delete.location.start().lt(replace.location.start())
                    && delete.location.end().gt(replace.location.end())
                {
                    // The delete wraps the replace, so apply the delete.
                    Some(LintFix::Delete(delete))
                } else if replace.location.start().lt(delete.location.start())
                    && replace.location.end().gt(delete.location.end())
                {
                    // The replace wraps the delete, so apply the replace.
                    Some(LintFix::Replace(replace))
                } else {
                    None
                }
            }
            (LintFix::Replace(replace_a), LintFix::Replace(replace_b)) => {
                // If one completely overlaps the other, apply it. Otherwise,
                // return None.
                if replace_b.location.start().lt(replace_a.location.start())
                    && replace_b.location.end().gt(replace_a.location.end())
                {
                    // The replace_b wraps the replace_a, so apply the replace_b.
                    Some(LintFix::Replace(replace_b))
                } else if replace_a.location.start().lt(replace_b.location.start())
                    && replace_a.location.end().gt(replace_b.location.end())
                {
                    // The replace_a wraps the replace_b, so apply the replace_a.
                    Some(LintFix::Replace(replace_a))
                } else {
                    None
                }
            }
        }
    }
}

impl Linter {
    /// Auto-fix any fixable errors.
    ///
    /// Returns a tuple of (number of files fixed, number of errors fixed).
    pub fn fix(&self, diagnostics: &[LintOutput]) -> Result<(usize, usize)> {
        let mut files_fixed: usize = 0;
        let mut errors_fixed: usize = 0;

        let fixable_outputs: Vec<&LintOutput> = diagnostics
            .iter()
            .filter(|diagnostic| diagnostic.errors().iter().any(|error| error.fix.is_some()))
            .collect();
        if fixable_outputs.is_empty() {
            debug!("No fixable errors found for this set of diagnostics.");
            trace!("Diagnostics: {:#?}", diagnostics);
            return Ok((files_fixed, errors_fixed));
        }

        for diagnostic in fixable_outputs {
            let local_errors_fixed = Self::fix_single_file(diagnostic).inspect_err(|err| {
                error!("Error fixing file {}: {}", diagnostic.file_path(), err)
            })?;
            errors_fixed += local_errors_fixed;
            files_fixed += 1;
        }

        Ok((files_fixed, errors_fixed))
    }

    fn fix_single_file(diagnostic: &LintOutput) -> Result<usize> {
        let mut errors_fixed = 0;

        let file = diagnostic.file_path();
        debug!("Fixing errors in {file}");

        let original_content = fs::read_to_string(file).map_err(|err| {
            AppError::FileSystemError(format!("reading file {file} for auto-fixing"), err)
        })?;
        let (_frontmatter_lines, frontmatter_offset, _frontmatter, content) =
            extract_frontmatter(&original_content);
        let mut content = content.to_string();

        let fixes_to_apply = Self::calculate_fixes_to_apply(file, diagnostic);
        debug!("Fixes to apply for file {file}: {fixes_to_apply:#?}");

        for fix in fixes_to_apply {
            match fix {
                LintFix::Insert(lint_fix_insert) => {
                    content = format!(
                        "{}{}{}",
                        &content[..lint_fix_insert.point.offset],
                        lint_fix_insert.text,
                        &content[lint_fix_insert.point.offset..]
                    );
                    errors_fixed += 1;
                }
                LintFix::Delete(lint_fix_delete) => {
                    content = format!(
                        "{}{}",
                        &content[..lint_fix_delete.location.start().offset],
                        &content[lint_fix_delete.location.end().offset..]
                    );
                    errors_fixed += 1;
                }
                LintFix::Replace(lint_fix_replace) => {
                    content = format!(
                        "{}{}{}",
                        &content[..lint_fix_replace.location.start().offset],
                        lint_fix_replace.text,
                        &content[lint_fix_replace.location.end().offset..]
                    );
                    errors_fixed += 1;
                }
            }
        }

        let final_content = format!("{}{}", &original_content[..frontmatter_offset], content);

        fs::write(diagnostic.file_path(), final_content).map_err(|err| {
            AppError::FileSystemError(format!("writing file {file} post-fixing"), err)
        })?;

        Ok(errors_fixed)
    }

    fn calculate_fixes_to_apply(file: &str, diagnostic: &LintOutput) -> Vec<LintFix> {
        let mut requested_fixes: Vec<LintFix> = diagnostic
            .errors()
            .iter()
            .filter_map(|err| err.fix.clone())
            .flatten()
            .collect();
        requested_fixes.sort();
        // Reversing so that fixes are applied in reverse order, avoiding
        // offset shift.
        let requested_fixes = requested_fixes.into_iter().rev();
        debug!("Requested fixes for file {file}: {requested_fixes:#?}");

        let mut fixes_to_apply: Vec<LintFix> = Vec::new();
        for fix in requested_fixes {
            if let Some(last_scheduled_fix) = fixes_to_apply.last() {
                if last_scheduled_fix.eq(&fix) {
                    // The fixes conflict, so pick one to fix, or merge
                    // them.
                    let last_scheduled_fix = fixes_to_apply.pop().unwrap();
                    if let Some(new_fix) = last_scheduled_fix.choose_or_merge(fix) {
                        fixes_to_apply.push(new_fix);
                    }
                } else {
                    // The fixes don't conflict, so apply both.
                    fixes_to_apply.push(fix.clone());
                }
            } else {
                fixes_to_apply.push(fix.clone());
            }
        }

        fixes_to_apply
    }
}
