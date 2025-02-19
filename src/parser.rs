use std::{any::Any, collections::HashMap, error::Error, fmt::Display};

use anyhow::{anyhow, Result};
use itertools::Itertools;
use log::{debug, trace, warn};
use markdown::{mdast::Node, to_mdast, unist, Constructs, ParseOptions};
use regex::Regex;

use crate::{
    geometry::{
        AdjustedOffset, AdjustedPoint, AdjustedRange, DenormalizedLocation, MaybeEndedLineRange,
    },
    rope::Rope,
    rules::RuleContext,
};

type Frontmatter = Box<dyn Any>;

#[derive(Debug)]
pub(crate) struct ParseResult {
    pub ast: Node,
    pub rope: Rope,
    pub content_start_offset: AdjustedOffset,
    #[allow(unused)]
    pub frontmatter: Option<Frontmatter>,
}

pub(crate) fn parse(input: &str) -> Result<ParseResult> {
    let (content, rope, content_start_offset, frontmatter) = process_raw_content_string(input);
    let ast = parse_internal(content)?;

    trace!("AST: {:#?}", ast);

    Ok(ParseResult {
        ast,
        rope,
        content_start_offset,
        frontmatter,
    })
}

fn process_raw_content_string(input: &str) -> (&str, Rope, AdjustedOffset, Option<Frontmatter>) {
    let rope = Rope::from(input);
    let mut frontmatter = None;
    let mut content = input;

    let mut content_start_offset = AdjustedOffset::default();

    if content.trim_start().starts_with("---") {
        let frontmatter_start_offset: AdjustedOffset = (content.find("---").unwrap() + 3).into();

        if let Some(frontmatter_end_index) = content[frontmatter_start_offset.into()..].find("---")
        {
            let mut end_offset: AdjustedOffset =
                (Into::<usize>::into(frontmatter_start_offset) + frontmatter_end_index).into();

            let frontmatter_str = &content[frontmatter_start_offset.into()..end_offset.into()];

            if let Ok(toml_frontmatter) = toml::from_str::<toml::Value>(frontmatter_str) {
                debug!("Parsed as TOML: {toml_frontmatter:#?}");
                frontmatter = Some(Box::new(toml_frontmatter) as Frontmatter);
            } else if let Ok(yaml_frontmatter) =
                serde_yaml::from_str::<serde_yaml::Value>(frontmatter_str)
            {
                debug!("Parsed as YAML: {yaml_frontmatter:#?}");
                frontmatter = Some(Box::new(yaml_frontmatter) as Frontmatter);
            } else {
                debug!("Failed to parse frontmatter as TOML or YAML: {frontmatter_str}")
            }

            // Update end_offset to include the closing "---" and following blank lines

            // Move past the closing "---"
            end_offset.increment(3);

            // Skip all whitespace and newlines after the closing "---"
            let mut remaining_index = 0;
            let remaining = &content[end_offset.into()..];
            while remaining_index < remaining.len() {
                if remaining[remaining_index..].starts_with(char::is_whitespace) {
                    remaining_index += 1;
                } else {
                    break;
                }
            }
            end_offset.increment(remaining_index);

            content_start_offset = end_offset;
        }
    }

    content = &input[content_start_offset.into()..];

    (content, rope, content_start_offset, frontmatter)
}

