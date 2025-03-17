use std::{borrow::Cow, collections::HashMap, iter::Peekable, sync::LazyLock};

use bon::bon;
use crop::RopeSlice;
use indexmap::IndexSet;
use log::{debug, trace};
use markdown::mdast;
use regex::Regex;
use serde::{
    de::{MapAccess, SeqAccess},
    ser::{SerializeMap, SerializeTuple},
    Deserialize, Serialize, Serializer,
};
use supa_mdx_macros::RuleName;

use crate::{
    context::Context,
    errors::LintError,
    fix::LintCorrection,
    location::{AdjustedRange, DenormalizedLocation},
    rope::Rope,
    utils::words::{
        extras::{WordIteratorExtension, WordIteratorPrefix},
        WordIterator, WordIteratorItem,
    },
    LintLevel,
};

use super::{Rule, RuleName, RuleSettings};

#[derive(Debug, Default, RuleName)]
pub struct Rule004ExcludeWords(WordExclusionIndex);

/// Provides an index of exclusions to allow for easy lookup and matching based
/// on the first word of the exclusion.
#[derive(Debug, Default)]
struct WordExclusionIndex {
    index: WordExclusionIndexInner,
    rules: Vec<RuleMeta>,
}

#[derive(Debug, Default)]
struct WordExclusionIndexInner(HashMap<Prefix<'static>, WordExclusionMeta>);

#[derive(Debug, Default, PartialEq, Eq, Hash)]
struct Prefix<'a>(Cow<'a, str>, CaseSensitivity);

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Hash, Deserialize, Serialize)]
enum CaseSensitivity {
    Sensitive,
    #[default]
    Insensitive,
}

#[derive(Debug, Default)]
struct WordExclusionMeta {
    /// The trailing part of an exclusion, after the first word is stripped.
    remainders: IndexSet<String>,
    /// The rule indexes and replacements associated with these exclusions, if
    /// any. Rule indexes correspond to the position within the rules of the
    /// WordExclusionIndex.
    ///
    /// Invariant: Ordering must correspond to the ordering of `remainders`.
    details: Vec<(usize, Option<String>)>,
}

/// The definition of a user-defined rule.
///
/// ## Fields
/// * `String` - A human-readable description of the rule
/// * `LintLevel` - The level at which the rule should be linted
#[derive(Debug, Default, Clone)]
struct RuleMeta(String, LintLevel);

/// A structure to allow for deserialization from an easy-to-write rule config
/// format.
#[derive(Debug, Default)]
struct WordExclusionIndexIntermediate {
    rule: HashMap<String, WordExclusionMetaIntermediate>,
}

impl Serialize for WordExclusionIndexIntermediate {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let mut map = serializer.serialize_map(Some(self.rule.len()))?;
        for (key, value) in &self.rule {
            map.serialize_entry(key, value)?;
        }
        map.end()
    }
}

impl<'de> Deserialize<'de> for WordExclusionIndexIntermediate {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        struct Visitor;

        impl<'de> serde::de::Visitor<'de> for Visitor {
            type Value = WordExclusionIndexIntermediate;

            fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
                formatter.write_str("A map of rule names to their exclusion details")
            }

            fn visit_map<M>(self, mut map: M) -> Result<Self::Value, M::Error>
            where
                M: MapAccess<'de>,
            {
                let mut rule = HashMap::new();

                while let Some((key, value)) =
                    map.next_entry::<String, WordExclusionMetaIntermediate>()?
                {
                    rule.insert(key, value);
                }

                Ok(WordExclusionIndexIntermediate { rule })
            }
        }

        deserializer.deserialize_any(Visitor)
    }
}

#[derive(Debug, Default, Deserialize, Serialize)]
struct WordExclusionMetaIntermediate {
    #[serde(default)]
    level: LintLevel,
    #[serde(default)]
    case_sensitive: bool,
    words: Vec<ExclusionDefinition>,
    description: String,
}

#[derive(Debug)]
enum ExclusionDefinition {
    ExcludeOnly(String),
    WithReplace(String, String),
}

impl Serialize for ExclusionDefinition {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        match self {
            ExclusionDefinition::ExcludeOnly(s) => serializer.serialize_str(s),
            ExclusionDefinition::WithReplace(a, b) => {
                let mut seq = serializer.serialize_tuple(2)?;
                seq.serialize_element(a)?;
                seq.serialize_element(b)?;
                seq.end()
            }
        }
    }
}

impl<'de> Deserialize<'de> for ExclusionDefinition {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        struct Visitor;

        impl<'de> serde::de::Visitor<'de> for Visitor {
            type Value = ExclusionDefinition;

            fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
                formatter.write_str("A string (representing an exclusion) or a tuple of two strings (representing an exclusion and its replacement")
            }

            fn visit_str<E>(self, value: &str) -> Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                Ok(ExclusionDefinition::ExcludeOnly(value.to_string()))
            }

            fn visit_seq<A>(self, mut seq: A) -> Result<Self::Value, A::Error>
            where
                A: SeqAccess<'de>,
            {
                let first: String = seq
                    .next_element()?
                    .ok_or_else(|| serde::de::Error::invalid_length(0, &self))?;
                let second: String = seq
                    .next_element()?
                    .ok_or_else(|| serde::de::Error::invalid_length(1, &self))?;
                Ok(ExclusionDefinition::WithReplace(first, second))
            }
        }

        deserializer.deserialize_any(Visitor)
    }
}

