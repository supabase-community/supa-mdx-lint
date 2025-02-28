use std::{
    borrow::Cow,
    cell::OnceCell,
    collections::{HashMap, VecDeque},
    hash::{Hash, Hasher},
};

use anyhow::Result;
use bon::bon;
use either::Either;
use markdown::mdast::{MdxFlowExpression, Node};
use regex::Regex;

use crate::{
    app_error::{MultiError, ParseError, ResultBoth},
    geometry::{AdjustedOffset, AdjustedPoint, DenormalizedLocation, MaybeEndedLineRange},
    parser::{CommentString, ParseResult},
    utils::mdast::{MaybePosition, VariantName},
    RuleContext,
};

#[derive(Debug, PartialEq, Eq)]
struct HashableMdxNode<'node> {
    inner: &'node MdxFlowExpression,
}

impl<'node> HashableMdxNode<'node> {
    fn new(inner: &'node MdxFlowExpression) -> Self {
        Self { inner }
    }
}

impl<'node> Hash for HashableMdxNode<'node> {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        if let Some(pos) = &self.inner.position {
            pos.start.line.hash(state);
            pos.start.column.hash(state);
            pos.end.line.hash(state);
            pos.end.column.hash(state);
        }
        self.inner.value.hash(state);
    }
}

/// Collect a map of comment pairs and their next non-comment nodes from the
/// given AST.
fn collect_comment_pairs<'ast>(
    root: &'ast Node,
) -> Option<HashMap<HashableMdxNode<'ast>, Option<&'ast Node>>> {
    let mut comment_q = None::<VecDeque<_>>;
    let mut pairs = None::<HashMap<_, _>>;

    fn traverse<'node>(
        node: &'node Node,
        comment_q: &mut Option<VecDeque<&'node MdxFlowExpression>>,
        pairs: &mut Option<HashMap<HashableMdxNode<'node>, Option<&'node Node>>>,
    ) {
        match node {
            Node::MdxFlowExpression(expr) if expr.value.is_comment() => {
                comment_q.get_or_insert_with(VecDeque::new).push_back(expr);
            }
            _ => {
                while let Some(comment) = comment_q.as_mut().and_then(|p| p.pop_front()) {
                    pairs
                        .get_or_insert_with(HashMap::new)
                        .insert(HashableMdxNode::new(comment), Some(node));
                }
            }
        }

        if let Some(children) = node.children() {
            for child in children {
                traverse(child, comment_q, pairs);
            }
        }
    }

    traverse(root, &mut comment_q, &mut pairs);

    while let Some(comment) = comment_q.as_mut().and_then(|p| p.pop_front()) {
        pairs
            .get_or_insert_with(HashMap::new)
            .insert(HashableMdxNode::new(comment), None);
    }

    pairs
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum RuleKey<'s> {
    All,
    Rule(Cow<'s, str>),
}

impl<'s> Hash for RuleKey<'s> {
    fn hash<H: Hasher>(&self, state: &mut H) {
        match self {
            RuleKey::All => 0.hash(state),
            RuleKey::Rule(rule) => rule.hash(state),
        }
    }
}

impl<'s> From<&'s str> for RuleKey<'s> {
    fn from(rule: &'s str) -> Self {
        RuleKey::Rule(Cow::Borrowed(rule))
    }
}

impl<'s> From<String> for RuleKey<'s> {
    fn from(rule: String) -> Self {
        RuleKey::Rule(Cow::Owned(rule))
    }
}

