use std::{fs, io::Write};

use anyhow::Result;

use crate::{errors::LintError, fix::LintCorrection, rope::Rope, utils::num_digits, LintOutput};

#[derive(Debug, Clone)]
pub struct MarkdownFormatter;

impl MarkdownFormatter {
    pub fn should_log_metadata(&self) -> bool {
        true
    }

    pub fn format<Writer: Write>(&self, output: &[LintOutput], io: &mut Writer) -> Result<()> {
        writeln!(io, "# supa-mdx-lint results")?;
        writeln!(io)?;

        for output in output {
            if output.errors.is_empty() {
                continue;
            }
            writeln!(io, "## {}", output.file_path)?;
            writeln!(io)?;
            for error in &output.errors {
                self.format_error(&output.file_path, error, io)?;
            }
        }

        Ok(())
    }

    fn format_error<Writer: Write>(
        &self,
        file_path: &str,
        error: &LintError,
        io: &mut Writer,
    ) -> Result<()> {
        writeln!(io, "```")?;
        writeln!(io, "{}", self.get_error_snippet(file_path, error)?)?;
        writeln!(io, "```")?;
        writeln!(io, "{}", error.message)?;
        writeln!(io)?;
        if let Some(rec_text) = self.get_recommendations_text(error) {
            writeln!(io, "{}", rec_text)?;
        }
        Ok(())
    }

    fn get_error_snippet(&self, file_path: &str, error: &LintError) -> Result<String> {
        let content = Rope::from(fs::read_to_string(file_path)?);
        let start_row = error.location.start.row;
        let end_row = error
            .location
            .end
            .row
            .saturating_add(1)
            .min(content.line_len() - 1);

        let col_num_width = num_digits(end_row);
        let mut result = String::new();
        for row in start_row..=end_row {
            let line = content.line(row);
            let line_number_str = format!("{:width$}", row + 1, width = col_num_width);
            result += &format!("{} | {}\n", line_number_str, line);
        }
        Ok(result)
    }

    fn get_recommendations_text(&self, error: &LintError) -> Option<String> {
        let rec_length = error.fix.as_ref().map_or(0, |fix| fix.len())
            + error.suggestions.as_ref().map_or(0, |sug| sug.len());
        let all_recommendations = match (error.fix.as_ref(), error.suggestions.as_ref()) {
            (None, None) => None,
            (fix, suggestions) => {
                let mut combined = Vec::with_capacity(rec_length);
                if let Some(f) = fix {
                    combined.extend(f.iter());
                }
                if let Some(s) = suggestions {
                    combined.extend(s.iter());
                }
                Some(combined)
            }
        };
        if all_recommendations.is_none() {
            return None;
        }
        let all_recommendations = all_recommendations.unwrap();

        let mut result = "### Recommendations\n\n".to_string();
        let line_number_width = num_digits(all_recommendations.len());
        all_recommendations
            .iter()
            .enumerate()
            .for_each(|(idx, rec)| {
                result += &format!(
                    "{:width$}. {}\n",
                    idx + 1,
                    self.get_recommendation_text(*rec),
                    width = line_number_width
                );
            });

        Some(result)
    }

