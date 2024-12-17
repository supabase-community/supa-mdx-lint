use std::{collections::HashSet, fs, io::Write};

use anyhow::Result;

use crate::{errors::LintLevel, rope::Rope};

use super::LintOutput;

/// Outputs linter diagnostics in the pretty format, for CLI display, which has
/// the structure:
///
/// ```text
/// <file Path>
/// ===========
/// [<severity>: <rule>] <msg>
/// <line> The line number containing the error
///                                       ^^^^^
///
/// [<severity>: <rule>] <msg>
/// ...
/// ```
///
/// The diagnostics are followed by a summary of the number of linted files,
/// total errors, and total warnings.
#[derive(Debug, Clone)]
pub struct PrettyFormatter;

impl PrettyFormatter {
    pub(super) fn format<Writer: Write>(
        &self,
        output: &[LintOutput],
        io: &mut Writer,
    ) -> Result<()> {
        // Whether anything has been written to the output, used to determine
        // whether to write a newline before each section.
        let mut written = false;

        for curr in output.iter() {
            if curr.errors.is_empty() {
                continue;
            }

            let content = fs::read_to_string(&curr.file_path)?;
            let rope = Rope::from(content);

            if written {
                writeln!(io)?;
            }
            written |= true;

            writeln!(io, "{}", curr.file_path)?;
            writeln!(io, "{}", "=".repeat(curr.file_path.len()))?;

            for (idx, error) in curr.errors.iter().enumerate() {
                if idx > 0 {
                    writeln!(io)?;
                }

                writeln!(io, "[{}: {}] {}", error.level, error.rule, error.message)?;

                let start_line = rope.line_of_byte(error.location.offset_range.start.into());
                let end_line = rope.line_of_byte(error.location.offset_range.end.into());

                for line_no in start_line..=end_line {
                    let line = rope.line(line_no);
                    let number_graphemes = line.graphemes().count();

                    let line_number_display = format!("{}: ", line_no + 1);
                    let line_number_length = line_number_display.len();

                    if line_no == start_line && line_no == end_line {
                        writeln!(io, "{}{}", line_number_display, line)?;

                        let (_line, start_col) =
                            rope.line_column_of_byte(error.location.offset_range.start.into());
                        let (_line, end_col) =
                            rope.line_column_of_byte(error.location.offset_range.end.into());
                        let graphemes_before = line.byte_slice(..start_col).graphemes().count();
                        let graphemes_within =
                            line.byte_slice(start_col..end_col).graphemes().count();

                        writeln!(
                            io,
                            "{}{}{}{}",
                            " ".repeat(line_number_length),
                            " ".repeat(graphemes_before),
                            "^".repeat(graphemes_within),
                            " ".repeat(number_graphemes - graphemes_before - graphemes_within)
                        )?;
                    } else if line_no == start_line {
                        writeln!(io, "{}{}", line_number_display, line)?;

                        let (_line, col) =
                            rope.line_column_of_byte(error.location.offset_range.start.into());
                        let graphemes_before = line.byte_slice(..col).graphemes().count();

                        writeln!(
                            io,
                            "{}{}{}",
                            " ".repeat(line_number_length),
                            " ".repeat(graphemes_before),
                            "^".repeat(number_graphemes - graphemes_before)
                        )?;
                    } else if line_no == end_line {
                        writeln!(io, "{}{}", " ".repeat(line_number_length), line)?;

                        let (_line, col) =
                            rope.line_column_of_byte(error.location.offset_range.end.into());
                        let graphemes_before = line.byte_slice(..col).graphemes().count();

                        writeln!(
                            io,
                            "{}{}{}",
                            " ".repeat(line_number_length),
                            "^".repeat(graphemes_before),
                            " ".repeat(number_graphemes - graphemes_before)
                        )?;
                    } else {
                        writeln!(io, "{}{}", " ".repeat(line_number_length), line)?;
                        writeln!(
                            io,
                            "{}{}",
                            " ".repeat(line_number_length),
                            "^".repeat(number_graphemes)
                        )?;
                    }
                }
            }
        }

        if written {
            writeln!(io)?;
        }
        PrettyFormatter::write_summary(output, io)?;

        Ok(())
    }
}