impl AsRef<str> for RuleKey<'_> {
    fn as_ref(&self) -> &str {
        match self {
            RuleKey::All => "All rules",
            RuleKey::Rule(rule) => rule,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct LintTimeConfigureAttr<'comment> {
    rule_name: &'comment str,
    attributes: Option<Cow<'comment, str>>,
    next_line_only: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct LintTimeConfigureInfo<'comment> {
    attributes: LintTimeConfigureAttr<'comment>,
    covered_range: MaybeEndedLineRange,
}

impl<'comment>
    TryFrom<(
        &str,
        &'comment str,
        Option<&'comment str>,
        Option<&'comment str>,
    )> for LintTimeConfigureAttr<'comment>
{
    type Error = ParseError;

    fn try_from(
        value: (
            &str,
            &'comment str,
            Option<&'comment str>,
            Option<&'comment str>,
        ),
    ) -> Result<Self, Self::Error> {
        match value {
            (_, "configure", Some(rule), Some(attributes)) => Ok(LintTimeConfigureAttr {
                rule_name: rule,
                attributes: Some(Cow::Borrowed(attributes)),
                next_line_only: false,
            }),
            (_, "configure", Some(rule), None) => Ok(LintTimeConfigureAttr {
                rule_name: rule,
                attributes: None,
                next_line_only: false,
            }),
            (_, "configure-next-line", Some(rule), Some(attributes)) => Ok(LintTimeConfigureAttr {
                rule_name: rule,
                attributes: Some(Cow::Borrowed(attributes)),
                next_line_only: true,
            }),
            (_, "configure-next-line", Some(rule), None) => Ok(LintTimeConfigureAttr {
                rule_name: rule,
                attributes: None,
                next_line_only: true,
            }),
            (orig, ..) => Err(ParseError::ConfigurationCommentMissingRule(
                orig.to_string(),
            )),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum RuleToggle {
    EnableAll,
    EnableRule { rule: String },
    DisableAll { next_line_only: bool },
    DisableRule { rule: String, next_line_only: bool },
}

impl From<(&str, Option<&str>)> for RuleToggle {
    fn from(value: (&str, Option<&str>)) -> Self {
        match value {
            ("enable", Some(rule)) => RuleToggle::EnableRule {
                rule: rule.to_string(),
            },
            ("enable", _) => RuleToggle::EnableAll,
            ("disable", Some(rule)) => RuleToggle::DisableRule {
                rule: rule.to_string(),
                next_line_only: false,
            },
            ("disable", None) => RuleToggle::DisableAll {
                next_line_only: false,
            },
            ("disable-next-line", Some(rule)) => RuleToggle::DisableRule {
                rule: rule.to_string(),
                next_line_only: true,
            },
            ("disable-next-line", None) => RuleToggle::DisableAll {
                next_line_only: true,
            },
            _ => unreachable!("Only valid toggle arguments sent from call site (hardcoded)"),
        }
    }
}

trait NextLineOnly {
    fn next_line_only(&self) -> bool;
}

impl NextLineOnly for RuleToggle {
    fn next_line_only(&self) -> bool {
        match self {
            RuleToggle::DisableAll { next_line_only } => *next_line_only,
            RuleToggle::DisableRule { next_line_only, .. } => *next_line_only,
            _ => false,
        }
    }
}

impl NextLineOnly for LintTimeConfigureAttr<'_> {
    fn next_line_only(&self) -> bool {
        self.next_line_only
    }
}

enum ConfigurationComment<'comment> {
    Configure(LintTimeConfigureAttr<'comment>),
    EnableDisable(RuleToggle),
}

const CONFIG_COMMENT_REGEX: OnceCell<Regex> = OnceCell::new();

#[bon]
impl<'comment> ConfigurationComment<'comment> {
    fn parse(value: &'comment str) -> Option<Self> {
        let comment_string = value.into_comment()?;

        let regex = CONFIG_COMMENT_REGEX;
        // supa-mdx-lint configure-next-line Rule001HeadingCase +Supabase +pgjwt
        let regex = regex.get_or_init(||
            Regex::new(r"^supa-mdx-lint-(enable|disable|disable-next-line|configure|configure-next-line)(?:\s+(\S+)(?:\s+(.+))?)?$").expect("Hardcoded regex should not fail")
        );

        if let Some(captures) = regex.captures(comment_string) {
            if let Some(action) = captures.get(1) {
                match action.as_str() {
                    toggle @ ("enable" | "disable" | "disable-next-line") => {
                        let rule_toggle =
                            RuleToggle::from((toggle, captures.get(2).map(|m| m.as_str())));
                        return Some(ConfigurationComment::EnableDisable(rule_toggle));
                    }
                    configuration @ ("configure" | "configure-next-line") => {
                        return LintTimeConfigureAttr::try_from((
                            comment_string,
                            configuration,
                            captures.get(2).map(|m| m.as_str()),
                            captures.get(3).map(|m| m.as_str()),
                        ))
                        .ok()
                        .map(|info| ConfigurationComment::Configure(info));
                    }
                    _ => {}
                }
            }
        }

        None
    }

    #[builder]
    fn get_covered_range(
        curr: impl MaybePosition + VariantName,
        next: Option<impl MaybePosition + VariantName>,
        next_line_only: bool,
        parsed: &ParseResult,
    ) -> Result<MaybeEndedLineRange, ParseError> {
        let Some(pos) = curr.position() else {
            return Err(ParseError::MissingPosition(curr.variant_name()));
        };

        let start_offset = AdjustedOffset::from_unist(&pos.start, parsed.content_start_offset());
        let start_line = AdjustedPoint::from_adjusted_offset(&start_offset, parsed.rope());

        if !next_line_only {
            return Ok(MaybeEndedLineRange::new(start_line.row, None));
        }

        let next = next
            .map(|next| {
                let Some(next_pos) = next.position() else {
                    return Err(ParseError::MissingPosition(next.variant_name()));
                };

                let end_offset =
                    AdjustedOffset::from_unist(&next_pos.end, parsed.content_start_offset());
                let end_point = AdjustedPoint::from_adjusted_offset(&end_offset, parsed.rope());

                if end_point.column == 0 {
                    Ok(Some(end_point.row))
                } else if end_point.row == parsed.rope().line_len() - 1 {
                    Ok(None)
                } else {
                    Ok(Some(end_point.row + 1))
                }
            })
            .transpose()?
            .flatten();

        Ok(MaybeEndedLineRange::new(start_line.row, next))
    }
}

#[derive(Debug, Default)]
pub(crate) struct ConfigurationCommentCollection<'comment>(
    Vec<
        Result<
            Either<LintTimeConfigureInfo<'comment>, (RuleToggle, MaybeEndedLineRange)>,
            ParseError,
        >,
    >,
);

impl<'ast> ConfigurationCommentCollection<'ast> {
    pub(crate) fn from_parse_result(parsed: &'ast ParseResult) -> Self {
        let ast = parsed.ast();
        let Some(comment_pairs) = collect_comment_pairs(ast) else {
            return Self::default();
        };
        let comment_pairs = comment_pairs
            .into_iter()
            .filter_map(|(comment, next_node)| {
                if let Some(config_comment) = ConfigurationComment::parse(&comment.inner.value) {
                    match config_comment {
                        ConfigurationComment::Configure(info) => {
                            match ConfigurationComment::get_covered_range()
                                .curr(comment.inner)
                                .maybe_next(next_node)
                                .next_line_only(info.next_line_only())
                                .parsed(parsed)
                                .call()
                            {
                                Ok(range) => Some(Ok(Either::Left(LintTimeConfigureInfo {
                                    attributes: info,
                                    covered_range: range,
                                }))),
                                Err(err) => Some(Err(err)),
                            }
                        }
                        ConfigurationComment::EnableDisable(info) => {
                            match ConfigurationComment::get_covered_range()
                                .curr(comment.inner)
                                .maybe_next(next_node)
                                .next_line_only(info.next_line_only())
                                .parsed(parsed)
                                .call()
                            {
                                Ok(range) => Some(Ok(Either::Right((info, range)))),
                                Err(err) => Some(Err(err)),
                            }
                        }
                    }
                } else {
                    None
                }
            })
            .collect();
        Self(comment_pairs)
    }

    pub(crate) fn into_parts(
        self,
    ) -> ResultBoth<(LintTimeRuleConfigs<'ast>, LintDisables<'ast>), MultiError> {
        let mut configs = LintTimeRuleConfigs::default();
        let mut disables_builder = LintDisablesBuilder::default();
        let mut errors = None::<MultiError>;
        for res in self.0.into_iter() {
            match res {
                Ok(Either::Left(info)) => {
                    let attributes = info.attributes.attributes.clone();
                    configs.insert(
                        info.attributes.rule_name.into(),
                        (
                            attributes.unwrap_or_default().into_owned(),
                            info.covered_range.clone(),
                        ),
                    );
                }
                Ok(Either::Right((info, range))) => match info {
                    RuleToggle::EnableAll => {
                        disables_builder.add_toggle(RuleKey::All.into(), Switch::On, range.clone())
                    }
                    RuleToggle::EnableRule { rule } => {
                        disables_builder.add_toggle(rule.into(), Switch::On, range.clone())
                    }
                    RuleToggle::DisableAll { .. } => {
                        disables_builder.add_toggle(RuleKey::All.into(), Switch::Off, range.clone())
                    }
                    RuleToggle::DisableRule { rule, .. } => {
                        disables_builder.add_toggle(rule.into(), Switch::Off, range.clone())
                    }
                },
                Err(err) => {
                    errors
                        .get_or_insert_with(MultiError::default)
                        .add_err(err.into());
                }
            }
        }

        let (disables, build_err) = disables_builder.build().split();
        if let Some(build_err) = build_err {
            errors
                .get_or_insert_with(MultiError::default)
                .add_err(Box::new(build_err));
        }

        ResultBoth::new((configs, disables), errors)
    }
}

#[derive(Debug, Default)]
pub(crate) struct LintTimeRuleConfigs<'key>(HashMap<RuleKey<'key>, (String, MaybeEndedLineRange)>);

impl<'key> std::ops::Deref for LintTimeRuleConfigs<'key> {
    type Target = HashMap<RuleKey<'key>, (String, MaybeEndedLineRange)>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl std::ops::DerefMut for LintTimeRuleConfigs<'_> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

#[derive(Debug)]
enum Switch {
    On,
    Off,
}

#[derive(Debug, Default)]
struct LintDisablesBuilder<'key>(HashMap<RuleKey<'key>, Vec<(Switch, MaybeEndedLineRange)>>);

#[derive(Debug, Default)]
pub struct LintDisables<'key>(HashMap<RuleKey<'key>, Vec<MaybeEndedLineRange>>);