#[derive(Debug)]
struct IndexLookupResult<'a> {
    case_sensitive_details: Option<&'a WordExclusionMeta>,
    case_insensitive_details: Option<&'a WordExclusionMeta>,
}

impl From<bool> for CaseSensitivity {
    fn from(case_sensitive: bool) -> CaseSensitivity {
        if case_sensitive {
            CaseSensitivity::Sensitive
        } else {
            CaseSensitivity::Insensitive
        }
    }
}

impl<'a> From<(Cow<'a, str>, CaseSensitivity)> for Prefix<'a> {
    fn from((s, case_sensitivity): (Cow<'a, str>, CaseSensitivity)) -> Self {
        let prefix = match case_sensitivity {
            CaseSensitivity::Sensitive => s,
            CaseSensitivity::Insensitive => s.to_lowercase().into(),
        };
        Prefix(prefix, case_sensitivity)
    }
}

impl RuleMeta {
    fn description(&self) -> &str {
        &self.0
    }

    fn level(&self) -> LintLevel {
        self.1
    }
}

impl ExclusionDefinition {
    fn into_parts(self) -> (String, Option<String>) {
        match self {
            ExclusionDefinition::ExcludeOnly(w) => (w, None),
            ExclusionDefinition::WithReplace(w, r) => (w, Some(r)),
        }
    }
}

#[bon]
impl WordExclusionIndex {
    #[builder]
    fn insert_exclusion(
        &mut self,
        exclusion: ExclusionDefinition,
        case_sensitivity: CaseSensitivity,
        rule_index: usize,
    ) {
        let (word, replacement) = exclusion.into_parts();

        let rope = Rope::from(word.as_ref());
        let mut iter = WordIterator::new(rope.byte_slice(..), 0, Default::default());

        let prefix = iter.next();
        let remainder = iter.collect_remainder();

        if let Some(prefix) = prefix {
            self.handle_insert_prefix()
                .prefix(prefix.1.to_string())
                .maybe_remainder(remainder)
                .maybe_replacement(replacement)
                .case_sensitivity(case_sensitivity)
                .rule_index(rule_index)
                .call();
        }
    }

    #[builder]
    fn handle_insert_prefix(
        &mut self,
        prefix: String,
        remainder: Option<String>,
        replacement: Option<String>,
        case_sensitivity: CaseSensitivity,
        rule_index: usize,
    ) {
        let prefix = Prefix::from((Cow::from(prefix), case_sensitivity));
        let remainder = remainder.unwrap_or_default();

        let existing = self.index.0.get_mut(&prefix);
        match existing {
            Some(existing) => {
                let (inserted_idx, is_new) = existing.remainders.insert_full(remainder);

                if is_new {
                    existing.details.push((rule_index, replacement))
                } else {
                    let rule_meta = self
                        .rules
                        .get(rule_index)
                        .expect("Rule meta previously inserted into global rule map");
                    let new_rule_level = rule_meta.level();
                    match self.rules.get_mut(inserted_idx) {
                        Some(existing_rule) if existing_rule.level() < new_rule_level => {
                            if let Some(idx) = existing.details.get_mut(inserted_idx) {
                                *idx = (rule_index, replacement)
                            }
                        }
                        _ => {
                            // The new rule doesn't outrank the existing one,
                            // leave it.
                        }
                    }
                }
            }
            None => {
                let mut remainders = IndexSet::new();
                remainders.insert(remainder);

                self.index.0.insert(
                    prefix,
                    WordExclusionMeta {
                        remainders,
                        details: vec![(rule_index, replacement)],
                    },
                );
            }
        }
    }

    fn get<'a, 'b: 'a>(&'a self, prefix: &'b str) -> IndexLookupResult {
        let case_sensitive_key = Prefix::from((Cow::from(prefix), CaseSensitivity::Sensitive));
        let case_insensitive_key = Prefix::from((Cow::from(prefix), CaseSensitivity::Insensitive));

        let case_sensitive = self.index.0.get(&case_sensitive_key);
        let case_insensitive = self.index.0.get(&case_insensitive_key);

        IndexLookupResult {
            case_sensitive_details: case_sensitive,
            case_insensitive_details: case_insensitive,
        }
    }
}

impl From<WordExclusionIndexIntermediate> for WordExclusionIndex {
    fn from(exclude_words: WordExclusionIndexIntermediate) -> Self {
        let mut this = Self {
            index: WordExclusionIndexInner::default(),
            rules: Vec::with_capacity(exclude_words.rule.len()),
        };

        for (_, rule_details) in exclude_words.rule {
            let rule_index = this.rules.len();
            this.rules
                .push(RuleMeta(rule_details.description, rule_details.level));

            let words = rule_details.words;
            for word in words {
                this.insert_exclusion()
                    .exclusion(word)
                    .case_sensitivity(rule_details.case_sensitive.into())
                    .rule_index(rule_index)
                    .call();
            }
        }

        this
    }
}

impl Rule for Rule004ExcludeWords {
    fn default_level(&self) -> LintLevel {
        // An implementation is required for this trait, but this rule defines
        // its levels in its own configuration, so this is ignored.
        LintLevel::default()
    }

    fn setup(&mut self, settings: Option<&mut RuleSettings>) {
        trace!("Setting up Rule004ExcludeWords");

        let Some(settings) = settings else {
            return;
        };

        let rules = settings.get_deserializable::<WordExclusionIndexIntermediate>("rules");
        if let Some(rules) = rules {
            self.0 = rules.into();
        }

        debug!("Rule 004 is set up: {:#?}", self)
    }