    fn get_recommendation_text(&self, corr: &LintCorrection) -> String {
        match corr {
            LintCorrection::Insert(ins) => {
                format!(
                    "Insert the following text at row {}, column {}: {}",
                    ins.location.start.row + 1,
                    ins.location.start.column + 1,
                    ins.text
                )
            }
            LintCorrection::Delete(del) => {
                format!(
                    "Delete the text from row {}, column {} to row {}, column {}",
                    del.location.start.row + 1,
                    del.location.start.column + 1,
                    del.location.end.row + 1,
                    del.location.end.column + 1
                )
            }
            LintCorrection::Replace(rep) => {
                format!(
                    "Replace the text from row {}, column {} to row {}, column {} with {}",
                    rep.location.start.row + 1,
                    rep.location.start.column + 1,
                    rep.location.end.row + 1,
                    rep.location.end.column + 1,
                    rep.text
                )
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use bon::builder;
    use tempfile::TempDir;

    use super::*;
    use crate::{
        errors::{LintError, LintLevel},
        fix::{LintCorrectionDelete, LintCorrectionInsert, LintCorrectionReplace},
        geometry::DenormalizedLocation,
    };

    #[builder]
    fn format_mock_error(
        contents: &str,
        location: DenormalizedLocation,
        fix: Option<Vec<LintCorrection>>,
        sugg: Option<Vec<LintCorrection>>,
        #[builder(default = "test.md")] mock_path: &str,
        #[builder(default = LintLevel::Error)] level: LintLevel,
        #[builder(default = "MockRule")] rule_name: &str,
        #[builder(default = "This is an error")] error_message: &str,
    ) -> Result<String> {
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join(mock_path);
        fs::write(&file_path, &contents).unwrap();

        let error = LintError::from_raw_location()
            .rule(rule_name)
            .level(level)
            .message(error_message)
            .location(location)
            .maybe_fix(fix)
            .maybe_suggestions(sugg)
            .call();

        let file_path = file_path.to_string_lossy().to_string();
        let output = LintOutput {
            file_path: file_path.clone(),
            errors: vec![error],
        };
        let output = vec![output];

        let formatter = MarkdownFormatter;
        let mut result = Vec::new();
        formatter.format(&output, &mut result)?;
        String::from_utf8(result).map_err(|e| e.into())
    }

    #[test]
    fn test_markdown_formatter() {
        let contents = r#"# Hello World

What a wonderful world!"#;
        let location = DenormalizedLocation::dummy(8, 13, 0, 8, 0, 13);
        let output = format_mock_error()
            .contents(contents)
            .location(location)
            .call()
            .unwrap();

        assert!(output.starts_with("# supa-mdx-lint"));
        assert!(output.contains("1 | # Hello World"));
        assert!(output.contains("This is an error"));
    }

    #[test]
    fn test_markdown_formatter_replace() {
        let contents = r#"# Hello World

What a wonderful world!"#;
        let location = DenormalizedLocation::dummy(8, 13, 0, 8, 0, 13);
        let fix = vec![LintCorrection::Replace(LintCorrectionReplace {
            location: DenormalizedLocation::dummy(8, 13, 0, 8, 0, 13),
            text: "Friend".to_string(),
        })];
        let output = format_mock_error()
            .contents(contents)
            .location(location)
            .fix(fix)
            .call()
            .unwrap();

        assert!(output.starts_with("# supa-mdx-lint"));
        assert!(output.contains("1 | # Hello World"));
        assert!(output.contains("This is an error"));
        assert!(output.contains("Recommendations"));
        assert!(output.contains("Replace the text"));
        assert!(output.contains("Friend"));
    }

    #[test]
    fn test_markdown_formatter_insert() {
        let contents = r#"# Hello World

What a wonderful world!"#;
        let location = DenormalizedLocation::dummy(21, 21, 2, 6, 2, 6);
        let fix = vec![LintCorrection::Insert(LintCorrectionInsert {
            location: DenormalizedLocation::dummy(21, 21, 2, 6, 2, 6),
            text: " super".to_string(),
        })];
        let output = format_mock_error()
            .contents(contents)
            .location(location)
            .fix(fix)
            .call()
            .unwrap();

        assert!(output.starts_with("# supa-mdx-lint"));
        assert!(output.contains("3 | What a wonderful world!"));
        assert!(output.contains("This is an error"));
        assert!(output.contains("Recommendations"));
        assert!(output.contains("Insert the following text"));
        assert!(output.contains("super"));
    }

    #[test]
    fn test_markdown_formatter_delete() {
        let contents = r#"# Hello World

What a wonderful world!"#;
        let location = DenormalizedLocation::dummy(8, 13, 0, 8, 0, 13);
        let fix = vec![LintCorrection::Delete(LintCorrectionDelete {
            location: DenormalizedLocation::dummy(8, 13, 0, 8, 0, 13),
        })];
        let output = format_mock_error()
            .contents(contents)
            .location(location)
            .fix(fix)
            .call()
            .unwrap();

        assert!(output.starts_with("# supa-mdx-lint"));
        assert!(output.contains("1 | # Hello World"));
        assert!(output.contains("This is an error"));
        assert!(output.contains("Recommendations"));
        assert!(output.contains("Delete the text"));
        assert!(output.contains("row 1, column 9 to row 1, column 14"));
    }

    #[test]
    fn test_markdown_formatter_multiple_recommendations() {
        let contents = r#"# Hello World

What a wonderful world!"#;
        let location = DenormalizedLocation::dummy(8, 13, 0, 8, 0, 13);
        let fix = vec![LintCorrection::Replace(LintCorrectionReplace {
            location: DenormalizedLocation::dummy(8, 13, 0, 8, 0, 13),
            text: "Friend".to_string(),
        })];
        let suggestions = vec![
            LintCorrection::Replace(LintCorrectionReplace {
                location: DenormalizedLocation::dummy(8, 13, 0, 8, 0, 13),
                text: "Neighbor".to_string(),
            }),
            LintCorrection::Insert(LintCorrectionInsert {
                location: DenormalizedLocation::dummy(13, 13, 0, 13, 0, 13),
                text: " and Universe".to_string(),
            }),
        ];

        let output = format_mock_error()
            .contents(contents)
            .location(location)
            .fix(fix)
            .sugg(suggestions)
            .call()
            .unwrap();

        assert!(output.starts_with("# supa-mdx-lint"));
        assert!(output.contains("1 | # Hello World"));
        assert!(output.contains("This is an error"));
        assert!(output.contains("Recommendations"));
        assert!(output
            .contains("1. Replace the text from row 1, column 9 to row 1, column 14 with Friend"));
        assert!(output.contains(
            "2. Replace the text from row 1, column 9 to row 1, column 14 with Neighbor"
        ));
        assert!(output.contains("3. Insert the following text at row 1, column 14:  and Universe"));
    }

    #[test]
    fn test_markdown_formatter_multiple_errors() {
        let contents = r#"# Hello World

What a wonderful world!"#;
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("test.md");
        fs::write(&file_path, contents).unwrap();

        let output = LintOutput {
            file_path: file_path.to_string_lossy().to_string(),
            errors: vec![
                LintError::from_raw_location()
                    .rule("FirstRule")
                    .level(LintLevel::Error)
                    .message("First error message")
                    .location(DenormalizedLocation::dummy(8, 13, 0, 8, 0, 13))
                    .call(),
                LintError::from_raw_location()
                    .rule("SecondRule")
                    .level(LintLevel::Warning)
                    .message("Second error message")
                    .location(DenormalizedLocation::dummy(21, 30, 2, 6, 2, 15))
                    .call(),
            ],
        };

        let formatter = MarkdownFormatter;
        let mut result = Vec::new();
        formatter.format(&[output], &mut result).unwrap();
        let output_str = String::from_utf8(result).unwrap();

        assert!(output_str.starts_with("# supa-mdx-lint"));
        assert!(output_str.contains("1 | # Hello World"));
        assert!(output_str.contains("First error message"));
        assert!(output_str.contains("3 | What a wonderful world!"));
        assert!(output_str.contains("Second error message"));
    }

    #[test]
    fn test_markdown_formatter_multiple_files() {
        let temp_dir = TempDir::new().unwrap();

        // Create first file
        let file_path1 = temp_dir.path().join("file1.md");
        let contents1 = "# First File\nThis is the first file.";
        fs::write(&file_path1, contents1).unwrap();

        // Create second file
        let file_path2 = temp_dir.path().join("file2.md");
        let contents2 = "# Second File\nThis is the second file.";
        fs::write(&file_path2, contents2).unwrap();

        let output1 = LintOutput {
            file_path: file_path1.to_string_lossy().to_string(),
            errors: vec![LintError::from_raw_location()
                .rule("Rule1")
                .level(LintLevel::Error)
                .message("Error in first file")
                .location(DenormalizedLocation::dummy(0, 10, 0, 0, 0, 10))
                .call()],
        };

        let output2 = LintOutput {
            file_path: file_path2.to_string_lossy().to_string(),
            errors: vec![LintError::from_raw_location()
                .rule("Rule2")
                .level(LintLevel::Warning)
                .message("Warning in second file")
                .location(DenormalizedLocation::dummy(0, 12, 0, 0, 0, 12))
                .call()],
        };

        let formatter = MarkdownFormatter;
        let mut result = Vec::new();
        formatter.format(&[output1, output2], &mut result).unwrap();
        let output_str = String::from_utf8(result).unwrap();

        assert!(output_str.starts_with("# supa-mdx-lint"));

        // Check file1 content appears in output
        assert!(output_str.contains("file1.md"));
        assert!(output_str.contains("1 | # First File"));
        assert!(output_str.contains("Error in first file"));

        // Check file2 content appears in output
        assert!(output_str.contains("file2.md"));
        assert!(output_str.contains("1 | # Second File"));
        assert!(output_str.contains("Warning in second file"));
    }

    #[test]
    fn test_markdown_formatter_long_file() {
        // Create a long markdown file with 100 lines
        let mut contents = String::with_capacity(2000);
        for i in 1..=100 {
            contents.push_str(&format!("# Line {}\n", i));
        }

        // Place error somewhere in the middle
        let middle_line = 50;
        let start_pos = contents.find(&format!("# Line {}", middle_line)).unwrap();
        let end_pos = start_pos + 15; // Capture this line and part of the next
        let location =
            DenormalizedLocation::dummy(start_pos, end_pos, middle_line - 1, 0, middle_line, 4);

        let output = format_mock_error()
            .contents(&contents)
            .location(location)
            .error_message("Error in a long file")
            .call()
            .unwrap();

        // Verify the error is properly formatted
        assert!(output.starts_with("# supa-mdx-lint"));
        assert!(output.contains(&format!("{} | # Line {}", middle_line, middle_line)));
        assert!(output.contains("Error in a long file"));

        // Verify we don't have the entire file in the output
        assert!(!output.contains("# Line 1"));
        assert!(!output.contains("# Line 100"));

        // But we should have a reasonable context around the error
        assert!(output.contains(&format!("{} | # Line {}", middle_line + 1, middle_line + 1)));
        assert!(output.contains(&format!("{} | # Line {}", middle_line + 2, middle_line + 2)));
    }
}