impl PrettyFormatter {
    fn write_summary(output: &[LintOutput], io: &mut impl Write) -> Result<()> {
        let mut seen_files = HashSet::<&str>::new();
        let mut num_errors = 0;
        let mut num_warnings = 0;

        for o in output {
            seen_files.insert(&o.file_path);
            for error in &o.errors {
                match error.level {
                    LintLevel::Error => num_errors += 1,
                    LintLevel::Warning => num_warnings += 1,
                }
            }
        }

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

        writeln!(
            io,
            "🔍 {} source{} linted",
            seen_files.len(),
            if seen_files.len() != 1 { "s" } else { "" }
        )?;
        writeln!(io, "{}", diagnostic_message)?;
        Ok(())
    }

    pub(super) fn should_log_metadata(&self) -> bool {
        true
    }
}

#[cfg(test)]
mod tests {
    use tempfile::TempDir;

    use super::*;
    use crate::{
        errors::{LintError, LintLevel},
        geometry::DenormalizedLocation,
    };

    #[test]
    fn test_pretty_formatter() {
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("test.md");
        fs::write(&file_path, "# Hello World").unwrap();

        let error = LintError {
            rule: "MockRule".to_string(),
            level: LintLevel::Error,
            message: "This is an error".to_string(),
            location: DenormalizedLocation::dummy(8, 13, 0, 8, 0, 13),
            fix: None,
        };

        let file_path = file_path.to_string_lossy().to_string();
        let output = LintOutput {
            file_path: file_path.clone(),
            errors: vec![error],
        };
        let output = vec![output];

        let formatter = PrettyFormatter;
        let mut result = Vec::new();
        formatter.format(&output, &mut result).unwrap();
        assert_eq!(
            String::from_utf8(result).unwrap(),
            format!("{file_path}\n{}\n[ERROR: MockRule] This is an error\n1: # Hello World\n           ^^^^^\n\n🔍 1 source linted\n🔴 Found 1 error\n",
                "=".repeat(file_path.len()))
        );
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
        let mut result = Vec::new();
        formatter.format(&output, &mut result).unwrap();
        assert_eq!(
            String::from_utf8(result).unwrap(),
            "🔍 1 source linted\n🟢 No errors or warnings found\n"
        );
    }

    #[test]
    fn test_pretty_formatter_multiple_errors() {
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("test.md");
        fs::write(&file_path, "# Hello World\n\n# Hello World").unwrap();

        let error_1 = LintError {
            rule: "MockRule".to_string(),
            level: LintLevel::Error,
            message: "This is an error".to_string(),
            location: DenormalizedLocation::dummy(8, 13, 0, 8, 0, 13),
            fix: None,
        };
        let error_2 = LintError {
            rule: "MockRule".to_string(),
            level: LintLevel::Error,
            message: "This is another error".to_string(),
            location: DenormalizedLocation::dummy(23, 28, 2, 8, 2, 13),
            fix: None,
        };

        let file_path = file_path.to_string_lossy().to_string();
        let output = LintOutput {
            file_path: file_path.clone(),
            errors: vec![error_1, error_2],
        };
        let output = vec![output];

        let formatter = PrettyFormatter;
        let mut result = Vec::new();
        formatter.format(&output, &mut result).unwrap();
        assert_eq!(
            String::from_utf8(result).unwrap(),
            format!("{file_path}\n{}\n[ERROR: MockRule] This is an error\n1: # Hello World\n           ^^^^^\n\n[ERROR: MockRule] This is another error\n3: # Hello World\n           ^^^^^\n\n🔍 1 source linted\n🔴 Found 2 errors\n",
                "=".repeat(file_path.len()))
        );
    }

    #[test]
    fn test_pretty_formatter_multiple_files() {
        let temp_dir = TempDir::new().unwrap();
        let file_path_1 = temp_dir.path().join("test.md");
        fs::write(&file_path_1, "# Hello World").unwrap();
        let file_path_2 = temp_dir.path().join("test2.md");
        fs::write(&file_path_2, "# Hello World").unwrap();

        let error_1 = LintError {
            rule: "MockRule".to_string(),
            level: LintLevel::Error,
            message: "This is an error".to_string(),
            location: DenormalizedLocation::dummy(8, 13, 0, 8, 0, 13),
            fix: None,
        };

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
        let mut result = Vec::new();
        formatter.format(&output, &mut result).unwrap();
        assert_eq!(
            String::from_utf8(result).unwrap(),
            format!("{file_path_1}\n{}\n[ERROR: MockRule] This is an error\n1: # Hello World\n           ^^^^^\n\n{file_path_2}\n{}\n[ERROR: MockRule] This is an error\n1: # Hello World\n           ^^^^^\n\n🔍 2 sources linted\n🔴 Found 2 errors\n",
                "=".repeat(file_path_1.len()), "=".repeat(file_path_2.len()))
        );
    }
}