    fn check(
        &self,
        ast: &mdast::Node,
        context: &Context,
        _level: LintLevel,
    ) -> Option<Vec<LintError>> {
        let mdast::Node::Text(text_node) = ast else {
            return None;
        };
        let Some(position) = &text_node.position else {
            return None;
        };
        debug!("Checking Rule 004 for node {:#?}", ast);

        let mut errors = None::<Vec<LintError>>;

        let range = AdjustedRange::from_unadjusted_position(position, context);
        let text = context
            .rope()
            .byte_slice(Into::<std::ops::Range<usize>>::into(range.clone()));
        let mut word_iterator: WordIteratorExtension<'_, WordIteratorPrefix> =
            WordIterator::new(text, range.start.into(), Default::default()).into();

        loop {
            let Some((offset, word, _)) = word_iterator.next() else {
                break;
            };
            let word = word.to_string();

            let ExclusionMatch {
                new_iterator,
                match_: r#match,
            } = self.match_exclusions(self.0.get(&word), word_iterator);
            word_iterator = new_iterator;

            if let Some(MatchDetails {
                last_word,
                rule,
                replacement,
            }) = r#match
            {
                let end_offset = match last_word {
                    Some(last_word) => last_word.0 + last_word.1.len(),
                    None => offset + word.len(),
                };

                let error = self
                    .create_lint_error()
                    .beginning_offset(offset)
                    .end_offset(end_offset)
                    .maybe_replacement(replacement)
                    .rule(rule)
                    .range(range.clone())
                    .context(context)
                    .call();
                errors.get_or_insert_with(Vec::new).push(error);
            }
        }

        errors
    }
}

enum Suffix<'a> {
    Finish,
    Remaining(&'a str),
}

impl<'a> From<&'a str> for Suffix<'a> {
    fn from(s: &'a str) -> Self {
        match s {
            "" => Suffix::Finish,
            _ => Suffix::Remaining(s),
        }
    }
}

struct ExclusionMatch<'a> {
    new_iterator: WordIteratorExtension<'a, WordIteratorPrefix<'a>>,
    match_: Option<MatchDetails>,
}

#[derive(Debug)]
struct MatchDetails {
    last_word: Option<LastWordMatched>,
    replacement: Option<String>,
    rule: RuleMeta,
}

#[derive(Debug)]
struct MatchDetailsIntermediate<'a> {
    match_: MatchDetailsIntermediateInner,
    rule: RuleMeta,
    replacement: &'a Option<String>,
}

#[derive(Debug)]
enum MatchDetailsIntermediateInner {
    OneWord,
    /// The match is multiple words long. The position of the last matching
    /// word is tracked to calculate the full match range later. This is the
    /// offset not in the text, but in the vector of matches so far.
    MultipleWords(usize),
}

#[derive(Debug)]
struct LastWordMatched(usize, String);

#[bon]
impl Rule004ExcludeWords {
    #[builder]
    fn create_lint_error(
        &self,
        beginning_offset: usize,
        end_offset: usize,
        range: AdjustedRange,
        replacement: Option<String>,
        context: &Context<'_>,
        rule: RuleMeta,
    ) -> LintError {
        trace!("Creating lint error for Rule004. Range: {range:#?}; Beginning offset: {beginning_offset}; End offset: {end_offset}");
        let narrowed_range = AdjustedRange::new(beginning_offset.into(), end_offset.into());
        let word = context.rope().byte_slice(narrowed_range.to_usize_range());

        let suggestion = vec![LintCorrection::create_word_splice_correction()
            .context(context)
            .outer_range(&range)
            .splice_range(&narrowed_range)
            .maybe_replace(replacement.clone().map(Cow::from))
            .call()];
        let location = DenormalizedLocation::from_offset_range(narrowed_range, context);
        let message = substitute_format_string(rule.description().to_string(), word, replacement);

        LintError::from_raw_location()
            .rule(self.name())
            .message(message)
            .level(rule.level())
            .location(location)
            .suggestions(suggestion)
            .call()
    }

