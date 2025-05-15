use anyhow::Result;

use crate::{output::OutputFormatter, ConfigMetadata};

use super::{LintOutput, OutputSummary};

/// Outputs linter diagnostics in the simple format, for CLI display, which has
/// the structure:
///
/// ```text
/// <file path>:<line>:<column>: [<severity>] <msg>
/// ```
///
/// The diagnostics are followed by a summary of the number of linted files,
/// total errors, and total warnings.
#[derive(Debug, Clone)]
pub struct SimpleFormatter;

impl OutputFormatter for SimpleFormatter {
    fn id(&self) -> &'static str {
        "simple"
    }

    fn format(&self, output: &[LintOutput], _metadata: &ConfigMetadata) -> Result<String> {
        let mut result = String::new();
        // Whether anything has been written to the output, used to determine
        // whether to write a newline before the summary.
        let mut written = false;

        for output in output.iter() {
            for error in output.errors.iter() {
                written |= true;

                result.push_str(&format!(
                    "{}:{}:{}: [{}] {}\n",
                    output.file_path,
                    error.location.start.row + 1,
                    error.location.start.column + 1,
                    error.level,
                    error.message,
                ));
            }
        }

        if written {
            result.push('\n');
        }
        result.push_str(&self.format_summary(output));

        Ok(result)
    }

    fn should_log_metadata(&self) -> bool {
        true
    }
}

impl SimpleFormatter {
    fn format_summary(&self, output: &[LintOutput]) -> String {
        let mut result = String::new();
        let OutputSummary {
            num_errors,
            num_files,
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
        result.push_str(&format!("{}\n", diagnostic_message));
        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        errors::{LintError, LintLevel},
        location::DenormalizedLocation,
    };

    #[test]
    fn test_simple_formatter() {
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

        let formatter = SimpleFormatter;
        let result = formatter
            .format(&output, &ConfigMetadata::default())
            .unwrap();
        assert_eq!(
            result,
            "test.md:1:1: [ERROR] This is an error\n\n🔍 1 source linted\n🔴 Found 1 error\n"
        );
    }

    #[test]
    fn test_simple_formatter_warning() {
        let file_path = "test.md".to_string();
        let error = LintError::from_raw_location()
            .rule("MockRule")
            .level(LintLevel::Warning)
            .message("This is a warning")
            .location(DenormalizedLocation::dummy(0, 7, 0, 0, 1, 0))
            .call();
        let output = LintOutput {
            file_path,
            errors: vec![error],
        };
        let output = vec![output];

        let formatter = SimpleFormatter;
        let result = formatter
            .format(&output, &ConfigMetadata::default())
            .unwrap();
        assert_eq!(
            result,
            "test.md:1:1: [WARN] This is a warning\n\n🔍 1 source linted\n🟡 Found 1 warning\n"
        );
    }

    #[test]
    fn test_simple_formatter_warning_and_error() {
        let file_path = "test.md".to_string();
        let error1 = LintError::from_raw_location()
            .rule("MockRule")
            .level(LintLevel::Error)
            .message("This is an error")
            .location(DenormalizedLocation::dummy(0, 7, 0, 0, 1, 0))
            .call();
        let error2 = LintError::from_raw_location()
            .rule("MockRule")
            .level(LintLevel::Warning)
            .message("This is a warning")
            .location(DenormalizedLocation::dummy(14, 46, 3, 0, 4, 2))
            .call();
        let output = LintOutput {
            file_path,
            errors: vec![error1, error2],
        };
        let output = vec![output];

        let formatter = SimpleFormatter;
        let result = formatter
            .format(&output, &ConfigMetadata::default())
            .unwrap();
        assert_eq!(
            result,
            "test.md:1:1: [ERROR] This is an error\ntest.md:4:1: [WARN] This is a warning\n\n🔍 1 source linted\n🔴 Found 1 error and 1 warning\n"
        );
    }

    #[test]
    fn test_simple_formatter_no_errors() {
        let file_path = "test.md".to_string();
        let output = LintOutput {
            file_path,
            errors: vec![],
        };
        let output = vec![output];

        let formatter = SimpleFormatter;
        let result = formatter
            .format(&output, &ConfigMetadata::default())
            .unwrap();
        assert_eq!(
            result,
            "🔍 1 source linted\n🟢 No errors or warnings found\n"
        );
    }

    #[test]
    fn test_simple_formatter_multiple_errors() {
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
            .location(DenormalizedLocation::dummy(14, 46, 3, 0, 4, 2))
            .call();

        let output = LintOutput {
            file_path,
            errors: vec![error_1, error_2],
        };
        let output = vec![output];

        let formatter = SimpleFormatter;
        let result = formatter
            .format(&output, &ConfigMetadata::default())
            .unwrap();
        assert_eq!(
            result,
            "test.md:1:1: [ERROR] This is an error\ntest.md:4:1: [ERROR] This is another error\n\n🔍 1 source linted\n🔴 Found 2 errors\n"
        );
    }

    #[test]
    fn test_simple_formatter_multiple_files() {
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
            .location(DenormalizedLocation::dummy(14, 46, 3, 0, 4, 2))
            .call();

        let output_1 = LintOutput {
            file_path: file_path_1,
            errors: vec![error_1, error_2],
        };

        let file_path_2 = "test2.md".to_string();
        let error_3 = LintError::from_raw_location()
            .rule("MockRule")
            .level(LintLevel::Error)
            .message("This is an error")
            .location(DenormalizedLocation::dummy(0, 7, 0, 0, 1, 0))
            .call();
        let error_4 = LintError::from_raw_location()
            .rule("MockRule")
            .level(LintLevel::Error)
            .message("This is another error")
            .location(DenormalizedLocation::dummy(14, 46, 3, 0, 4, 2))
            .call();

        let output_2 = LintOutput {
            file_path: file_path_2,
            errors: vec![error_3, error_4],
        };

        let output = vec![output_1, output_2];

        let formatter = SimpleFormatter;
        let result = formatter
            .format(&output, &ConfigMetadata::default())
            .unwrap();
        assert_eq!(
            result,
            "test.md:1:1: [ERROR] This is an error\ntest.md:4:1: [ERROR] This is another error\ntest2.md:1:1: [ERROR] This is an error\ntest2.md:4:1: [ERROR] This is another error\n\n🔍 2 sources linted\n🔴 Found 4 errors\n"
        );
    }
}