#[derive(Debug)]
enum MergeRangesResult {
    Merged,
    NotMerged((Switch, MaybeEndedLineRange)),
}

/// Merges two disable directives if they are a pair (disable, enable).
fn maybe_merge_ranges(
    rule: &str,
    last: &mut MaybeEndedLineRange,
    curr: (Switch, MaybeEndedLineRange),
) -> Result<MergeRangesResult, ParseError> {
    match curr.0 {
        Switch::On => {
            if last.is_open_ended() {
                last.end = Some(curr.1.start);
                Ok(MergeRangesResult::Merged)
            } else {
                Err(ParseError::UnmatchedConfigurationPair(
                    format!("{rule} enabled without a matching disable comment"),
                    curr.1.start,
                ))
            }
        }
        Switch::Off => {
            if last.overlaps_strict(&curr.1) {
                last.end = last.end.max(curr.1.end);
                Err(ParseError::UnmatchedConfigurationPair(format!("{rule} disabled twice in succession for overlapping ranges. This is probably not what you want and can cause the effective range to be different from expected."), curr.1.start))
            } else {
                Ok(MergeRangesResult::NotMerged(curr))
            }
        }
    }
}

impl<'key> LintDisablesBuilder<'key> {
    fn add_toggle(&mut self, rule_key: RuleKey<'key>, switch: Switch, range: MaybeEndedLineRange) {
        self.0.entry(rule_key).or_default().push((switch, range));
    }