    fn match_exclusions<'a>(
        &self,
        IndexLookupResult {
            case_sensitive_details,
            case_insensitive_details,
        }: IndexLookupResult,
        words: WordIteratorExtension<'a, WordIteratorPrefix<'a>>,
    ) -> ExclusionMatch<'a> {
        trace!("Checking for need to match exclusions in Rule 004");
        if case_sensitive_details.is_none() && case_insensitive_details.is_none() {
            return ExclusionMatch {
                new_iterator: words,
                match_: None,
            };
        }
        debug!("Matching exclusions in Rule 004");

        let mut result_so_far = None::<MatchDetailsIntermediate>;
        let all = combine_exclusions(case_sensitive_details, case_insensitive_details);

        let mut consumed = vec![];
        let words = self
            .match_exclusions_rec()
            .remaining(all)
            .consumed(&mut consumed)
            .words(words)
            .result(&mut result_so_far)
            .call();

        let new_iterator = {
            match result_so_far {
                Some(MatchDetailsIntermediate {
                    match_: MatchDetailsIntermediateInner::MultipleWords(end_pos_incl),
                    ..
                }) => reattach_unused_words(words, consumed.clone().into_iter(), end_pos_incl + 1),
                _ => reattach_unused_words(words, consumed.clone().into_iter(), 0),
            }
        };
        ExclusionMatch {
            new_iterator,
            match_: result_so_far.map(|res| MatchDetails {
                last_word: match res.match_ {
                    MatchDetailsIntermediateInner::OneWord => None,
                    MatchDetailsIntermediateInner::MultipleWords(end_pos_incl) => {
                        let last_word = consumed.into_iter().nth(end_pos_incl).expect(
                            "Saved result only points to actual positions in the list of matches",
                        );
                        Some(LastWordMatched(last_word.0, last_word.1.to_string()))
                    }
                },
                rule: res.rule,
                replacement: res.replacement.clone(),
            }),
        }
    }

    #[builder]
    fn match_exclusions_rec<'a, 'b>(
        &self,
        /// Words that have been consumed so far.
        consumed: &mut Vec<WordIteratorItem<'b>>,
        /// The remaining candidates that may still be viable matches. Stored
        /// alongside their rule index.
        mut remaining: Peekable<
            impl Iterator<Item = (usize, Suffix<'a>, CaseSensitivity, &'a Option<String>)>,
        >,
        /// The remaining words to match.
        mut words: WordIteratorExtension<'b, WordIteratorPrefix<'b>>,
        result: &mut Option<MatchDetailsIntermediate<'a>>,
    ) -> WordIteratorExtension<'b, WordIteratorPrefix<'b>> {
        #[cfg(debug_assertions)]
        trace!("Recursing through the match in Rule004. Consumed: \"{consumed:#?}\"; Current result: {result:#?}");

        match words.next() {
            None => {
                // There are no words left in the string to match. If any of
                // the prior matches were complete matches, then they are the
                // longest matches. Pick an arbitary one.
                if let Some((rule_index, _, _, repl)) =
                    remaining.find(|(_, rem, _, _)| matches!(rem, Suffix::Finish))
                {
                    self.save_result()
                        .matched(consumed)
                        .rule_index(rule_index)
                        .replacement(repl)
                        .result(result)
                        .call()
                }
                words
            }
            Some(word_item) => {
                let mut next_iteration = None;
                for (rule_index, suffix, case_sensitivity, repl) in remaining {
                    match suffix {
                        Suffix::Finish => self
                            .save_result()
                            .matched(consumed)
                            .rule_index(rule_index)
                            .result(result)
                            .replacement(repl)
                            .call(),
                        Suffix::Remaining(s) => {
                            if let Some(remainder) =
                                trim_start((s, case_sensitivity), word_item.1.to_string())
                            {
                                // The match could potentially continue. Store the
                                // candidate to run another iteration.
                                next_iteration.get_or_insert_with(Vec::new).push((
                                    rule_index,
                                    Suffix::from(remainder),
                                    case_sensitivity,
                                    repl,
                                ));
                            }
                        }
                    }
                }

                consumed.push(word_item);
                if let Some(next_iteration) = next_iteration {
                    self.match_exclusions_rec()
                        .remaining(next_iteration.into_iter().peekable())
                        .words(words)
                        .consumed(consumed)
                        .result(result)
                        .call()
                } else {
                    words
                }
            }
        }
    }

    #[builder]
    fn save_result<'a>(
        &self,
        matched: &[WordIteratorItem<'_>],
        rule_index: usize,
        replacement: &'a Option<String>,
        result: &mut Option<MatchDetailsIntermediate<'a>>,
    ) {
        let match_ = if matched.is_empty() {
            MatchDetailsIntermediateInner::OneWord
        } else {
            MatchDetailsIntermediateInner::MultipleWords(matched.len() - 1)
        };

        result.replace(MatchDetailsIntermediate {
            match_,
            rule: self
                .0
                .rules
                .get(rule_index)
                .expect("Rule meta added when this linter rule was set up")
                .clone(),
            replacement,
        });
    }
}

fn combine_exclusions<'a>(
    case_sensitive: Option<&'a WordExclusionMeta>,
    case_insensitive: Option<&'a WordExclusionMeta>,
) -> Peekable<impl Iterator<Item = (usize, Suffix<'a>, CaseSensitivity, &'a Option<String>)>> {
    fn remainders_iter(
        details: &WordExclusionMeta,
    ) -> impl Iterator<Item = (usize, Suffix, &Option<String>)> {
        details.remainders.iter().enumerate().map(|(i, rem)| {
            let (rule_index, replacement) = details
                .details
                .get(i)
                .expect("Details added when setting up rule");
            (*rule_index, Suffix::from(rem.as_str()), replacement)
        })
    }

    let case_sensitive = case_sensitive
        .map(remainders_iter)
        .into_iter()
        .flatten()
        .map(|(i, rem, repl)| (i, rem, CaseSensitivity::Sensitive, repl));
    let case_insensitive = case_insensitive
        .map(remainders_iter)
        .into_iter()
        .flatten()
        .map(|(i, rem, repl)| (i, rem, CaseSensitivity::Insensitive, repl));

    case_sensitive.chain(case_insensitive).peekable()
}

fn trim_start(hay: (&str, CaseSensitivity), prefix: impl AsRef<str>) -> Option<&str> {
    let prefix = prefix.as_ref();
    match hay.1 {
        CaseSensitivity::Sensitive => {
            if hay.0.starts_with(prefix) {
                Some(&hay.0[prefix.len()..])
            } else {
                None
            }
        }
        CaseSensitivity::Insensitive => {
            let hay_lower = hay.0.to_lowercase();
            let prefix_lower = prefix.to_lowercase();
            if hay_lower.starts_with(&prefix_lower) {
                Some(&hay.0[prefix.len()..])
            } else {
                None
            }
        }
    }
}

