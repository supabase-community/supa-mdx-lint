use std::fs;

use anyhow::Result;
use miette::{miette, LabeledSpan, NamedSource, Severity};

use crate::{errors::LintLevel, output::OutputFormatter};

use super::{LintOutput, OutputSummary};

impl From<LintLevel> for Severity {
    fn from(level: LintLevel) -> Self {
        match level {
            LintLevel::Error => Severity::Error,
            LintLevel::Warning => Severity::Warning,
        }
    }
}

/// Outputs linter diagnostics in the pretty format, for CLI display, using
/// Miette.
///
/// The diagnostics are followed by a summary of the number of linted files,
/// total errors, and total warnings.
#[derive(Debug, Clone)]
pub struct PrettyFormatter;

impl OutputFormatter for PrettyFormatter {
    fn id(&self) -> &'static str {
        "pretty"
    }

    fn should_log_metadata(&self) -> bool {
        true
    }

    fn format(&self, output: &[LintOutput]) -> Result<String> {
        let mut result = String::new();
        // Whether anything has been written to the result, used to determine
        // whether to write a newline before each section.
        let mut written = false;

        for curr in output.iter() {
            if curr.errors.is_empty() {
                continue;
            }
            if written {
                result.push('\n');
            }
            written |= true;

            let content = fs::read_to_string(&curr.file_path)?;

            for (idx, error) in curr.errors.iter().enumerate() {
                if idx > 0 {
                    result.push('\n');
                }

                let severity: Severity = error.level.into();
                let message = error.message.clone();

                let error = miette!(
                    severity = severity,
                    labels = vec![LabeledSpan::at(
                        error.location.offset_range.to_usize_range(),
                        "here"
                    )],
                    "{}",
                    message
                )
                .with_source_code(NamedSource::new(&curr.file_path, content.clone()));
                result.push_str(&format!("{:?}", error));
            }
        }

        if written {
            result.push('\n');
        }
        result.push_str(&self.format_summary(output));

        Ok(result)
    }
}

impl PrettyFormatter {
    fn format_summary(&self, output: &[LintOutput]) -> String {
        let mut result = String::new();
        let OutputSummary {
            num_files,
            num_errors,
            num_warnings,
        } = self.get_summary(output);

        let diagnostic_message = match (num_errors, num_warnings) {
            (0, 0) => "🟢 No errors or warnings found",
            (0, num_warnings) => &format!(
                "🟡 Found {} warning{}",
                num_warnings,
                if num_warnings != 1 { "s" } else { "" }
            ),
            (num_errors, 0) => &format!(
                "🔴 Found {} error{}",
                num_errors,
                if num_errors != 1 { "s" } else { "" }
            ),
            (num_errors, num_warnings) => &format!(
                "🔴 Found {} error{} and {} warning{}",
                num_errors,
                if num_errors != 1 { "s" } else { "" },
                num_warnings,
                if num_warnings != 1 { "s" } else { "" }
            ),
        };

        result.push_str(&format!(
            "🔍 {} source{} linted\n",
            num_files,
            if num_files != 1 { "s" } else { "" }
        ));
        result.push_str(diagnostic_message);
        result
    }
}

#[cfg(test)]
mod tests {
    use tempfile::TempDir;

    use super::*;
    use crate::{
        errors::{LintError, LintLevel},
        location::DenormalizedLocation,
    };

    #[test]
    fn test_pretty_formatter() {
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("test.md");
        fs::write(&file_path, "# Hello World").unwrap();

        let error = LintError::from_raw_location()
            .rule("MockRule")
            .level(LintLevel::Error)
            .message("This is an error")
            .location(DenormalizedLocation::dummy(8, 13, 0, 8, 0, 13))
            .call();

        let file_path = file_path.to_string_lossy().to_string();
        let output = LintOutput {
            file_path: file_path.clone(),
            errors: vec![error],
        };
        let output = vec![output];

        let formatter = PrettyFormatter;
        let result = formatter.format(&output).unwrap();

        assert!(result.contains("test.md"));
        assert!(result.contains("This is an error"));
        assert!(result.contains("# Hello World"));
        assert!(result.contains("1 error"));
    }

    #[test]
    fn test_pretty_formatter_no_errors() {
        let file_path = "test.md".to_string();
        let output = LintOutput {
            file_path,
            errors: vec![],
        };
        let output = vec![output];

        let formatter = PrettyFormatter;
        let result = formatter.format(&output).unwrap();

        assert!(result.contains("1 source"));
        assert!(result.contains("No errors"));
    }

    #[test]
    fn test_pretty_formatter_multiple_errors() {
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("test.md");
        fs::write(&file_path, "# Hello World\n\n# Hello World").unwrap();

        let error_1 = LintError::from_raw_location()
            .rule("MockRule")
            .level(LintLevel::Error)
            .message("This is an error")
            .location(DenormalizedLocation::dummy(8, 13, 0, 8, 0, 13))
            .call();
        let error_2 = LintError::from_raw_location()
            .rule("MockRule")
            .level(LintLevel::Error)
            .message("This is another error")
            .location(DenormalizedLocation::dummy(23, 28, 2, 8, 2, 13))
            .call();

        let file_path = file_path.to_string_lossy().to_string();
        let output = LintOutput {
            file_path: file_path.clone(),
            errors: vec![error_1, error_2],
        };
        let output = vec![output];

        let formatter = PrettyFormatter;
        let result = formatter.format(&output).unwrap();

        assert!(result.contains("This is an error"));
        assert!(result.contains("This is another error"));
        assert!(result.contains("1 source"));
        assert!(result.contains("2 errors"));
    }

    #[test]
    fn test_pretty_formatter_multiple_files() {
        let temp_dir = TempDir::new().unwrap();
        let file_path_1 = temp_dir.path().join("test.md");
        fs::write(&file_path_1, "# Hello World").unwrap();
        let file_path_2 = temp_dir.path().join("test2.md");
        fs::write(&file_path_2, "# Hello World").unwrap();

        let error_1 = LintError::from_raw_location()
            .rule("MockRule")
            .level(LintLevel::Error)
            .message("This is an error")
            .location(DenormalizedLocation::dummy(8, 13, 0, 8, 0, 13))
            .call();

        let file_path_1 = file_path_1.to_string_lossy().to_string();
        let file_path_2 = file_path_2.to_string_lossy().to_string();
        let output_1 = LintOutput {
            file_path: file_path_1.clone(),
            errors: vec![error_1.clone()],
        };
        let output_2 = LintOutput {
            file_path: file_path_2.clone(),
            errors: vec![error_1],
        };

        let output = vec![output_1, output_2];

        let formatter = PrettyFormatter;
        let result = formatter.format(&output).unwrap();

        assert!(result.contains("test.md"));
        assert!(result.contains("test2.md"));
        assert!(result.contains("This is an error"));
        assert!(result.contains("2 sources"));
        assert!(result.contains("2 errors"));
    }
}