    fn build(self) -> ResultBoth<LintDisables<'key>, MultiError> {
        let mut disables = HashMap::new();
        let mut errors = None::<MultiError>;

        for (rule_key, mut toggles) in self.0 {
            toggles.sort_by_key(|(_, range)| range.clone());

            let mut disabled_ranges = Vec::<(Switch, MaybeEndedLineRange)>::new();
            for toggle in toggles {
                match disabled_ranges.last_mut() {
                    Some(last) => {
                        match maybe_merge_ranges(rule_key.as_ref(), &mut last.1, toggle) {
                            Ok(MergeRangesResult::Merged) => {}
                            Ok(MergeRangesResult::NotMerged(toggle)) => {
                                disabled_ranges.push(toggle)
                            }
                            Err(err) => {
                                errors
                                    .get_or_insert_with(MultiError::default)
                                    .add_err(Box::new(err));
                            }
                        }
                    }
                    None if matches!(toggle.0, Switch::Off) => {
                        disabled_ranges.push(toggle);
                    }
                    None if matches!(toggle.0, Switch::On) => {
                        errors
                            .get_or_insert_with(MultiError::default)
                            .add_err(Box::new(ParseError::UnmatchedConfigurationPair(
                                format!(
                                "{} enabled with corresponding disable statement. This is a no-op",
                                rule_key.as_ref()
                            ),
                                toggle.1.start,
                            )));
                    }
                    _ => unreachable!(
                        "Compiler does not seem to know that all the toggle variations are covered"
                    ),
                }
            }
            disables.insert(
                rule_key,
                disabled_ranges
                    .into_iter()
                    .map(|(_, range)| range)
                    .collect(),
            );
        }

