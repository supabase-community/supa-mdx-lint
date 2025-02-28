use std::{
    borrow::Cow,
    cell::OnceCell,
    collections::{HashMap, VecDeque},
    hash::{Hash, Hasher},
    rc::Rc,
};

use anyhow::Result;
use bon::bon;
use either::Either;
use itertools::Itertools;
use log::{debug, warn};
use markdown::{
    mdast::{MdxFlowExpression, Node},
    unist,
};
use regex::Regex;

use crate::{
    app_error::{MultiError, ParseError, ResultBoth},
    geometry::{
        AdjustedOffset, AdjustedPoint, AdjustedRange, DenormalizedLocation, MaybeEndedLineRange,
    },
    parser::CommentString,
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
            (_, "configure", Some(rule), None) => Ok(LintTimeConfigureAttr {
                rule_name: rule,
                attributes: None,
                next_line_only: false,
            }),
            (_, "configure", Some(rule), Some(attributes)) => Ok(LintTimeConfigureAttr {
                rule_name: rule,
                attributes: Some(Cow::Borrowed(attributes)),
                next_line_only: false,
            }),
            (_, "configure-next-line", Some(rule), None) => Ok(LintTimeConfigureAttr {
                rule_name: rule,
                attributes: None,
                next_line_only: true,
            }),
            (_, "configure-next-line", Some(rule), Some(attributes)) => Ok(LintTimeConfigureAttr {
                rule_name: rule,
                attributes: Some(Cow::Borrowed(attributes)),
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
            ("enable", None) => RuleToggle::EnableAll,
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
            Regex::new(r"^supa-mdx-lint-(enable|disable|disable-next-line|configure|configure-next-line|(?:\s+(\S+)(?:\s+(.+))?)?$").expect("Hardcoded regex should not fail")
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
        context: &RuleContext<'_>,
    ) -> Result<MaybeEndedLineRange, ParseError> {
        let Some(pos) = curr.position() else {
            return Err(ParseError::MissingPosition(curr.variant_name()));
        };

        let start_offset = AdjustedOffset::from_unist(&pos.start, context);
        let start_line = AdjustedPoint::from_adjusted_offset(&start_offset, context.rope());

        if !next_line_only {
            return Ok(MaybeEndedLineRange::new(start_line.row, None));
        }

        let next = next
            .map(|next| {
                let Some(next_pos) = next.position() else {
                    return Err(ParseError::MissingPosition(next.variant_name()));
                };

                // Need to deal with positioning of column within line
                let end_offset = AdjustedOffset::from_unist(&next_pos.end, context);
                let end_line = AdjustedPoint::from_adjusted_offset(&end_offset, context.rope());

                Ok(end_line.row)
            })
            .transpose()?;

        Ok(MaybeEndedLineRange::new(start_line.row, next))
    }
}

pub(crate) struct ConfigurationCommentCollection<'comment>(
    Vec<
        Result<
            Either<LintTimeConfigureInfo<'comment>, (RuleToggle, MaybeEndedLineRange)>,
            ParseError,
        >,
    >,
);

impl<'ast> ConfigurationCommentCollection<'ast> {
    pub(crate) fn from_ast_context(ast: &'ast Node, context: &RuleContext) -> Option<Self> {
        let comment_pairs = collect_comment_pairs(ast)?;
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
                                .context(context)
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
                                .context(context)
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
        Some(Self(comment_pairs))
    }

    pub(crate) fn split(
        self,
    ) -> ResultBoth<(LintTimeRuleConfigs<'ast>, LintDisables<'ast>), MultiError> {
        let mut configs = LintTimeRuleConfigs::default();
        let mut disables = LintDisables::default();
        let mut errors = None::<MultiError>;
        for res in self.0 {
            match res {
                Ok(Either::Left(info)) => {
                    configs.insert(
                        info.attributes.rule_name.into(),
                        (
                            info.attributes.attributes.unwrap_or_default().into_owned(),
                            info.covered_range,
                        ),
                    );
                }
                Ok(Either::Right((info, range))) => todo!(),
                Err(err) => {
                    errors
                        .get_or_insert_with(MultiError::default)
                        .add_error(err.into());
                }
            }
        }
        ResultBoth::new()
            .set_result((configs, disables))
            .set_maybe_err(errors)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum RuleKey<'s> {
    All,
    Rule(&'s str),
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
        RuleKey::Rule(rule)
    }
}

#[derive(Debug, Default)]
pub struct LintTimeRuleConfigs<'key>(HashMap<RuleKey<'key>, (String, MaybeEndedLineRange)>);

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