fn reattach_unused_words<'words>(
    words: WordIteratorExtension<'words, WordIteratorPrefix<'words>>,
    consumed: impl Iterator<Item = WordIteratorItem<'words>>,
    num_used: usize,
) -> WordIteratorExtension<'words, WordIteratorPrefix<'words>> {
    #[cfg(debug_assertions)]
    trace!("Reattaching unused words after matching");
    words.extend_on_prefix(WordIteratorPrefix::new(consumed.skip(num_used)))
}

static FORMAT_REGEX: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"[^%](?<placeholder>%s|%r)").expect("Hardcoded regex should not fail to compile")
});

fn substitute_format_string(s: String, word: RopeSlice<'_>, replacement: Option<String>) -> String {
    if FORMAT_REGEX.captures(&s).is_none() {
        return s;
    }

    let mut result = String::with_capacity(s.len());
    let mut last_index = 0;
    for capture in FORMAT_REGEX.captures_iter(&s) {
        let placeholder = capture.name("placeholder").unwrap();
        let range = placeholder.range();

        let substitution = if placeholder.as_str().ends_with('s') {
            word.to_string()
        } else {
            replacement
                .clone()
                .unwrap_or("<REPLACEMENT_WORD>".to_string())
        };

        result.push_str(&s[last_index..range.start]);
        result.push_str(&substitution);
        last_index = range.end;
    }
    result.push_str(&s[last_index..]);
    result
}

#[cfg(test)]
mod tests {
    use crate::{
        fix::LintCorrectionReplace,
        location::AdjustedOffset,
        parser::{parse, ParseResult},
    };

    use super::*;

    fn setup_rule(
        rules: Vec<(impl Into<String>, WordExclusionMetaIntermediate)>,
    ) -> Rule004ExcludeWords {
        let mut rule = Rule004ExcludeWords::default();
        let mut settings = WordExclusionIndexIntermediate {
            rule: HashMap::new(),
        };

        for (rule_description, rule_meta) in rules {
            settings.rule.insert(rule_description.into(), rule_meta);
        }

        let mut settings =
            RuleSettings::with_serializable::<WordExclusionIndexIntermediate>("rules", &settings);
        rule.setup(Some(&mut settings));
        rule
    }

    fn get_simple_ast(
        md: impl AsRef<str>,
    ) -> (
        ParseResult,
        impl Fn(&ParseResult) -> &mdast::Node,
        impl Fn(&ParseResult) -> Context<'_>,
    ) {
        let parse_result = parse(md.as_ref()).unwrap();
        (
            parse_result,
            |parse_result| {
                parse_result
                    .ast()
                    .children()
                    .unwrap()
                    .first()
                    .unwrap()
                    .children()
                    .unwrap()
                    .first()
                    .unwrap()
            },
            |parse_result| {
                Context::builder()
                    .parse_result(parse_result)
                    .build()
                    .unwrap()
            },
        )
    }

    #[test]
    fn test_rule004_exclude_word() {
        let rules = vec![(
            "foo".to_string(),
            WordExclusionMetaIntermediate {
                description: "Don't use 'Foo'".to_string(),
                case_sensitive: true,
                words: vec![ExclusionDefinition::ExcludeOnly("Foo".to_string())],
                level: LintLevel::Error,
            },
        )];
        let rule = setup_rule(rules);

        let (parse_result, get_ast, get_context) = get_simple_ast("This is a Foo test.");
        let result = rule.check(
            get_ast(&parse_result),
            &get_context(&parse_result),
            LintLevel::Error,
        );
        assert!(result.is_some());

        let errors = result.unwrap();
        assert_eq!(errors.len(), 1);

        let error = errors.get(0).unwrap();
        assert_eq!(error.message, "Don't use 'Foo'");
        assert_eq!(error.level, LintLevel::Error);
        assert_eq!(error.location.offset_range.start, AdjustedOffset::from(10));
        assert_eq!(error.location.offset_range.end, AdjustedOffset::from(13));
    }

    #[test]
    fn test_rule004_exclude_and_replace_word() {
        let rules = vec![(
            "foo".to_string(),
            WordExclusionMetaIntermediate {
                description: "Don't use 'Foo'".to_string(),
                case_sensitive: true,
                words: vec![ExclusionDefinition::WithReplace(
                    "Foo".to_string(),
                    "Bar".to_string(),
                )],
                level: LintLevel::Error,
            },
        )];
        let rule = setup_rule(rules);

        let (parse_result, get_ast, get_context) = get_simple_ast("This is a Foo test.");
        let result = rule.check(
            get_ast(&parse_result),
            &get_context(&parse_result),
            LintLevel::Error,
        );
        assert!(result.is_some());

        let errors = result.unwrap();
        assert_eq!(errors.len(), 1);

        let error = errors.get(0).unwrap();
        assert_eq!(error.message, "Don't use 'Foo'");
        assert_eq!(error.level, LintLevel::Error);
        assert_eq!(error.location.offset_range.start, AdjustedOffset::from(10));
        assert_eq!(error.location.offset_range.end, AdjustedOffset::from(13));

        assert!(error.suggestions.is_some());
        let suggestions = error.suggestions.as_ref().unwrap();
        assert_eq!(suggestions.len(), 1);
        let suggestion = suggestions.get(0).unwrap();
        assert!(matches!(
            suggestion,
            LintCorrection::Replace(LintCorrectionReplace { .. })
        ));
    }

