use std::fmt::Write;

use anyhow::Result;
use log::{debug, warn};
use serde::Serialize;

use crate::{
    errors::LintLevel,
    fix::LintCorrection,
    location::{AdjustedPoint, DenormalizedLocation},
    output::OutputFormatter,
    ConfigMetadata,
};

use super::LintOutput;

/// Outputs linter diagnostics in the
/// [Reviewdog Diagnostic Format](https://github.com/reviewdog/reviewdog/tree/master/proto/rdf).
///
/// Uses the `rdjsonl` form, which has the structure:
///
/// ```text
/// {"message": "<msg>", "location": {"path": "<file path>", "range": {"start": {"line": 14, "column": 15}}}, "severity": "ERROR"}
/// {"message": "<msg>", "location": {"path": "<file path>", "range": {"start": {"line": 14, "column": 15}, "end": {"line": 14, "column": 18}}}, "suggestions": [{"range": {"start": {"line": 14, "column": 15}, "end": {"line": 14, "column": 18}}, "text": "<replacement text>"}], "severity": "WARNING"}
/// ```
#[derive(Debug, Clone)]
pub struct RdfFormatter;

#[derive(Debug, PartialEq, Eq, Serialize)]
struct RdfOutput<'output> {
    message: &'output str,
    location: RdfLocation<'output>,
    severity: &'output LintLevel,
    #[serde(skip_serializing_if = "Option::is_none")]
    suggestions: Option<Vec<RdfSuggestion<'output>>>,
}

#[derive(Debug, PartialEq, Eq, Serialize)]
struct RdfLocation<'location> {
    path: &'location str,
    range: RdfRange,
}

#[derive(Debug, PartialEq, Eq, Serialize)]
struct RdfRange {
    start: RdfPosition,
    end: RdfPosition,
}

impl From<DenormalizedLocation> for RdfRange {
    fn from(location: DenormalizedLocation) -> Self {
        Self::from(&location)
    }
}

impl From<&DenormalizedLocation> for RdfRange {
    fn from(location: &DenormalizedLocation) -> Self {
        Self {
            start: (&location.start).into(),
            end: (&location.end).into(),
        }
    }
}

#[derive(Debug, PartialEq, Eq, Serialize)]
struct RdfPosition {
    line: usize,
    column: usize,
}

impl From<&AdjustedPoint> for RdfPosition {
    fn from(point: &AdjustedPoint) -> Self {
        Self {
            line: point.row + 1,
            column: point.column + 1,
        }
    }
}

#[derive(Debug, PartialEq, Eq, Serialize)]
struct RdfSuggestion<'suggestion> {
    range: RdfRange,
    text: &'suggestion str,
}

impl<'fix> RdfSuggestion<'fix> {
    fn from_lint_fix(fix: &'fix LintCorrection) -> Self {
        match fix {
            LintCorrection::Insert(fix) => Self {
                range: (&fix.location).into(),
                text: &fix.text,
            },
            LintCorrection::Delete(fix) => Self {
                range: (&fix.location).into(),
                text: "",
            },
            LintCorrection::Replace(fix) => Self {
                range: (&fix.location).into(),
                text: &fix.text,
            },
        }
    }
}