// This is severely broken and does not do what's intended. We need to get
// ranges, because otherwise the logic of whethr the last one was on or off
// doesn't work.
fn maybe_merge_ranges(
    rule: &str,
    last: &mut (Switch, MaybeEndedLineRange),
    curr: (Switch, MaybeEndedLineRange),
) -> Result<Either<(), MaybeEndedLineRange>> {
    match curr.0 {
        Switch::On => match last.0 {
            Switch::On if last.1.overlaps_nonstrict(&curr.1) => {
                warn!("{rule} was enabled twice in succession for overlapping ranges. This can lead to an unexpected final effective range, and can probably be consolidated. Ranges: [{last:?}; {curr:?}]");

                let new_end = match (last.1.end, curr.1.end) {
                    (Some(last_end), Some(curr_end)) => Some(last_end.max(curr_end)),
                    _ => None,
                };

                last.1.end = new_end;
                Ok(Either::Left(()))
            }
            _ => Ok(Either::Right(curr.1)),
        },
        Switch::Off => match last.0 {
            Switch::On => {}
            Switch::Off => {
                warn!("{rule} was disabled twice in succession, so the last disable is a no-op. This is probably not what you want");

                Ok(Either::Left(()))
            }
        },
    }
}

impl<'key> LintDisablesBuilder<'key> {
    fn add_toggle(&mut self, rule_key: RuleKey<'key>, switch: Switch, range: MaybeEndedLineRange) {
        self.0.entry(rule_key).or_default().push((switch, range));
    }
}

#[derive(Debug, Default)]
pub struct LintDisables<'key>(HashMap<RuleKey<'key>, Vec<MaybeEndedLineRange>>);

impl<'key> From<LintDisablesBuilder<'key>> for LintDisables<'key> {
    fn from(builder: LintDisablesBuilder<'key>) -> Self {
        let mut disables = HashMap::new();

        for (rule_key, mut toggles) in builder.0 {
            toggles.sort_by_key(|(_, range)| range.clone());

            let mut disabled_ranges = Vec::<(Switch, MaybeEndedLineRange)>::new();
            for (switch, range) in toggles {
                match switch {
                    Switch::On => todo!(),
                    Switch::Off => match disabled_ranges.last() {
                        Some(last) => {}
                        None => {
                            disabled_ranges.push((switch, range));
                        }
                    },
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

        LintDisables(disables)
    }
}

impl<'key> LintDisables<'key> {
    /// Collects all disable statements in the AST and returns a map of rule names to their
    /// corresponding disables.
    fn collect_lint_disables(
        ast: &Node,
        context: &RuleContext,
    ) -> Result<HashMap<String, Vec<MaybeEndedLineRange>>, ParseError> {
        let mut disables = HashMap::<String, Vec<MaybeEndedLineRange>>::new();

        fn collect_lint_disables_internal(
            ast: &Node,
            next_node: Option<&Node>,
            context: &RuleContext,
            disables: &mut HashMap<String, Vec<MaybeEndedLineRange>>,
            all_marker: &str,
        ) -> std::result::Result<(), ParseError> {
            fn reenable_last(
                previous: Option<&mut Vec<MaybeEndedLineRange>>,
                current_position: &unist::Position,
                rule: Option<&str>,
                context: &RuleContext,
            ) -> Result<(), ParseError> {
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

                        Err(ParseError::UnmatchedPair(
                            format!("{subject_copula} enabled without a preceding disable"),
                            start_point.row + 1,
                            start_point.column + 1,
                        ))
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
                        return Err(ParseError::MissingPosition(expression.variant_name()));
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

    pub fn new(node: &Node, context: &RuleContext) -> Result<Self, ParseError> {
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

#[cfg(test)]
mod test {
    use markdown::mdast::Paragraph;

    use crate::parse;

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
        let mut comment_pairs = collect_comment_pairs(&parse_result.ast)
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
}
