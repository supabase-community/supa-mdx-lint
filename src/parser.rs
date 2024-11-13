use anyhow::{anyhow, Result};
use log::{debug, warn};
use markdown::{mdast::Node, to_mdast, Constructs, ParseOptions};
use regex::Regex;
use std::{any::Any, collections::HashMap, error::Error, fmt::Display, num::NonZeroUsize};

use crate::{
    document::{AdjustedPoint, Location},
    rules::RuleContext,
    utils::NonZeroLineRange,
};

type Frontmatter = Box<dyn Any>;

#[derive(Debug)]
pub struct ParseResult {
    pub ast: Node,
    pub frontmatter_lines: usize,
    pub frontmatter: Option<Frontmatter>,
}

pub fn parse(input: &str) -> Result<ParseResult> {
    let (frontmatter_lines, frontmatter, content) = extract_frontmatter(input);
    let ast = parse_internal(content)?;
    Ok(ParseResult {
        ast,
        frontmatter_lines,
        frontmatter,
    })
}

fn extract_frontmatter(input: &str) -> (usize, Option<Frontmatter>, &str) {
    let mut frontmatter = None;
    let mut content = input;

    let mut frontmatter_end = AdjustedPoint::default();

    if content.trim_start().starts_with("---") {
        let start_offset = content.find("---").unwrap() + 3;

        if let Some(end_offset) = content[start_offset..].find("---") {
            let mut end_offset = start_offset + end_offset;
            let frontmatter_str = content[start_offset..end_offset].to_string();

            if let Ok(toml_frontmatter) = toml::from_str::<toml::Value>(&frontmatter_str) {
                debug!("Parsed as TOML: {toml_frontmatter:?}");
                frontmatter = Some(Box::new(toml_frontmatter) as Frontmatter);
            } else if let Ok(yaml_frontmatter) =
                serde_yaml::from_str::<serde_yaml::Value>(&frontmatter_str)
            {
                debug!("Parsed as YAML: {yaml_frontmatter:?}");
                frontmatter = Some(Box::new(yaml_frontmatter) as Frontmatter);
            } else {
                debug!("Failed to parse frontmatter as TOML or YAML")
            }

            // If both parse attempts fail, frontmatter remains None

            // Update end_offset to include the closing "---" and following blank lines
            end_offset += 3; // Move past the closing "---"
            let remaining = &content[end_offset..];
            let mut newline_offset = 0;

            // Skip all whitespace and newlines after the closing "---"
            while newline_offset < remaining.len() {
                if remaining[newline_offset..].starts_with(|x: char| x == '\n' || x.is_whitespace())
                {
                    newline_offset += 1;
                } else {
                    break;
                }
            }

            end_offset += newline_offset;

            frontmatter_end =
                AdjustedPoint::new(content[..end_offset].lines().count() + 1, 1, end_offset);
        }
    }

    if frontmatter.is_some() {
        content = &input[frontmatter_end.offset..];
    }

    let frontmatter_lines: usize = if frontmatter.is_some() {
        frontmatter_end.line.get() - 1
    } else {
        0
    };

    (frontmatter_lines, frontmatter, content)
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
    .map_err(|e| anyhow!("Markdown parsing error: {:?}", e))?;

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

        assert_eq!(result.frontmatter_lines, 0);
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

        assert_eq!(result.frontmatter_lines, 4);
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

        assert_eq!(result.frontmatter_lines, 6);
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
        assert_eq!(result.frontmatter_lines, 5);
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

#[derive(Debug)]
struct LintDisable {
    line_range: NonAdjustedLineRange,
}

#[derive(Debug)]
struct NonAdjustedLineRange {
    start: NonZeroUsize,
    end: Option<NonZeroUsize>,
}

#[derive(Debug)]
struct AdjustedLineRange {
    start: NonZeroUsize,
    end: Option<NonZeroUsize>,
}

impl NonZeroLineRange for AdjustedLineRange {
    fn start_line(&self) -> NonZeroUsize {
        self.start
    }

    fn end_line(&self) -> Option<NonZeroUsize> {
        self.end
    }
}

impl AdjustedLineRange {
    fn from_unadjusted_line_range(range: &NonAdjustedLineRange, context: &RuleContext) -> Self {
        Self {
            start: range
                .start
                .checked_add(context.frontmatter_lines())
                .expect("Frontmatter lines should be non-negative, unlikely to overflow"),
            end: range.end.map(|end| {
                end.checked_add(context.frontmatter_lines())
                    .expect("Frontmatter lines should be non-negative, unlikely to overflow")
            }),
        }
    }
}

#[derive(Debug, Default)]
pub struct LintDisables(HashMap<String, Vec<LintDisable>>);

impl LintDisables {
    /// Returns the marker used to indicate that all rules should be disabled.
    fn all_marker() -> &'static str {
        "__priv__ALL__"
    }

    /// Collects all disable statements in the AST and returns a map of rule names to their
    /// corresponding disables.
    fn collect_lint_disables(
        ast: &Node,
    ) -> Result<HashMap<String, Vec<LintDisable>>, DisableParseError> {
        let mut disables = HashMap::<String, Vec<LintDisable>>::new();

        fn collect_lint_disables_internal(
            ast: &Node,
            disables: &mut HashMap<String, Vec<LintDisable>>,
            #[allow(non_snake_case)] ALL_MARKER: &str,
        ) -> std::result::Result<(), DisableParseError> {
            match ast {
                Node::MdxFlowExpression(expression) => {
                    if let Some(rule) = RuleToggle::parse(&expression.value) {
                        match rule {
                            RuleToggle::EnableAll => {
                                let all_disables = disables.get_mut(ALL_MARKER);
                                let last_all_disable = all_disables.and_then(|v| v.last_mut());
                                if last_all_disable.is_some()
                                    && last_all_disable.as_ref().unwrap().line_range.end.is_none()
                                {
                                    let end_line = expression.position.as_ref().map(|p| p.end.line);
                                    last_all_disable.unwrap().line_range.end =
                                        end_line.map(|line| NonZeroUsize::new(line).unwrap());
                                    return Ok(());
                                } else {
                                    return Err(DisableParseError::new(format!(
                                        "Rules were enabled without a preceding disable. [{}:{}]",
                                        expression
                                            .position
                                            .as_ref()
                                            .map(|p| p.start.line)
                                            .unwrap_or(0),
                                        expression
                                            .position
                                            .as_ref()
                                            .map(|p| p.start.column)
                                            .unwrap_or(0)
                                    )));
                                }
                            }
                            RuleToggle::EnableRule { rule } => {
                                let disables_for_rule = disables.get_mut(&rule);
                                let last_disable = disables_for_rule.and_then(|v| v.last_mut());
                                if last_disable.is_some()
                                    && last_disable.as_ref().unwrap().line_range.end.is_none()
                                {
                                    let end_line = expression.position.as_ref().map(|p| p.end.line);
                                    last_disable.unwrap().line_range.end =
                                        end_line.map(|line| NonZeroUsize::new(line).unwrap());
                                    return Ok(());
                                } else {
                                    return Err(DisableParseError::new(format!(
                                        "Rule {} was enabled without a preceding disable. [{}:{}]",
                                        rule,
                                        expression
                                            .position
                                            .as_ref()
                                            .map(|p| p.start.line)
                                            .unwrap_or(0),
                                        expression
                                            .position
                                            .as_ref()
                                            .map(|p| p.start.column)
                                            .unwrap_or(0)
                                    )));
                                }
                            }
                            RuleToggle::DisableAll { next_line_only } => {
                                let all_disables =
                                    disables.entry(ALL_MARKER.to_string()).or_default();
                                let last_disable = all_disables.last();
                                if last_disable.is_some()
                                    && last_disable.unwrap().line_range.end.is_none()
                                {
                                    warn!("All rules were disabled twice in succession. This might indicate that the first disable was not reversed as intended.");
                                }

                                let start_line =
                                    expression.position.as_ref().map(|pos| pos.start.line);
                                if start_line.is_none() {
                                    return Err(DisableParseError::new("Could not disable all rules because underlying node is missing line number."));
                                };
                                let end_line = if next_line_only {
                                    Some(start_line.unwrap() + 2)
                                } else {
                                    None
                                };

                                all_disables.push(LintDisable {
                                    line_range: NonAdjustedLineRange {
                                        start: NonZeroUsize::new(start_line.unwrap()).unwrap(),
                                        end: end_line.map(|line| NonZeroUsize::new(line).unwrap()),
                                    },
                                });
                                return Ok(());
                            }
                            RuleToggle::DisableRule {
                                rule: ref rule_name,
                                next_line_only,
                            } => {
                                let disables_for_rule =
                                    disables.entry(rule_name.clone()).or_default();
                                let last_disable = disables_for_rule.last();
                                if last_disable.is_some()
                                    && last_disable.unwrap().line_range.end.is_none()
                                {
                                    warn!("Rule {} was disabled twice in succession. This might indicate that the first disable was no reversed as intended.", rule_name);
                                }

                                let start_line =
                                    expression.position.as_ref().map(|pos| pos.start.line);
                                if start_line.is_none() {
                                    return Err(DisableParseError::new(format!("Could not disable rule {} because underlying node is missing line number.", rule_name)));
                                };
                                let end_line = if next_line_only {
                                    Some(start_line.unwrap() + 2)
                                } else {
                                    None
                                };

                                disables_for_rule.push(LintDisable {
                                    line_range: NonAdjustedLineRange {
                                        start: NonZeroUsize::new(start_line.unwrap()).unwrap(),
                                        end: end_line.map(|line| NonZeroUsize::new(line).unwrap()),
                                    },
                                });
                                return Ok(());
                            }
                        }
                    }

                    Ok(())
                }
                _ => {
                    if let Some(children) = ast.children() {
                        for child in children {
                            collect_lint_disables_internal(child, disables, ALL_MARKER)?;
                        }
                    }

                    Ok(())
                }
            }
        }
        collect_lint_disables_internal(ast, &mut disables, LintDisables::all_marker())?;

        for (_, value) in disables.iter_mut() {
            value.sort_by_key(|k| k.line_range.start);
        }

        Ok(disables)
    }

    pub fn is_rule_disabled_for_location(
        &self,
        rule: &str,
        location: &Location,
        context: &RuleContext,
    ) -> bool {
        debug!(
            "Checking if rule {} is disabled for location: {:#?}",
            rule, location
        );

        let overlapping_all_rules = self.0.get(LintDisables::all_marker()).map(|all_rules| {
            all_rules.iter().any(|rule| {
                let line_range =
                    AdjustedLineRange::from_unadjusted_line_range(&rule.line_range, context);
                location.overlaps_lines(&line_range)
            })
        });
        debug!("Overlapping all disables: {:#?}", overlapping_all_rules);
        if let Some(true) = overlapping_all_rules {
            return true;
        }

        let overlapping_specific_rules = self.0.get(rule).map(|specific_rules| {
            specific_rules.iter().any(|rule| {
                let line_range =
                    AdjustedLineRange::from_unadjusted_line_range(&rule.line_range, context);
                location.overlaps_lines(&line_range)
            })
        });
        debug!(
            "Overlapping specific disables: {:#?}",
            overlapping_specific_rules
        );
        matches!(overlapping_specific_rules, Some(true))
    }
}