fn parse_internal(input: &str) -> Result<Node> {
    let mdast = to_mdast(
        input,
        &ParseOptions {
            constructs: Constructs {
                autolink: false,
                code_indented: false,
                frontmatter: true,
                gfm_footnote_definition: true,
                gfm_label_start_footnote: true,
                gfm_table: true,
                html_flow: false,
                html_text: false,
                mdx_esm: true,
                mdx_expression_flow: true,
                mdx_expression_text: true,
                mdx_jsx_flow: true,
                mdx_jsx_text: true,
                ..Default::default()
            },
            ..Default::default()
        },
    )
    .map_err(|e| anyhow!("Not valid Markdown: {:?}", e))?;

    Ok(mdast)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_markdown_without_frontmatter() {
        let input = r#"# Heading

Content here."#;
        let result = parse(input).unwrap();

        assert_eq!(result.content_start_offset, AdjustedOffset::from(0));
        assert!(result.frontmatter.is_none());

        let root = result.ast;
        let heading = root.children().unwrap().first().unwrap();
        assert_eq!(heading.position().unwrap().start.line, 1);
        assert_eq!(heading.position().unwrap().start.column, 1);
        assert_eq!(heading.position().unwrap().start.offset, 0);
    }

    #[test]
    fn test_parse_markdown_with_yaml_frontmatter() {
        let input = r#"---
title: Test
---

# Heading

Content here."#;
        let result = parse(input).unwrap();

        assert_eq!(result.content_start_offset, AdjustedOffset::from(21));
        assert!(result.frontmatter.is_some());

        let frontmatter = result.frontmatter.unwrap();
        let yaml = frontmatter.downcast_ref::<serde_yaml::Value>().unwrap();
        if let serde_yaml::Value::Mapping(map) = yaml {
            assert_eq!(map.len(), 1);
            assert!(map.contains_key(&serde_yaml::Value::String("title".to_string())));
        } else {
            panic!("Expected YAML frontmatter to be a mapping");
        }

        let root = result.ast;
        let heading = root.children().unwrap().first().unwrap();
        assert_eq!(heading.position().unwrap().start.line, 1);
        assert_eq!(heading.position().unwrap().start.column, 1);
    }

    #[test]
    fn test_parse_markdown_with_toml_frontmatter() {
        let input = r#"---
title = "TOML Test"
[author]
name = "John Doe"
---

# TOML Heading

Content with TOML frontmatter."#;
        let result = parse(input).unwrap();

        assert_eq!(result.content_start_offset, AdjustedOffset::from(56));
        assert!(result.frontmatter.is_some());

        let frontmatter = result.frontmatter.unwrap();
        let toml = frontmatter.downcast_ref::<toml::Value>().unwrap();

        assert!(toml.is_table());
        let table = toml.as_table().unwrap();

        assert!(table.contains_key("title"));

        let root = result.ast;
        let heading = root.children().unwrap().first().unwrap();
        assert_eq!(heading.position().unwrap().start.line, 1);
        assert_eq!(heading.position().unwrap().start.column, 1);
    }

    #[test]
    fn test_parse_markdown_with_frontmatter_and_multiple_newlines() {
        let input = r#"---
title: Test
---


# Heading

Content here."#;
        let result = parse(input).unwrap();
        assert_eq!(result.content_start_offset, AdjustedOffset::from(22));
        assert!(result.frontmatter.is_some());

        let root = result.ast;
        let heading = root.children().unwrap().first().unwrap();
        assert_eq!(heading.position().unwrap().start.line, 1);
        assert_eq!(heading.position().unwrap().start.column, 1);
    }
}

#[derive(Debug, PartialEq, Eq)]
enum RuleToggle {
    EnableAll,
    EnableRule { rule: String },
    DisableAll { next_line_only: bool },
    DisableRule { rule: String, next_line_only: bool },
}

#[derive(Debug, Default)]
pub struct LintDisables(HashMap<String, Vec<MaybeEndedLineRange>>);