        ResultBoth::new(LintDisables(disables), errors)
    }
}

impl<'key> LintDisables<'key> {
    pub(crate) fn disabled_for_location(
        &self,
        rule_name: &str,
        location: &DenormalizedLocation,
        ctx: &RuleContext,
    ) -> bool {
        let all_key = RuleKey::All;
        let specific_key = RuleKey::from(rule_name);

        if let Some(disabled_ranges) = self.0.get(&all_key) {
            if disabled_ranges
                .iter()
                .any(|range| range.overlaps_lines(&location.offset_range, ctx.rope()))
            {
                return true;
            }
        } else if let Some(disabled_ranges) = self.0.get(&specific_key) {
            if disabled_ranges
                .iter()
                .any(|range| range.overlaps_lines(&location.offset_range, ctx.rope()))
            {
                return true;
            }
        }

        false
    }
}

#[cfg(test)]
mod test {

    use markdown::mdast::Paragraph;

    use crate::parse;

    use super::*;

    #[test]
    fn test_collect_comment_pairs() {
        let markdown = r#"
{/* Comment 1 */}
{/* Comment 2 */}
Paragraph 1

A list:
- Item 1
  {/* Comment 3 */}
- Item 2
"#;

        let parse_result = parse(markdown).unwrap();
        let mut comment_pairs = collect_comment_pairs(parse_result.ast())
            .unwrap()
            .into_iter()
            .collect::<Vec<_>>();
        comment_pairs.sort_by_key(|(comment, _)| &comment.inner.value);

        let first = comment_pairs.first().unwrap();
        assert_eq!(first.0.inner.value, "/* Comment 1 */");
        match first.1 {
            Some(Node::Paragraph(Paragraph { children, .. })) => match children.get(0).unwrap() {
                Node::Text(text) => {
                    assert_eq!(text.value, "Paragraph 1");
                }
                _ => {
                    panic!("Expected a text node");
                }
            },
            _ => {
                panic!("Expected a paragraph");
            }
        }

        let second = comment_pairs.get(1).unwrap();
        assert_eq!(second.0.inner.value, "/* Comment 2 */");
        match second.1 {
            Some(Node::Paragraph(Paragraph { children, .. })) => match children.get(0).unwrap() {
                Node::Text(text) => {
                    assert_eq!(text.value, "Paragraph 1");
                }
                _ => {
                    panic!("Expected a text node");
                }
            },
            _ => {
                panic!("Expected a paragraph");
            }
        }

        let third = comment_pairs.get(2).unwrap();
        assert_eq!(third.0.inner.value, "/* Comment 3 */");
        match third.1 {
            Some(Node::ListItem(list_item)) => match list_item.children.get(0).unwrap() {
                Node::Paragraph(Paragraph { children, .. }) => match children.get(0).unwrap() {
                    Node::Text(text) => {
                        assert_eq!(text.value, "Item 2");
                    }
                    _ => {
                        panic!("Expected a text node");
                    }
                },
                _ => {
                    panic!("Expected a paragraph");
                }
            },
            _ => {
                panic!("Expected a list item");
            }
        }
    }

