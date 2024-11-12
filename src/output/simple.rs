//! Outputs linter diagnostics in the simple format, for CLI display, which has
//! the structure:
//!
//! ```text
//! <file path>:<line>:<column>: [<severity>] <msg>
//! ```
//!
//! The diagnostics are followed by a summary of the number of linted files,
//! total errors, and total warnings.

use std::{collections::HashSet, io::Write};

use anyhow::Result;
use log::warn;

use super::{LintOutput, OutputFormatter};

pub struct SimpleFormatter;

impl OutputFormatter for SimpleFormatter {
    fn format<Writer: Write>(&self, output: &[LintOutput], io: &mut Writer) -> Result<()> {
        // Whether anything has been written to the output, used to determine
        // whether to write a newline before the summary.
        let mut written = false;

        for output in output.iter() {
            for error in output.errors.iter() {
                written = true;
                match writeln!(
                    io,
                    "{}:{}:{}: [ERROR] {}",
                    output.file_path,
                    error.location.start().line,
                    error.location.start().column,
                    error.message,
                ) {
                    Ok(_) => {}
                    Err(err) => {
                        warn!("Failed to write to output: {}", err);
                        return Err(err.into());
                    }
                }
            }
        }

        if written {
            writeln!(io)?;
        }
        SimpleFormatter::write_summary(output, io)?;

        Ok(())
    }
}

impl SimpleFormatter {
    fn write_summary(output: &[LintOutput], io: &mut impl Write) -> Result<()> {
        let mut seen_files = HashSet::<&str>::new();
        let mut num_errors = 0;
        let mut num_warnings = 0;

        for o in output {
            seen_files.insert(&o.file_path);
            for e in &o.errors {
                num_errors += 1;
            }
        }

        let diagnostic_message = match (num_errors, num_warnings) {
            (0, 0) => "🟢 No errors or warnings found",
            (0, num_warnings) => &format!(
                "🟡 Found {} warning{}",
                num_warnings,
                if num_warnings > 1 { "s" } else { "" }
            ),
            (num_errors, 0) => &format!(
                "🔴 Found {} error{}",
                num_errors,
                if num_errors > 1 { "s" } else { "" }
            ),
            (num_errors, num_warnings) => &format!(
                "🔴 Found {} error{} and {} warning{}",
                num_errors,
                if num_errors > 1 { "s" } else { "" },
                num_warnings,
                if num_warnings > 1 { "s" } else { "" }
            ),
        };

        writeln!(
            io,
            "🔍 {} source{} linted",
            seen_files.len(),
            if seen_files.len() > 1 { "s" } else { "" }
        )?;
        writeln!(io, "{}", diagnostic_message)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{document::Location, errors::LintError};

    #[test]
    fn test_simple_formatter() {
        let file_path = "test.md".to_string();
        let error = LintError {
            message: "This is an error".to_string(),
            location: Location::dummy(1, 1, 0, 1, 2, 1),
            fix: None,
        };

        let output = LintOutput {
            file_path,
            errors: vec![error],
        };
        let output = vec![output];

        let formatter = SimpleFormatter;
        let mut result = Vec::new();
        formatter.format(&output, &mut result).unwrap();
        assert_eq!(
            String::from_utf8(result).unwrap(),
            "test.md:1:1: [ERROR] This is an error\n\n🔍 1 source linted\n🔴 Found 1 error\n"
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
        let mut result = Vec::new();
        formatter.format(&output, &mut result).unwrap();
        assert_eq!(
            String::from_utf8(result).unwrap(),
            "🔍 1 source linted\n🟢 No errors or warnings found\n"
        );
    }

    #[test]
    fn test_simple_formatter_multiple_errors() {
        let file_path = "test.md".to_string();
        let error_1 = LintError {
            message: "This is an error".to_string(),
            location: Location::dummy(1, 1, 0, 1, 2, 1),
            fix: None,
        };
        let error_2 = LintError {
            message: "This is another error".to_string(),
            location: Location::dummy(2, 1, 10, 2, 2, 11),
            fix: None,
        };

        let output = LintOutput {
            file_path,
            errors: vec![error_1, error_2],
        };
        let output = vec![output];

        let formatter = SimpleFormatter;
        let mut result = Vec::new();
        formatter.format(&output, &mut result).unwrap();
        assert_eq!(
            String::from_utf8(result).unwrap(),
            "test.md:1:1: [ERROR] This is an error\ntest.md:2:1: [ERROR] This is another error\n\n🔍 1 source linted\n🔴 Found 2 errors\n"
        );
    }

    #[test]
    fn test_simple_formatter_multiple_files() {
        let file_path_1 = "test.md".to_string();
        let error_1 = LintError {
            message: "This is an error".to_string(),
            location: Location::dummy(1, 1, 0, 1, 2, 1),
            fix: None,
        };
        let error_2 = LintError {
            message: "This is another error".to_string(),
            location: Location::dummy(2, 1, 10, 2, 2, 11),
            fix: None,
        };

        let output_1 = LintOutput {
            file_path: file_path_1,
            errors: vec![error_1, error_2],
        };

        let file_path_2 = "test2.md".to_string();
        let error_3 = LintError {
            message: "This is an error".to_string(),
            location: Location::dummy(1, 1, 0, 1, 2, 1),
            fix: None,
        };
        let error_4 = LintError {
            message: "This is another error".to_string(),
            location: Location::dummy(2, 1, 10, 2, 2, 11),
            fix: None,
        };

        let output_2 = LintOutput {
            file_path: file_path_2,
            errors: vec![error_3, error_4],
        };

        let output = vec![output_1, output_2];

        let formatter = SimpleFormatter;
        let mut result = Vec::new();
        formatter.format(&output, &mut result).unwrap();
        assert_eq!(
            String::from_utf8(result).unwrap(),
            "test.md:1:1: [ERROR] This is an error\ntest.md:2:1: [ERROR] This is another error\ntest2.md:1:1: [ERROR] This is an error\ntest2.md:2:1: [ERROR] This is another error\n\n🔍 2 sources linted\n🔴 Found 4 errors\n"
        );
    }
}