impl LintDisables {
    /// Returns the marker used to indicate that all rules should be disabled.
    fn get_all_marker() -> &'static str {
        "__priv__ALL__"
    }

    /// Collects all disable statements in the AST and returns a map of rule names to their
    /// corresponding disables.
    fn collect_lint_disables(
        ast: &Node,
        context: &RuleContext,
    ) -> Result<HashMap<String, Vec<MaybeEndedLineRange>>, DisableParseError> {
        let mut disables = HashMap::<String, Vec<MaybeEndedLineRange>>::new();

        fn collect_lint_disables_internal(
            ast: &Node,
            next_node: Option<&Node>,
            context: &RuleContext,
            disables: &mut HashMap<String, Vec<MaybeEndedLineRange>>,
            all_marker: &str,
        ) -> std::result::Result<(), DisableParseError> {
            fn reenable_last(
                previous: Option<&mut Vec<MaybeEndedLineRange>>,
                current_position: &unist::Position,
                rule: Option<&str>,
                context: &RuleContext,
            ) -> Result<(), DisableParseError> {
                let last_disable = previous.and_then(|previous| previous.last_mut());
                match last_disable {
                    Some(disabled_range) if disabled_range.is_open_ended() => {
                        let end_offset =
                            AdjustedRange::from_unadjusted_position(current_position, context).end;
                        let end_line =
                            AdjustedPoint::from_adjusted_offset(&end_offset, context.rope()).row;
                        disabled_range.end = Some(end_line);
                        Ok(())
                    }
                    _ => {
                        let adjusted_start =
                            AdjustedRange::from_unadjusted_position(current_position, context)
                                .start;
                        let start_point =
                            AdjustedPoint::from_adjusted_offset(&adjusted_start, context.rope());

                        let subject_copula = if let Some(rule) = rule {
                            format!("Rule {} was", rule)
                        } else {
                            "Rules were".to_string()
                        };

                        Err(DisableParseError::new(format!(
                            "{subject_copula} enabled without a preceding disable. [{}:{}]",
                            start_point.row + 1,
                            start_point.column + 1
                        )))
                    }
                }
            }

            fn disable(
                previous: &mut Vec<MaybeEndedLineRange>,
                current_position: &unist::Position,
                next_node: Option<&Node>,
                rule: Option<&str>,
                next_line_only: bool,
                context: &RuleContext,
            ) {
                let last_disable = previous.last();
                match last_disable {
                    Some(disabled_range) if disabled_range.is_open_ended() => {
                        let subject_copula = if let Some(rule) = rule {
                            format!("Rule {} was", rule)
                        } else {
                            "Rules were".to_string()
                        };
                        warn!("{subject_copula} disabled twice in succession. This might indicate that the first disable was not reversed as intended.");
                    }
                    _ => {}
                }

                let start_offset =
                    AdjustedRange::from_unadjusted_position(current_position, context).start;
                let start_line =
                    AdjustedPoint::from_adjusted_offset(&start_offset, context.rope()).row;

                let end_line = if next_line_only {
                    match next_node.and_then(|node| node.position()) {
                        Some(next_position) => {
                            let next_start_offset =
                                AdjustedRange::from_unadjusted_position(next_position, context)
                                    .start;
                            let next_start_line = AdjustedPoint::from_adjusted_offset(
                                &next_start_offset,
                                context.rope(),
                            )
                            .row;
                            if next_start_line > start_line {
                                Some(next_start_line + 1)
                            } else {
                                Some(start_line + 2)
                            }
                        }
                        None => Some(start_line + 2),
                    }
                } else {
                    None
                };

                previous.push(MaybeEndedLineRange::new(start_line, end_line));
            }

            match ast {
                Node::MdxFlowExpression(expression) => {
                    let Some(current_position) = expression.position.as_ref() else {
                        return Err(DisableParseError::new("Could not toggle a rule because the underlying node is missing a position."));
                    };

                    if let Some(rule) = RuleToggle::parse(&expression.value) {
                        match rule {
                            RuleToggle::EnableAll => {
                                return reenable_last(
                                    disables.get_mut(all_marker),
                                    current_position,
                                    None,
                                    context,
                                )
                            }
                            RuleToggle::EnableRule { rule } => {
                                return reenable_last(
                                    disables.get_mut(&rule),
                                    current_position,
                                    Some(&rule),
                                    context,
                                );
                            }
                            RuleToggle::DisableAll { next_line_only } => {
                                let all_disables =
                                    disables.entry(all_marker.to_string()).or_default();
                                disable(
                                    all_disables,
                                    current_position,
                                    next_node,
                                    None,
                                    next_line_only,
                                    context,
                                );
                                return Ok(());
                            }
                            RuleToggle::DisableRule {
                                rule: ref rule_name,
                                next_line_only,
                            } => {
                                let disables_for_rule =
                                    disables.entry(rule_name.clone()).or_default();
                                disable(
                                    disables_for_rule,
                                    current_position,
                                    next_node,
                                    Some(rule_name),
                                    next_line_only,
                                    context,
                                );
                                return Ok(());
                            }
                        }
                    }

                    Ok(())
                }
                _ => {
                    if let Some(children) = ast.children() {
                        for node_pair in children.iter().zip_longest(children.iter().skip(1)) {
                            let child = node_pair.clone().left().unwrap();
                            let next = node_pair.right();
                            collect_lint_disables_internal(
                                child, next, context, disables, all_marker,
                            )?;
                        }
                    }

                    Ok(())
                }
            }
        }

        collect_lint_disables_internal(
            ast,
            None,
            context,
            &mut disables,
            LintDisables::get_all_marker(),
        )?;

        for (_, value) in disables.iter_mut() {
            value.sort_by_key(|k| k.start);
        }

        Ok(disables)
    }

    pub fn is_rule_disabled_for_location(
        &self,
        rule: &str,
        location: &DenormalizedLocation,
        context: &RuleContext,
    ) -> bool {
        debug!(
            "Checking if rule {} is disabled for location: {:#?}",
            rule, location
        );

        let overlapping_all_rules = self.0.get(LintDisables::get_all_marker()).map(|all_rules| {
            all_rules
                .iter()
                .any(|rule| rule.overlaps_lines(&location.offset_range, context.rope()))
        });
        debug!("Overlapping all disables: {:#?}", overlapping_all_rules);
        if let Some(true) = overlapping_all_rules {
            return true;
        }

        let overlapping_specific_rules = self.0.get(rule).map(|specific_rules| {
            specific_rules
                .iter()
                .any(|rule| rule.overlaps_lines(&location.offset_range, context.rope()))
        });
        debug!(
            "Overlapping specific disables: {:#?}",
            overlapping_specific_rules
        );
        matches!(overlapping_specific_rules, Some(true))
    }

    pub fn new(node: &Node, context: &RuleContext) -> Result<Self, DisableParseError> {
        let disables = LintDisables::collect_lint_disables(node, context)?;
        Ok(Self(disables))
    }
}