    #[test]
    fn test_rule004_exclude_multiple_words() {
        let rules = vec![
            (
                "foo",
                WordExclusionMetaIntermediate {
                    description: "Don't use 'Foo'".to_string(),
                    case_sensitive: true,
                    words: vec![ExclusionDefinition::ExcludeOnly("Foo".to_string())],
                    level: LintLevel::Error,
                },
            ),
            (
                "bar",
                WordExclusionMetaIntermediate {
                    description: "Don't use 'bar'".to_string(),
                    case_sensitive: true,
                    words: vec![ExclusionDefinition::ExcludeOnly("bar".to_string())],
                    level: LintLevel::Error,
                },
            ),
        ];
        let rule = setup_rule(rules);

        let (parse_result, get_ast, get_context) = get_simple_ast("This is a Foo test with bar.");
        let result = rule.check(
            get_ast(&parse_result),
            &get_context(&parse_result),
            LintLevel::Error,
        );
        assert!(result.is_some());

        let errors = result.unwrap();
        assert_eq!(errors.len(), 2);

        let error = errors.get(0).unwrap();
        assert_eq!(error.message, "Don't use 'Foo'");
        assert_eq!(error.level, LintLevel::Error);
        assert_eq!(error.location.offset_range.start, AdjustedOffset::from(10));
        assert_eq!(error.location.offset_range.end, AdjustedOffset::from(13));

        let error = errors.get(1).unwrap();
        assert_eq!(error.message, "Don't use 'bar'");
        assert_eq!(error.level, LintLevel::Error);
        assert_eq!(error.location.offset_range.start, AdjustedOffset::from(24));
        assert_eq!(error.location.offset_range.end, AdjustedOffset::from(27));
    }

    #[test]
    fn test_rule004_multiword_exclusion() {
        let rules = vec![(
            "foo bar".to_string(),
            WordExclusionMetaIntermediate {
                description: "Don't use 'Foo bar'".to_string(),
                case_sensitive: true,
                words: vec![ExclusionDefinition::ExcludeOnly("Foo bar".to_string())],
                level: LintLevel::Error,
            },
        )];
        let rule = setup_rule(rules);

        let (parse_result, get_ast, get_context) = get_simple_ast("This is a Foo bar test.");
        let result = rule.check(
            get_ast(&parse_result),
            &get_context(&parse_result),
            LintLevel::Error,
        );
        assert!(result.is_some());

        let errors = result.unwrap();
        assert_eq!(errors.len(), 1);

        let error = errors.get(0).unwrap();
        assert_eq!(error.message, "Don't use 'Foo bar'");
        assert_eq!(error.level, LintLevel::Error);
        assert_eq!(error.location.offset_range.start, AdjustedOffset::from(10));
        assert_eq!(error.location.offset_range.end, AdjustedOffset::from(17));
    }

    #[test]
    fn test_rule004_overlapping_exclusions() {
        let rules = vec![
            (
                "Foo barbie",
                WordExclusionMetaIntermediate {
                    description: "Don't use 'Foo barbie'".to_string(),
                    case_sensitive: true,
                    words: vec![ExclusionDefinition::ExcludeOnly("Foo barbie".to_string())],
                    level: LintLevel::Error,
                },
            ),
            (
                "bartender",
                WordExclusionMetaIntermediate {
                    description: "Don't use 'bartender'".to_string(),
                    case_sensitive: true,
                    words: vec![ExclusionDefinition::ExcludeOnly("bartender".to_string())],
                    level: LintLevel::Error,
                },
            ),
        ];
        let rule = setup_rule(rules);

        let (parse_result, get_ast, get_context) = get_simple_ast("This is a Foo bartender.");
        let result = rule.check(
            get_ast(&parse_result),
            &get_context(&parse_result),
            LintLevel::Error,
        );
        assert!(result.is_some());

        let errors = result.unwrap();
        assert_eq!(errors.len(), 1);

        let error = errors.get(0).unwrap();
        assert_eq!(error.message, "Don't use 'bartender'");
        assert_eq!(error.level, LintLevel::Error);
        assert_eq!(error.location.offset_range.start, AdjustedOffset::from(14));
        assert_eq!(error.location.offset_range.end, AdjustedOffset::from(23));
    }

    #[test]
    fn test_rule004_use_longest_overlapping() {
        let rules = vec![
            (
                "Foo bar",
                WordExclusionMetaIntermediate {
                    description: "Don't use 'Foo bar'".to_string(),
                    case_sensitive: true,
                    words: vec![ExclusionDefinition::ExcludeOnly("Foo bar".to_string())],
                    level: LintLevel::Error,
                },
            ),
            (
                "Foo bartender",
                WordExclusionMetaIntermediate {
                    description: "Don't use 'Foo bartender'".to_string(),
                    case_sensitive: true,
                    words: vec![ExclusionDefinition::ExcludeOnly(
                        "Foo bartender".to_string(),
                    )],
                    level: LintLevel::Error,
                },
            ),
        ];
        let rule = setup_rule(rules);

        let (parse_result, get_ast, get_context) = get_simple_ast("This is a Foo bartender.");
        let result = rule.check(
            get_ast(&parse_result),
            &get_context(&parse_result),
            LintLevel::Error,
        );
        assert!(result.is_some());

        let errors = result.unwrap();
        assert_eq!(errors.len(), 1);

        let error = errors.get(0).unwrap();
        assert_eq!(error.message, "Don't use 'Foo bartender'");
        assert_eq!(error.level, LintLevel::Error);
        assert_eq!(error.location.offset_range.start, AdjustedOffset::from(10));
        assert_eq!(error.location.offset_range.end, AdjustedOffset::from(23));
    }

