use std::{collections::HashSet, fs, ops::Range, path::PathBuf};

use anyhow::Result;
use bon::bon;
use dialoguer::{Confirm, Editor, Select};
use miette::{miette, LabeledSpan, NamedSource, Severity};
use owo_colors::OwoColorize;
use supa_mdx_lint::{
    errors::LintError,
    fix::LintCorrection,
    rope::{Rope, RopeSlice},
    utils::Offsets,
    LintTarget, Linter,
};

enum CorrectionStrategy {
    Fix,
    Skip,
}

#[derive(Debug, Clone, Eq, PartialEq, Hash)]
struct ErrorCacheKey(usize, usize, String);

impl From<&LintError> for ErrorCacheKey {
    fn from(error: &LintError) -> Self {
        Self(error.start(), error.end(), error.message().to_string())
    }
}

struct CachedFile {
    path: PathBuf,
    rope: Rope,
    content: String,
    has_diagnostics: bool,
    edited: bool,
    skipped: HashSet<ErrorCacheKey>,
}

impl CachedFile {
    fn load(path: impl Into<PathBuf>) -> Result<Self> {
        let path = path.into();
        let content = fs::read_to_string(&path)?;
        let rope = Rope::from(content.as_str());
        Ok(Self {
            path,
            rope,
            content,
            has_diagnostics: false,
            edited: false,
            skipped: HashSet::new(),
        })
    }

    fn sync_staged_contents(&mut self) {
        self.content = self.rope.to_string();
        self.edited = true;
    }

    fn commit_contents(&self) -> Result<()> {
        fs::write(&self.path, self.content.as_str())?;
        Ok(())
    }
}

pub struct InteractiveFixManager<'a, 'b> {
    linter: &'a Linter,
    targets: Vec<LintTarget<'b>>,
    curr_file: Option<CachedFile>,
}

#[bon]
impl<'a, 'b> InteractiveFixManager<'a, 'b> {
    pub fn new(linter: &'a Linter, targets: Vec<LintTarget<'b>>) -> Self {
        Self {
            linter,
            targets,
            curr_file: None,
        }
    }

    pub fn run(&mut self) -> Result<()> {
        for idx in 0..self.targets.len() {
            #[cfg(debug_assertions)]
            log::trace!("Linting target {idx}: {:?}", self.targets.get(idx));

            let target = self.targets.get(idx).unwrap();
            let LintTarget::FileOrDirectory(path) = target else {
                continue;
            };
            self.curr_file = Some(CachedFile::load(path)?);

            self.run_relint_loop()?;

            if !self.curr_file.as_ref().unwrap().has_diagnostics {
                continue;
            }

            let mut new_prompt = if self.curr_file.as_ref().unwrap().edited {
                match self.curr_file.as_mut().unwrap().commit_contents() {
                    Ok(_) => "💾  Changes successfully written to file".to_string(),
                    Err(err) => format!("🚨  Error writing changes to file: {err}").to_string(),
                }
            } else {
                "0️⃣  No edits made to current file".to_string()
            };

            self.curr_file = None;
            new_prompt.push_str("\n\n👉  Continue to next file?");

            match Confirm::new()
                .with_prompt(format!("\n\n{new_prompt}"))
                .report(false)
                .interact()?
            {
                true => continue,
                false => break,
            }
        }
        println!("\n\n🎉  Finished!");
        Ok(())
    }