impl RuleToggle {
    pub fn parse(value: &str) -> Option<Self> {
        let value = value.trim();
        if !value.starts_with("/*") || !value.ends_with("*/") {
            return None;
        }
        let value = value.trim_start_matches("/*").trim_end_matches("*/").trim();

        let regex =
            Regex::new(r"^supa-mdx-lint-(enable|disable|disable-next-line)(?:\s+(.+))?$").unwrap();
        if let Some(captures) = regex.captures(value) {
            match captures.get(1) {
                Some(action) => match action.as_str() {
                    "enable" => {
                        if let Some(rule_name) = captures.get(2) {
                            return Some(RuleToggle::EnableRule {
                                rule: rule_name.as_str().to_string(),
                            });
                        } else {
                            return Some(RuleToggle::EnableAll);
                        }
                    }
                    "disable" => {
                        if let Some(rule_name) = captures.get(2) {
                            return Some(RuleToggle::DisableRule {
                                next_line_only: false,
                                rule: rule_name.as_str().to_string(),
                            });
                        } else {
                            return Some(RuleToggle::DisableAll {
                                next_line_only: false,
                            });
                        }
                    }
                    "disable-next-line" => {
                        if let Some(rule_name) = captures.get(2) {
                            return Some(RuleToggle::DisableRule {
                                next_line_only: true,
                                rule: rule_name.as_str().to_string(),
                            });
                        } else {
                            return Some(RuleToggle::DisableAll {
                                next_line_only: true,
                            });
                        }
                    }
                    _ => return None,
                },
                None => return None,
            };
        }

        None
    }
}

#[derive(Debug)]
pub struct DisableParseError(String);

impl DisableParseError {
    fn new(message: impl Into<String>) -> Self {
        Self(message.into())
    }
}

impl Display for DisableParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Error parsing disable statements in file: {}", self.0)
    }
}