    #[test]
    fn test_rule004_no_exclusions() {
        let rules = Vec::<(String, _)>::new();
        let rule = setup_rule(rules);

        let (parse_result, get_ast, get_context) = get_simple_ast("This is a Foo bar test.");
        let result = rule.check(
            get_ast(&parse_result),
            &get_context(&parse_result),
            LintLevel::Error,
        );
        assert!(result.is_none());
    }

    #[test]
    fn test_rule004_recover_false_longer_overlap() {
        let rules = vec![
            (
                "Foo bartender",
                WordExclusionMetaIntermediate {
                    description: "Don't use 'Foo bartender'".to_string(),
                    case_sensitive: true,
                    words: vec![ExclusionDefinition::ExcludeOnly(
                        "Foo bartender".to_string(),
                    )],
                    level: LintLevel::Error,
                },
            ),
            (
                "Foo bartender blah whaaaat",
                WordExclusionMetaIntermediate {
                    description: "Don't use 'Foo bartender blah whaaaat'".to_string(),
                    case_sensitive: true,
                    words: vec![ExclusionDefinition::ExcludeOnly(
                        "Foo bartender blah whaaaat".to_string(),
                    )],
                    level: LintLevel::Error,
                },
            ),
        ];
        let rule = setup_rule(rules);

        let (parse_result, get_ast, get_context) = get_simple_ast("This is a Foo bartender blah.");
        let result = rule.check(
            get_ast(&parse_result),
            &get_context(&parse_result),
            LintLevel::Error,
        );
        assert!(result.is_some());

        let errors = result.unwrap();
        assert_eq!(errors.len(), 1);

        let error = errors.get(0).unwrap();
        assert_eq!(error.message, "Don't use 'Foo bartender'");
        assert_eq!(error.level, LintLevel::Error);
        assert_eq!(error.location.offset_range.start, AdjustedOffset::from(10));
        assert_eq!(error.location.offset_range.end, AdjustedOffset::from(23));
    }

    #[test]
    fn test_rule004_no_matching_exclusions() {
        let rules = vec![(
            "Foo",
            WordExclusionMetaIntermediate {
                description: "Don't use 'Foo'".to_string(),
                case_sensitive: true,
                words: vec![ExclusionDefinition::ExcludeOnly("Foo".to_string())],
                level: LintLevel::Error,
            },
        )];
        let rule = setup_rule(rules);

        let (parse_result, get_ast, get_context) = get_simple_ast("This is a passing test.");
        let result = rule.check(
            get_ast(&parse_result),
            &get_context(&parse_result),
            LintLevel::Error,
        );
        assert!(result.is_none());
    }

    #[test]
    fn test_rule004_case_insensitive() {
        let rules = vec![(
            "foo",
            WordExclusionMetaIntermediate {
                description: "Don't use 'foo'".to_string(),
                case_sensitive: false,
                words: vec![ExclusionDefinition::ExcludeOnly("foo".to_string())],
                level: LintLevel::Error,
            },
        )];
        let rule = setup_rule(rules);

        let (parse_result, get_ast, get_context) = get_simple_ast("This is a Foo test.");
        let result = rule.check(
            get_ast(&parse_result),
            &get_context(&parse_result),
            LintLevel::Error,
        );
        assert!(result.is_some());

        let errors = result.unwrap();
        assert_eq!(errors.len(), 1);

        let error = errors.get(0).unwrap();
        assert_eq!(error.message, "Don't use 'foo'");
        assert_eq!(error.level, LintLevel::Error);
        assert_eq!(error.location.offset_range.start, AdjustedOffset::from(10));
        assert_eq!(error.location.offset_range.end, AdjustedOffset::from(13));
    }

    #[test]
    fn test_rule004_lint_level() {
        let rules = vec![(
            "foo",
            WordExclusionMetaIntermediate {
                description: "Don't use 'foo'".to_string(),
                case_sensitive: false,
                words: vec![ExclusionDefinition::ExcludeOnly("foo".to_string())],
                level: LintLevel::Warning,
            },
        )];
        let rule = setup_rule(rules);

        let (parse_result, get_ast, get_context) = get_simple_ast("This is a Foo test.");
        let result = rule.check(
            get_ast(&parse_result),
            &get_context(&parse_result),
            LintLevel::Error,
        );
        assert!(result.is_some());

        let errors = result.unwrap();
        assert_eq!(errors.len(), 1);

        let error = errors.get(0).unwrap();
        assert_eq!(error.message, "Don't use 'foo'");
        assert_eq!(error.level, LintLevel::Warning);
        assert_eq!(error.location.offset_range.start, AdjustedOffset::from(10));
        assert_eq!(error.location.offset_range.end, AdjustedOffset::from(13));
    }