impl OutputFormatter for RdfFormatter {
    fn id(&self) -> &'static str {
        "rdf"
    }

    fn should_log_metadata(&self) -> bool {
        false
    }

    fn format(&self, outputs: &[LintOutput], metadata: &ConfigMetadata) -> Result<String> {
        let mut result = String::new();
        for output in outputs.iter() {
            for error in output.errors.iter() {
                let suggestions = match (error.fix.as_ref(), error.suggestions.as_ref()) {
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
                };

                let mut message = String::new();
                write!(
                    message,
                    "[{}] {}{}",
                    error.rule,
                    error.message,
                    if let Some(location) = metadata
                        .config_file_locations
                        .as_ref()
                        .and_then(|locations| locations.get(&error.rule))
                    {
                        format!(" (configure rule at {location})")
                    } else {
                        "".to_string()
                    }
                )?;

                let rdf_output = RdfOutput {
                    message: &message,
                    location: RdfLocation {
                        path: &output.file_path,
                        range: (&error.location).into(),
                    },
                    severity: &error.level,
                    suggestions: suggestions.map(|fix| {
                        fix.iter()
                            .map(|corr| RdfSuggestion::from_lint_fix(corr))
                            .collect()
                    }),
                };
                debug!("Writing to ReviewDog output format: {rdf_output:?}");

                let json_string = match serde_json::to_string(&rdf_output) {
                    Ok(json_string) => json_string,
                    Err(err) => {
                        warn!("Failed to serialize output: {}", err);
                        return Err(err.into());
                    }
                };
                result.push_str(&json_string);
                result.push('\n');
            }
        }

        Ok(result)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        errors::LintError,
        fix::{LintCorrection, LintCorrectionDelete, LintCorrectionReplace},
    };

    #[test]
    fn test_rdf_formatter() {
        let file_path = "test.md".to_string();
        let error = LintError::from_raw_location()
            .rule("MockRule")
            .level(LintLevel::Error)
            .message("This is an error")
            .location(DenormalizedLocation::dummy(0, 7, 0, 0, 1, 0))
            .call();

        let output = LintOutput {
            file_path,
            errors: vec![error],
        };
        let output = vec![output];

        let formatter = RdfFormatter;
        let result = formatter
            .format(&output, &ConfigMetadata::default())
            .unwrap();
        let result = result.trim();
        let expected = r#"{"message":"[MockRule] This is an error","location":{"path":"test.md","range":{"start":{"line":1,"column":1},"end":{"line":2,"column":1}}},"severity":"ERROR"}"#;
        assert_eq!(result, expected);
    }

    #[test]
    fn test_rdf_formatter_with_fixes() {
        let file_path = "test.md".to_string();
        let error = LintError::from_raw_location()
            .rule("MockRule")
            .level(LintLevel::Error)
            .message("This is an error")
            .location(DenormalizedLocation::dummy(0, 8, 0, 0, 0, 8))
            .fix(vec![LintCorrection::Delete(LintCorrectionDelete {
                location: DenormalizedLocation::dummy(0, 8, 0, 0, 0, 8),
            })])
            .call();
        let output = LintOutput {
            file_path,
            errors: vec![error],
        };
        let output = vec![output];

        let formatter = RdfFormatter;
        let result = formatter
            .format(&output, &ConfigMetadata::default())
            .unwrap();

        let result = result.trim();
        let expected = r#"{"message":"[MockRule] This is an error","location":{"path":"test.md","range":{"start":{"line":1,"column":1},"end":{"line":1,"column":9}}},"severity":"ERROR","suggestions":[{"range":{"start":{"line":1,"column":1},"end":{"line":1,"column":9}},"text":""}]}"#;
        assert_eq!(result, expected);
    }

    #[test]
    fn test_rdf_formatter_multiple_errors() {
        let file_path = "test.md".to_string();
        let error_1 = LintError::from_raw_location()
            .rule("MockRule")
            .level(LintLevel::Error)
            .message("This is an error")
            .location(DenormalizedLocation::dummy(0, 7, 0, 0, 1, 0))
            .call();
        let error_2 = LintError::from_raw_location()
            .rule("MockRule")
            .level(LintLevel::Error)
            .message("This is another error")
            .location(DenormalizedLocation::dummy(0, 7, 0, 0, 4, 2))
            .call();

        let output = LintOutput {
            file_path,
            errors: vec![error_1, error_2],
        };
        let output = vec![output];

        let formatter = RdfFormatter;
        let result = formatter
            .format(&output, &ConfigMetadata::default())
            .unwrap();

        let result = result.trim();
        let expected = r#"{"message":"[MockRule] This is an error","location":{"path":"test.md","range":{"start":{"line":1,"column":1},"end":{"line":2,"column":1}}},"severity":"ERROR"}
{"message":"[MockRule] This is another error","location":{"path":"test.md","range":{"start":{"line":1,"column":1},"end":{"line":5,"column":3}}},"severity":"ERROR"}"#;
        assert_eq!(result, expected);
    }

    #[test]
    fn test_rdf_formatter_multiple_files() {
        let file_path_1 = "test.md".to_string();
        let error_1 = LintError::from_raw_location()
            .rule("MockRule")
            .level(LintLevel::Error)
            .message("This is an error")
            .location(DenormalizedLocation::dummy(0, 7, 0, 0, 1, 0))
            .call();
        let error_2 = LintError::from_raw_location()
            .rule("MockRule")
            .level(LintLevel::Error)
            .message("This is another error")
            .location(DenormalizedLocation::dummy(0, 7, 0, 0, 1, 0))
            .call();

        let output_1 = LintOutput {
            file_path: file_path_1,
            errors: vec![error_1.clone(), error_2.clone()],
        };

        let file_path_2 = "test2.md".to_string();

        let output_2 = LintOutput {
            file_path: file_path_2,
            errors: vec![error_1, error_2],
        };

        let output = vec![output_1, output_2];

        let formatter = RdfFormatter;
        let result = formatter
            .format(&output, &ConfigMetadata::default())
            .unwrap();

        let result = result.trim();
        let expected = r#"{"message":"[MockRule] This is an error","location":{"path":"test.md","range":{"start":{"line":1,"column":1},"end":{"line":2,"column":1}}},"severity":"ERROR"}
{"message":"[MockRule] This is another error","location":{"path":"test.md","range":{"start":{"line":1,"column":1},"end":{"line":2,"column":1}}},"severity":"ERROR"}
{"message":"[MockRule] This is an error","location":{"path":"test2.md","range":{"start":{"line":1,"column":1},"end":{"line":2,"column":1}}},"severity":"ERROR"}
{"message":"[MockRule] This is another error","location":{"path":"test2.md","range":{"start":{"line":1,"column":1},"end":{"line":2,"column":1}}},"severity":"ERROR"}"#;
        assert_eq!(result, expected);
    }

    #[test]
    fn test_rdf_formatter_with_fixes_and_suggestions() {
        let file_path = "test.md".to_string();
        let error = LintError::from_raw_location()
            .rule("MockRule")
            .level(LintLevel::Error)
            .message("This is an error with fixes and suggestions")
            .location(DenormalizedLocation::dummy(0, 8, 0, 0, 0, 8))
            .fix(vec![LintCorrection::Delete(LintCorrectionDelete {
                location: DenormalizedLocation::dummy(0, 8, 0, 0, 0, 8),
            })])
            .suggestions(vec![LintCorrection::Replace(LintCorrectionReplace {
                location: DenormalizedLocation::dummy(0, 8, 0, 0, 0, 8),
                text: "replacement text".to_string(),
            })])
            .call();
        let output = LintOutput {
            file_path,
            errors: vec![error],
        };
        let output = vec![output];

        let formatter = RdfFormatter;
        let result = formatter
            .format(&output, &ConfigMetadata::default())
            .unwrap();

        let result = result.trim();
        let expected = r#"{"message":"[MockRule] This is an error with fixes and suggestions","location":{"path":"test.md","range":{"start":{"line":1,"column":1},"end":{"line":1,"column":9}}},"severity":"ERROR","suggestions":[{"range":{"start":{"line":1,"column":1},"end":{"line":1,"column":9}},"text":""},{"range":{"start":{"line":1,"column":1},"end":{"line":1,"column":9}},"text":"replacement text"}]}"#;
        assert_eq!(result, expected);
    }

    #[test]
    fn test_rdf_formatter_with_only_suggestions() {
        let file_path = "test.md".to_string();
        let error = LintError::from_raw_location()
            .rule("MockRule")
            .level(LintLevel::Error)
            .message("This is an error with only suggestions")
            .location(DenormalizedLocation::dummy(0, 8, 0, 0, 0, 8))
            .suggestions(vec![LintCorrection::Replace(LintCorrectionReplace {
                location: DenormalizedLocation::dummy(0, 8, 0, 0, 0, 8),
                text: "replacement text".to_string(),
            })])
            .call();
        let output = LintOutput {
            file_path,
            errors: vec![error],
        };
        let output = vec![output];

        let formatter = RdfFormatter;
        let result = formatter
            .format(&output, &ConfigMetadata::default())
            .unwrap();

        let result = result.trim();
        let expected = r#"{"message":"[MockRule] This is an error with only suggestions","location":{"path":"test.md","range":{"start":{"line":1,"column":1},"end":{"line":1,"column":9}}},"severity":"ERROR","suggestions":[{"range":{"start":{"line":1,"column":1},"end":{"line":1,"column":9}},"text":"replacement text"}]}"#;
        assert_eq!(result, expected);
    }
}