    pub fn run_relint_loop(&mut self) -> Result<()> {
        'relint: loop {
            let diagnostics = self.linter.lint(&LintTarget::String(
                &self.curr_file.as_ref().unwrap().content.as_str(),
            ))?;
            match diagnostics.get(0) {
                Some(diagnostic) if !diagnostic.errors().is_empty() => {
                    self.curr_file.as_mut().unwrap().has_diagnostics = true;
                    for error in diagnostic.errors().iter() {
                        if self
                            .curr_file
                            .as_ref()
                            .unwrap()
                            .skipped
                            .contains(&error.into())
                        {
                            continue;
                        }

                        if let Some(CorrectionStrategy::Fix) =
                            self.prompt_error().error(error).call()?
                        {
                            continue 'relint;
                        }
                    }
                    return Ok(());
                }
                _ => {
                    return Ok(());
                }
            }
        }
    }

    #[builder]
    fn prompt_error(&mut self, error: &LintError) -> Result<Option<CorrectionStrategy>> {
        let pretty_error = self.pretty_error(
            error,
            &self
                .curr_file
                .as_ref()
                .unwrap()
                .path
                .to_string_lossy()
                .to_string(),
            self.curr_file.as_ref().unwrap().content.clone(),
        );

        let message = format!("\n\nError");
        let suggestions_heading = "Suggestions"
            .bold()
            .underline()
            .bright_magenta()
            .to_string();
        let suggestion_number =
            |i: usize| -> String { format!("Suggestion {}:", i + 1).bold().to_string() };

        let combined_suggestions = error.combined_suggestions();
        let suggestions = match combined_suggestions {
            Some(suggestions) if !suggestions.is_empty() => self.pretty_suggestions(
                &suggestions,
                self.curr_file.as_ref().unwrap().rope.byte_slice(..),
            ),
            _ => vec![],
        };

        let suggestions_string = suggestions
            .iter()
            .enumerate()
            .map(|(idx, (suggestion_string, _))| {
                format!("{} {}", suggestion_number(idx), suggestion_string)
            })
            .collect::<Vec<_>>()
            .join("\n\n");

        let custom_edit_prompt = Self::custom_edit_prompt(suggestions.len() + 1);
        let skip_for_now_prompt = Self::skip_for_now_prompt(suggestions.len() + 2);

        let selection = Select::new()
            .with_prompt(format!(
                "\n{}\n\n{:?}\n\n{}\n\n{}\n\n{}\n\n{}\n\n{}",
                message.bold().red().underline(),
                pretty_error,
                suggestions_heading,
                suggestions_string,
                custom_edit_prompt,
                skip_for_now_prompt,
                "Choose an option"
            ))
            .items(
                &(0..suggestions.len() + 2)
                    .map(|i| format!("Suggestion {}", i + 1))
                    .collect::<Vec<_>>(),
            )
            .interact()?;

        match selection {
            n if n == suggestions.len() + 1 => {
                self.curr_file
                    .as_mut()
                    .unwrap()
                    .skipped
                    .insert(error.into());
                Ok(Some(CorrectionStrategy::Skip))
            }
            n if n == suggestions.len() => {
                self.custom_edit(error)?;
                Ok(Some(CorrectionStrategy::Fix))
            }
            n => {
                let suggestion = suggestions.get(n).unwrap();
                self.apply_suggestion(suggestion.1);
                Ok(Some(CorrectionStrategy::Fix))
            }
        }
    }

    fn pretty_error(
        &self,
        error: &LintError,
        file_name: &str,
        content: String,
    ) -> impl std::fmt::Debug {
        let severity: Severity = error.level().into();
        let message = error.message();

        miette!(
            severity = severity,
            labels = vec![LabeledSpan::at(error.offset_range(), "here")],
            "{}",
            message
        )
        .with_source_code(NamedSource::new(file_name, content))
    }

    fn pretty_suggestions(
        &self,
        suggestions: &[&'a LintCorrection],
        rope: RopeSlice<'_>,
    ) -> Vec<(String, &'a LintCorrection)> {
        suggestions
            .into_iter()
            .map(|suggestion| (self.format_suggestion(suggestion, rope), *suggestion))
            .collect::<Vec<_>>()
    }

    fn custom_edit_prompt(number: usize) -> String {
        let mut result = format!("Suggestion {number}: ").bold().to_string();
        result.push_str("✍️  Make a custom edit");
        result
    }

    fn skip_for_now_prompt(number: usize) -> String {
        let mut result = format!("Suggestion {number}: ").bold().to_string();
        result.push_str("⏩ Skip for now");
        result
    }

    fn format_suggestion(&self, suggestion: &LintCorrection, rope: RopeSlice<'_>) -> String {
        match suggestion {
            LintCorrection::Insert(insert) => {
                let line_offset_range = Self::bytes_from_offsets(insert, rope);

                let mut result = "➕  Insert text before marked character\n\n".to_string();
                result.push_str(&Self::mark_position_string(line_offset_range, insert, rope));
                result
            }
            LintCorrection::Delete(delete) => {
                let line_offset_range = Self::bytes_from_offsets(delete, rope);

                let mut result = "✂️  Delete underlined text\n\n".to_string();
                result.push_str(&Self::mark_position_string(line_offset_range, delete, rope));
                result
            }
            LintCorrection::Replace(replace) => {
                let line_offset_range = Self::bytes_from_offsets(replace, rope);

                let mut result = format!(
                    "🔄  Replace underlined text with \"{}\"\n\n",
                    replace.text()
                );
                result.push_str(&Self::mark_position_string(
                    line_offset_range,
                    replace,
                    rope,
                ));
                result
            }
        }
    }

    fn apply_suggestion(&mut self, suggestion: &LintCorrection) {
        let rope = &mut self.curr_file.as_mut().unwrap().rope;

        match suggestion {
            LintCorrection::Insert(insert) => rope.insert(insert.start(), insert.text()),
            LintCorrection::Delete(delete) => rope.delete(delete.start()..delete.end()),
            LintCorrection::Replace(replace) => {
                rope.replace(replace.start()..replace.end(), replace.text())
            }
        }

        self.curr_file.as_mut().unwrap().sync_staged_contents();
    }

    fn custom_edit(&mut self, error: &LintError) -> Result<()> {
        let rope = &mut self.curr_file.as_mut().unwrap().rope;
        let edit_range = Self::bytes_from_offsets(error, rope.byte_slice(..));

        let Some(revised_content) =
            Editor::new().edit(&rope.byte_slice(edit_range.clone()).to_string())?
        else {
            println!("Editing canceled");
            return Ok(());
        };

        rope.replace(edit_range, &revised_content);
        self.curr_file.as_mut().unwrap().sync_staged_contents();
        Ok(())
    }

    fn bytes_from_offsets(offsets: impl Offsets, rope: RopeSlice<'_>) -> Range<usize> {
        let start_line_byte = rope.byte_of_line(rope.line_of_byte(offsets.start()));
        let end_line_byte = {
            let line = rope.line_of_byte(offsets.end());
            if line == rope.line_len() - 1 {
                rope.byte_len()
            } else {
                rope.byte_of_line(line + 1)
            }
        };
        Range {
            start: start_line_byte,
            end: end_line_byte,
        }
    }

    fn mark_position_string(
        display_range: Range<usize>,
        marked_range: impl Offsets,
        rope: RopeSlice<'_>,
    ) -> String {
        // Build in a little extra for formatting markers
        let mut result = String::with_capacity(display_range.end - display_range.start + 15);

        for char in rope
            .byte_slice(display_range.start..marked_range.start())
            .chars()
        {
            result.push(char);
        }

        let marked = rope
            .byte_slice(marked_range.start()..marked_range.end())
            .to_string();
        result.push_str(&format!("{}", marked.underline()));

        for char in rope
            .byte_slice(marked_range.end()..display_range.end)
            .chars()
        {
            result.push(char);
        }

        result
    }
}