impl Error for DisableParseError {}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn rule_toggle_parse_enable_all() {
        let value = "/* supa-mdx-lint-enable */";
        assert!(matches!(
            RuleToggle::parse(value),
            Some(RuleToggle::EnableAll)
        ));
    }

    #[test]
    fn rule_toggle_parse_enable_specific_rule() {
        let value = "/* supa-mdx-lint-enable specific-rule */";
        assert!(matches!(
            RuleToggle::parse(value),
            Some(RuleToggle::EnableRule { rule }) if rule == "specific-rule"
        ));
    }

    #[test]
    fn rule_toggle_parse_disable_all() {
        let value = "/* supa-mdx-lint-disable */";
        assert!(matches!(
            RuleToggle::parse(value),
            Some(RuleToggle::DisableAll { next_line_only }) if !next_line_only
        ));
    }

    #[test]
    fn rule_toggle_parse_disable_specific_rule() {
        let value = "/* supa-mdx-lint-disable specific-rule */";
        assert!(matches!(
            RuleToggle::parse(value),
            Some(RuleToggle::DisableRule { rule, next_line_only })
            if rule == "specific-rule" && !next_line_only
        ));
    }

    #[test]
    fn rule_toggle_parse_disable_next_line_all() {
        let value = "/* supa-mdx-lint-disable-next-line */";
        assert!(matches!(
            RuleToggle::parse(value),
            Some(RuleToggle::DisableAll { next_line_only }) if next_line_only
        ));
    }

    #[test]
    fn rule_toggle_parse_disable_next_line_specific_rule() {
        let value = "/* supa-mdx-lint-disable-next-line specific-rule */";
        assert!(matches!(
            RuleToggle::parse(value),
            Some(RuleToggle::DisableRule { rule, next_line_only })
            if rule == "specific-rule" && next_line_only
        ));
    }

    #[test]
    fn rule_toggle_parse_invalid_format() {
        let value = "supa-mdx-lint-enable";
        assert!(RuleToggle::parse(value).is_none());
    }

    #[test]
    fn rule_toggle_parse_invalid_command() {
        let value = "/* supa-mdx-lint-invalid */";
        assert!(RuleToggle::parse(value).is_none());
    }

    #[test]
    fn rule_toggle_parse_ignores_whitespace() {
        let value = "     /*     supa-mdx-lint-enable  rule-name  */";
        assert!(matches!(
            RuleToggle::parse(value),
            Some(RuleToggle::EnableRule { rule }) if rule == "rule-name"
        ));
    }

    #[test]
    fn test_collect_lint_disables_basic() {
        let input = r#"{/* supa-mdx-lint-disable foo */}
Some content
{/* supa-mdx-lint-enable foo */}"#;

        let parse_result = parse(input).unwrap();
        let context = RuleContext::new_parse_only_for_testing(parse_result);
        let disables = LintDisables::new(context.ast(), &context).unwrap();
        debug!("Disables: {:?}", disables);

        assert_eq!(disables.0.len(), 1);
        assert_eq!(disables.0["foo"][0].start, 0);
        assert_eq!(disables.0["foo"][0].end, Some(2));
    }

    #[test]
    fn test_collect_lint_disables_multiple_rules() {
        let input = r#"{/* supa-mdx-lint-disable foo */}
Content
{/* supa-mdx-lint-disable bar */}
More content
{/* supa-mdx-lint-enable foo */}
{/* supa-mdx-lint-enable bar */}"#;

        let parse_result = parse(input).unwrap();
        let context = RuleContext::new_parse_only_for_testing(parse_result);
        let disables = LintDisables::new(context.ast(), &context).unwrap();
        debug!("Disables: {:?}", disables);

        assert_eq!(disables.0.len(), 2);
        assert_eq!(disables.0["bar"][0].start, 2);
        assert_eq!(disables.0["bar"][0].end, Some(5));
    }

    #[test]
    fn test_collect_lint_disables_next_line() {
        let input = r#"{/* supa-mdx-lint-disable-next-line foo */}
This line is ignored
This line is not ignored"#;

        let parse_result = parse(input).unwrap();
        let context = RuleContext::new_parse_only_for_testing(parse_result);
        let disables = LintDisables::new(context.ast(), &context).unwrap();
        debug!("Disables: {:?}", disables);

        assert_eq!(disables.0.len(), 1);
        assert_eq!(disables.0["foo"][0].end, Some(2));
    }

    #[test]
    fn test_collect_lint_disables_disable_all() {
        let input = r#"{/* supa-mdx-lint-disable */}
Everything here is ignored
Still ignored
{/* supa-mdx-lint-enable */}"#;

        let parse_result = parse(input).unwrap();
        let context = RuleContext::new_parse_only_for_testing(parse_result);
        let disables = LintDisables::new(context.ast(), &context).unwrap();
        debug!("Disables: {:?}", disables);

        assert_eq!(disables.0.len(), 1);
        assert_eq!(disables.0[LintDisables::get_all_marker()][0].start, 0);
        assert_eq!(disables.0[LintDisables::get_all_marker()][0].end, Some(3));
    }

    #[test]
    fn test_collect_lint_never_reenabled() {
        let input = r#"{/* supa-mdx-lint-disable foo */}
Never reenabled"#;

        let parse_result = parse(input).unwrap();
        let context = RuleContext::new_parse_only_for_testing(parse_result);
        let disables = LintDisables::new(context.ast(), &context).unwrap();
        debug!("Disables: {:?}", disables);

        assert_eq!(disables.0.len(), 1);
        assert!(disables.0["foo"][0].end.is_none());
    }

    #[test]
    fn test_collect_lint_disables_invalid_enable() {
        let input = r#"{/* supa-mdx-lint-enable foo */}
This should error because there was no disable"#;

        let parse_result = parse(input).unwrap();
        let context = RuleContext::new_parse_only_for_testing(parse_result);
        assert!(LintDisables::new(context.ast(), &context).is_err());
    }

    #[test]
    fn test_collect_lint_disables_skip_blank_lines() {
        let input = r#"{/* supa-mdx-lint-disable-next-line foo */}

This line is ignored
This line is not ignored"#;

        let parse_result = parse(input).unwrap();
        let context = RuleContext::new_parse_only_for_testing(parse_result);
        let disables = LintDisables::new(context.ast(), &context).unwrap();
        debug!("Disables: {:?}", disables);

        assert_eq!(disables.0.len(), 1);
        assert_eq!(disables.0["foo"][0].end, Some(3));
    }
}