    #[test]
    fn rule_toggle_parse_enable_all() {
        let value = "/* supa-mdx-lint-enable */";
        assert!(matches!(
            ConfigurationComment::parse(value),
            Some(ConfigurationComment::EnableDisable(RuleToggle::EnableAll))
        ));
    }

    #[test]
    fn rule_toggle_parse_enable_specific_rule() {
        let value = "/* supa-mdx-lint-enable specific-rule */";
        assert!(matches!(
            ConfigurationComment::parse(value),
            Some(ConfigurationComment::EnableDisable(RuleToggle::EnableRule { rule }))
                if rule == "specific-rule"
        ));
    }

    #[test]
    fn rule_toggle_parse_disable_all() {
        let value = "/* supa-mdx-lint-disable */";
        assert!(matches!(
            ConfigurationComment::parse(value),
            Some(ConfigurationComment::EnableDisable(RuleToggle::DisableAll { next_line_only }))
                if !next_line_only
        ));
    }

    #[test]
    fn rule_toggle_parse_disable_specific_rule() {
        let value = "/* supa-mdx-lint-disable specific-rule */";
        assert!(matches!(
            ConfigurationComment::parse(value),
            Some(ConfigurationComment::EnableDisable(RuleToggle::DisableRule { rule, next_line_only }))
            if rule == "specific-rule" && !next_line_only
        ));
    }

    #[test]
    fn rule_toggle_parse_disable_next_line_all() {
        let value = "/* supa-mdx-lint-disable-next-line */";
        assert!(matches!(
            ConfigurationComment::parse(value),
            Some(ConfigurationComment::EnableDisable(RuleToggle::DisableAll { next_line_only }))
            if next_line_only
        ));
    }

    #[test]
    fn rule_toggle_parse_disable_next_line_specific_rule() {
        let value = "/* supa-mdx-lint-disable-next-line specific-rule */";
        assert!(matches!(
            ConfigurationComment::parse(value),
            Some(ConfigurationComment::EnableDisable(RuleToggle::DisableRule { rule, next_line_only }))
            if rule == "specific-rule" && next_line_only
        ));
    }

    #[test]
    fn rule_toggle_parse_invalid_format() {
        let value = "supa-mdx-lint-enable";
        assert!(ConfigurationComment::parse(value).is_none());
    }

    #[test]
    fn rule_toggle_parse_invalid_command() {
        let value = "/* supa-mdx-lint-invalid */";
        assert!(ConfigurationComment::parse(value).is_none());
    }

    #[test]
    fn rule_toggle_parse_ignores_whitespace() {
        let value = "     /*     supa-mdx-lint-enable  rule-name  */";
        assert!(matches!(
            ConfigurationComment::parse(value),
            Some(ConfigurationComment::EnableDisable(RuleToggle::EnableRule { rule }))
            if rule == "rule-name"
        ));
    }

    #[test]
    fn test_collect_lint_disables_basic() {
        let input = r#"{/* supa-mdx-lint-disable foo */}
Some content
{/* supa-mdx-lint-enable foo */}"#;

        let parse_result = parse(input).unwrap();
        let (_, disables) = ConfigurationCommentCollection::from_parse_result(&parse_result)
            .into_parts()
            .unwrap();

        assert_eq!(disables.0.len(), 1);
        assert_eq!(disables.0[&"foo".into()][0].start, 0);
        assert_eq!(disables.0[&"foo".into()][0].end, Some(2));
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
        let (_, disables) = ConfigurationCommentCollection::from_parse_result(&parse_result)
            .into_parts()
            .unwrap();

        assert_eq!(disables.0.len(), 2);
        assert_eq!(disables.0[&"bar".into()][0].start, 2);
        assert_eq!(disables.0[&"bar".into()][0].end, Some(5));
    }

    #[test]
    fn test_collect_lint_disables_next_line() {
        let input = r#"{/* supa-mdx-lint-disable-next-line foo */}
This line is ignored

This line is not ignored"#;

        let parse_result = parse(input).unwrap();
        let (_, disables) = ConfigurationCommentCollection::from_parse_result(&parse_result)
            .into_parts()
            .unwrap();

        assert_eq!(disables.0.len(), 1);
        assert_eq!(disables.0[&"foo".into()][0].end, Some(2));
    }