    #[test]
    fn test_rule_004_exclusion_with_apostrophe() {
        let rules = vec![(
            "blah",
            WordExclusionMetaIntermediate {
                description: "blah blah blah".to_string(),
                case_sensitive: false,
                words: vec![ExclusionDefinition::ExcludeOnly("that's it".to_string())],
                level: LintLevel::Error,
            },
        )];
        let rule = setup_rule(rules);

        let (parse_result, get_ast, get_context) = get_simple_ast("That's it, Bob's your uncle.");
        let result = rule.check(
            get_ast(&parse_result),
            &get_context(&parse_result),
            LintLevel::Error,
        );
        assert!(result.is_some());

        let errors = result.unwrap();
        assert_eq!(errors.len(), 1);

        let error = errors.get(0).unwrap();
        assert_eq!(error.message, "blah blah blah");
        assert_eq!(error.level, LintLevel::Error);
        assert_eq!(error.location.offset_range.start, AdjustedOffset::from(0));
        assert_eq!(error.location.offset_range.end, AdjustedOffset::from(9));
    }

    #[test]
    fn test_rule_004_exclusion_with_other_punctuation() {
        let rules = vec![(
            "blah blah",
            WordExclusionMetaIntermediate {
                description: "This isn't Reddit.".to_string(),
                case_sensitive: false,
                words: vec![ExclusionDefinition::ExcludeOnly("tl;dr".to_string())],
                level: LintLevel::Error,
            },
        )];
        let rule = setup_rule(rules);

        let (parse_result, get_ast, get_context) = get_simple_ast("tl;dr: Just do the thing.");
        let result = rule.check(
            get_ast(&parse_result),
            &get_context(&parse_result),
            LintLevel::Error,
        );
        assert!(result.is_some());

        let errors = result.unwrap();
        assert_eq!(errors.len(), 1);

        let error = errors.get(0).unwrap();
        assert_eq!(error.message, "This isn't Reddit.");
        assert_eq!(error.level, LintLevel::Error);
        assert_eq!(error.location.offset_range.start, AdjustedOffset::from(0));
        assert_eq!(error.location.offset_range.end, AdjustedOffset::from(5));
    }

    #[test]
    fn test_rule_004_formatted_message() {
        let rules = vec![(
            "something",
            WordExclusionMetaIntermediate {
                description: "Don't use %s".to_string(),
                case_sensitive: false,
                words: vec![ExclusionDefinition::ExcludeOnly("ladeeda".to_string())],
                level: LintLevel::Error,
            },
        )];
        let rule = setup_rule(rules);

        let (parse_result, get_ast, get_context) = get_simple_ast("Well, ladeeda.");
        let result = rule.check(
            get_ast(&parse_result),
            &get_context(&parse_result),
            LintLevel::Error,
        );
        assert!(result.is_some());

        let errors = result.unwrap();
        assert_eq!(errors.len(), 1);

        let error = errors.get(0).unwrap();
        assert_eq!(error.message, "Don't use ladeeda");
    }

    #[test]
    fn test_rule_004_formatted_message_with_escape() {
        let rules = vec![(
            "something",
            WordExclusionMetaIntermediate {
                description: "Don't use %%s".to_string(),
                case_sensitive: false,
                words: vec![ExclusionDefinition::ExcludeOnly("ladeeda".to_string())],
                level: LintLevel::Error,
            },
        )];
        let rule = setup_rule(rules);

        let (parse_result, get_ast, get_context) = get_simple_ast("Well, ladeeda.");
        let result = rule.check(
            get_ast(&parse_result),
            &get_context(&parse_result),
            LintLevel::Error,
        );
        assert!(result.is_some());

        let errors = result.unwrap();
        assert_eq!(errors.len(), 1);

        let error = errors.get(0).unwrap();
        assert_eq!(error.message, "Don't use %%s");
    }

    #[test]
    fn test_rule_004_formatted_message_with_replacement() {
        let rules = vec![(
            "something",
            WordExclusionMetaIntermediate {
                description: "Use %r instead of %s".to_string(),
                case_sensitive: false,
                words: vec![ExclusionDefinition::WithReplace(
                    "PostgreSQL".to_string(),
                    "Postgres".to_string(),
                )],
                level: LintLevel::Error,
            },
        )];
        let rule = setup_rule(rules);

        let (parse_result, get_ast, get_context) = get_simple_ast("PostgreSQL is awesome!");
        let result = rule.check(
            get_ast(&parse_result),
            &get_context(&parse_result),
            LintLevel::Error,
        );
        assert!(result.is_some());

        let errors = result.unwrap();
        assert_eq!(errors.len(), 1);

        let error = errors.get(0).unwrap();
        assert_eq!(error.message, "Use Postgres instead of PostgreSQL");
    }

    #[test]
    fn test_rule_004_delete_at_beginning() {
        let rules = vec![(
            "yeah",
            WordExclusionMetaIntermediate {
                description: "Don't use yeah".to_string(),
                case_sensitive: false,
                words: vec![ExclusionDefinition::ExcludeOnly("Yeah".to_string())],
                level: LintLevel::Error,
            },
        )];
        let rule = setup_rule(rules);

        let (parse_result, get_ast, get_context) = get_simple_ast("Yeah this is awesome!");
        let result = rule.check(
            get_ast(&parse_result),
            &get_context(&parse_result),
            LintLevel::Error,
        );
        assert!(result.is_some());

        let errors = result.unwrap();
        assert_eq!(errors.len(), 1);

        let error = errors.get(0).unwrap();
        let suggestion = error.suggestions.as_ref().unwrap().get(0).unwrap();
        match suggestion {
            LintCorrection::Replace(replace) => {
                assert_eq!(replace.location.offset_range.start, AdjustedOffset::from(0));
                assert_eq!(replace.text(), "T".to_string());
            }
            other => panic!("Should have been a replacement, got: {other:#?}"),
        }
    }
}