impl TryFrom<&Node> for LintDisables {
    type Error = DisableParseError;

    fn try_from(node: &Node) -> Result<Self, DisableParseError> {
        let disables = LintDisables::collect_lint_disables(node)?;
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
        let disables: LintDisables = (&parse_result.ast).try_into().unwrap();
        debug!("Disables: {:?}", disables);

        assert_eq!(disables.0.len(), 1);
        assert_eq!(
            disables.0["foo"][0].line_range.start,
            NonZeroUsize::new(1).unwrap()
        );
        assert_eq!(
            disables.0["foo"][0].line_range.end,
            Some(NonZeroUsize::new(3).unwrap())
        );
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
        let disables: LintDisables = (&parse_result.ast).try_into().unwrap();
        debug!("Disables: {:?}", disables);

        assert_eq!(disables.0.len(), 2);
        assert_eq!(
            disables.0["bar"][0].line_range.start,
            NonZeroUsize::new(3).unwrap()
        );
        assert_eq!(
            disables.0["bar"][0].line_range.end,
            Some(NonZeroUsize::new(6).unwrap())
        );
    }

    #[test]
    fn test_collect_lint_disables_next_line() {
        let input = r#"{/* supa-mdx-lint-disable-next-line foo */}
This line is ignored
This line is not ignored"#;

        let parse_result = parse(input).unwrap();
        let disables: LintDisables = (&parse_result.ast).try_into().unwrap();
        debug!("Disables: {:?}", disables);

        assert_eq!(disables.0.len(), 1);
        assert_eq!(
            disables.0["foo"][0].line_range.end,
            Some(NonZeroUsize::new(3).unwrap())
        );
    }