    #[test]
    fn test_collect_lint_disables_disable_all() {
        let input = r#"{/* supa-mdx-lint-disable */}
Everything here is ignored
Still ignored
{/* supa-mdx-lint-enable */}"#;

        let parse_result = parse(input).unwrap();
        let (_, disables) = ConfigurationCommentCollection::from_parse_result(&parse_result)
            .into_parts()
            .unwrap();

        assert_eq!(disables.0.len(), 1);
        assert_eq!(disables.0[&RuleKey::All][0].start, 0);
        assert_eq!(disables.0[&RuleKey::All][0].end, Some(3));
    }

    #[test]
    fn test_collect_lint_never_reenabled() {
        let input = r#"{/* supa-mdx-lint-disable foo */}
Never reenabled"#;

        let parse_result = parse(input).unwrap();
        let (_, disables) = ConfigurationCommentCollection::from_parse_result(&parse_result)
            .into_parts()
            .unwrap();

        assert_eq!(disables.0.len(), 1);
    }

    #[test]
    fn test_collect_lint_disables_invalid_enable() {
        let input = r#"{/* supa-mdx-lint-enable foo */}
This should error because there was no disable"#;

        let parse_result = parse(input).unwrap();
        let result = ConfigurationCommentCollection::from_parse_result(&parse_result).into_parts();

        assert!(result.has_err());
    }

    #[test]
    fn test_collect_lint_disables_skip_blank_lines() {
        let input = r#"{/* supa-mdx-lint-disable-next-line foo */}

This line is ignored

This line is not ignored"#;

        let parse_result = parse(input).unwrap();
        let (_, disables) = ConfigurationCommentCollection::from_parse_result(&parse_result)
            .into_parts()
            .unwrap();

        assert_eq!(disables.0.len(), 1);
        assert_eq!(disables.0[&"foo".into()][0].end, Some(3));
    }

    #[test]
    fn test_collect_lint_disables_skip_intervening_comments() {
        let input = r#"{/* supa-mdx-lint-disable-next-line foo */}

{/* some other comment */}
{/* supa-mdx-lint-disable-next-line bar */}

This line is ignored by both foo and bar

This line is not ignored
"#;

        let parse_result = parse(input).unwrap();
        let (_, disables) = ConfigurationCommentCollection::from_parse_result(&parse_result)
            .into_parts()
            .unwrap();

        assert_eq!(disables.0.len(), 2);
        assert_eq!(disables.0[&"foo".into()][0].start, 0);
        assert_eq!(disables.0[&"foo".into()][0].end, Some(6));
        assert_eq!(disables.0[&"bar".into()][0].start, 3);
        assert_eq!(disables.0[&"bar".into()][0].end, Some(6));
    }

    #[test]
    fn test_collect_lint_disables_with_frontmatter() {
        let input = r#"---
title: Some frontmatter
description: Testing with frontmatter
---

{/* supa-mdx-lint-disable-next-line foo */}
This line should be ignored by foo

Regular content

{/* supa-mdx-lint-disable bar */}
These lines should be ignored by bar
More content
{/* supa-mdx-lint-enable bar */}

This line should not be ignored
"#;

        let parse_result = parse(input).unwrap();
        let (_, disables) = ConfigurationCommentCollection::from_parse_result(&parse_result)
            .into_parts()
            .unwrap();

        assert_eq!(disables.0.len(), 2);

        // Check foo rule
        assert_eq!(disables.0[&"foo".into()].len(), 1);
        assert_eq!(disables.0[&"foo".into()][0].start, 5);
        assert_eq!(disables.0[&"foo".into()][0].end, Some(7));

        // Check bar rule
        assert_eq!(disables.0[&"bar".into()].len(), 1);
        assert_eq!(disables.0[&"bar".into()][0].start, 10);
        assert_eq!(disables.0[&"bar".into()][0].end, Some(13));
    }
}
