use std::io::Write;

use anyhow::Result;
use log::{debug, warn};
use serde::Serialize;

use crate::{
    document::{AdjustedPoint, Location},
    errors::LintLevel,
    fix::LintFix,
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

impl From<&Location> for RdfRange {
    fn from(location: &Location) -> Self {
        Self {
            start: location.start().into(),
            end: location.end().into(),
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
            line: point.line.into(),
            column: point.column.into(),
        }
    }
}

#[derive(Debug, PartialEq, Eq, Serialize)]
struct RdfSuggestion<'suggestion> {
    range: RdfRange,
    text: &'suggestion str,
}

impl<'fix> RdfSuggestion<'fix> {
    fn from_lint_fix(fix: &'fix LintFix) -> Self {
        match fix {
            LintFix::Insert(fix) => Self {
                range: RdfRange {
                    start: (&fix.point).into(),
                    end: RdfPosition {
                        line: fix.point.line.into(),
                        column: Into::<usize>::into(fix.point.column) + 1,
                    },
                },
                text: &fix.text,
            },
            LintFix::Delete(fix) => Self {
                range: (&fix.location).into(),
                text: "",
            },
            LintFix::Replace(fix) => Self {
                range: (&fix.location).into(),
                text: &fix.text,
            },
        }
    }
}

impl RdfFormatter {
    pub(super) fn format<Writer: Write>(
        &self,
        output: &[LintOutput],
        io: &mut Writer,
    ) -> Result<()> {
        for output in output.iter() {
            for error in output.errors.iter() {
                let rdf_output = RdfOutput {
                    message: &error.message,
                    location: RdfLocation {
                        path: &output.file_path,
                        range: (&error.location).into(),
                    },
                    severity: &error.level,
                    suggestions: error
                        .fix
                        .as_ref()
                        .map(|fix| fix.iter().map(RdfSuggestion::from_lint_fix).collect()),
                };
                debug!("Writing to ReviewDog output format: {rdf_output:?}");

                let json_string = match serde_json::to_string(&rdf_output) {
                    Ok(json_string) => json_string,
                    Err(err) => {
                        warn!("Failed to serialize output: {}", err);
                        return Err(err.into());
                    }
                };
                match writeln!(io, "{}", json_string) {
                    Ok(_) => {}
                    Err(err) => {
                        warn!("Failed to write to output: {}", err);
                        return Err(err.into());
                    }
                }
            }
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        document::Location,
        errors::LintError,
        fix::{LintFix, LintFixDelete},
    };

    #[test]
    fn test_rdf_formatter() {
        let file_path = "test.md".to_string();
        let error = LintError {
            level: LintLevel::Error,
            message: "This is an error".to_string(),
            location: Location::dummy(1, 1, 0, 1, 2, 1),
            fix: None,
        };

        let output = LintOutput {
            file_path,
            errors: vec![error],
        };
        let output = vec![output];

        let formatter = RdfFormatter;
        let mut result = Vec::new();
        formatter.format(&output, &mut result).unwrap();

        let result = String::from_utf8(result).unwrap();
        let result = result.trim();
        let expected = r#"{"message":"This is an error","location":{"path":"test.md","range":{"start":{"line":1,"column":1},"end":{"line":1,"column":2}}},"severity":"ERROR"}"#;
        assert_eq!(result, expected);
    }

    #[test]
    fn test_rdf_formatter_with_fixes() {
        let file_path = "test.md".to_string();
        let error = LintError {
            level: LintLevel::Error,
            message: "This is an error".to_string(),
            location: Location::dummy(1, 1, 0, 1, 9, 8),
            fix: Some(vec![LintFix::Delete(LintFixDelete {
                location: Location::dummy(1, 1, 0, 1, 9, 8),
            })]),
        };
        let output = LintOutput {
            file_path,
            errors: vec![error],
        };
        let output = vec![output];

        let formatter = RdfFormatter;
        let mut result = Vec::new();
        formatter.format(&output, &mut result).unwrap();

        let result = String::from_utf8(result).unwrap();
        let result = result.trim();
        let expected = r#"{"message":"This is an error","location":{"path":"test.md","range":{"start":{"line":1,"column":1},"end":{"line":1,"column":9}}},"severity":"ERROR","suggestions":[{"range":{"start":{"line":1,"column":1},"end":{"line":1,"column":9}},"text":""}]}"#;
        assert_eq!(result, expected);
    }

    #[test]
    fn test_rdf_formatter_multiple_errors() {
        let file_path = "test.md".to_string();
        let error_1 = LintError {
            level: LintLevel::Error,
            message: "This is an error".to_string(),
            location: Location::dummy(1, 1, 0, 1, 2, 1),
            fix: None,
        };
        let error_2 = LintError {
            level: LintLevel::Error,
            message: "This is another error".to_string(),
            location: Location::dummy(2, 1, 10, 2, 2, 11),
            fix: None,
        };

        let output = LintOutput {
            file_path,
            errors: vec![error_1, error_2],
        };
        let output = vec![output];

        let formatter = RdfFormatter;
        let mut result = Vec::new();
        formatter.format(&output, &mut result).unwrap();

        let result = String::from_utf8(result).unwrap();
        let result = result.trim();
        let expected = r#"{"message":"This is an error","location":{"path":"test.md","range":{"start":{"line":1,"column":1},"end":{"line":1,"column":2}}},"severity":"ERROR"}
{"message":"This is another error","location":{"path":"test.md","range":{"start":{"line":2,"column":1},"end":{"line":2,"column":2}}},"severity":"ERROR"}"#;
        assert_eq!(result, expected);
    }

    #[test]
    fn test_rdf_formatter_multiple_files() {
        let file_path_1 = "test.md".to_string();
        let error_1 = LintError {
            level: LintLevel::Error,
            message: "This is an error".to_string(),
            location: Location::dummy(1, 1, 0, 1, 2, 1),
            fix: None,
        };
        let error_2 = LintError {
            level: LintLevel::Error,
            message: "This is another error".to_string(),
            location: Location::dummy(2, 1, 10, 2, 2, 11),
            fix: None,
        };

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
        let mut result = Vec::new();
        formatter.format(&output, &mut result).unwrap();

        let result = String::from_utf8(result).unwrap();
        let result = result.trim();
        let expected = r#"{"message":"This is an error","location":{"path":"test.md","range":{"start":{"line":1,"column":1},"end":{"line":1,"column":2}}},"severity":"ERROR"}
{"message":"This is another error","location":{"path":"test.md","range":{"start":{"line":2,"column":1},"end":{"line":2,"column":2}}},"severity":"ERROR"}
{"message":"This is an error","location":{"path":"test2.md","range":{"start":{"line":1,"column":1},"end":{"line":1,"column":2}}},"severity":"ERROR"}
{"message":"This is another error","location":{"path":"test2.md","range":{"start":{"line":2,"column":1},"end":{"line":2,"column":2}}},"severity":"ERROR"}"#;
        assert_eq!(result, expected);
    }
}