    #[test]
    fn test_collect_lint_disables_disable_all() {
        let input = r#"{/* supa-mdx-lint-disable */}
Everything here is ignored
Still ignored
{/* supa-mdx-lint-enable */}"#;

        let parse_result = parse(input).unwrap();
        let disables: LintDisables = (&parse_result.ast).try_into().unwrap();
        debug!("Disables: {:?}", disables);

        assert_eq!(disables.0.len(), 1);
        assert_eq!(
            disables.0[LintDisables::all_marker()][0].line_range.start,
            NonZeroUsize::new(1).unwrap()
        );
        assert_eq!(
            disables.0[LintDisables::all_marker()][0].line_range.end,
            Some(NonZeroUsize::new(4).unwrap())
        );
    }

    #[test]
    fn test_collect_lint_never_reenabled() {
        let input = r#"{/* supa-mdx-lint-disable foo */}
Never reenabled"#;

        let parse_result = parse(input).unwrap();
        let disables: LintDisables = (&parse_result.ast).try_into().unwrap();
        debug!("Disables: {:?}", disables);

        assert_eq!(disables.0.len(), 1);
        assert!(disables.0["foo"][0].line_range.end.is_none());
    }

    #[test]
    fn test_collect_lint_disables_invalid_enable() {
        let input = r#"{/* supa-mdx-lint-enable foo */}
This should error because there was no disable"#;

        let parse_result = parse(input).unwrap();
        assert!(TryInto::<LintDisables>::try_into(&parse_result.ast).is_err());
    }
}
