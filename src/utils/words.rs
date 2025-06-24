use bon::builder;
use crop::RopeSlice;
use log::trace;

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum Capitalize {
    False,
    True,
}

#[derive(Debug, Default, Eq, PartialEq)]
pub(crate) enum BreakOnPunctuation {
    #[default]
    None,
    #[allow(dead_code)]
    Hyphen,
}

#[derive(Debug, Default, Eq, PartialEq)]
pub(crate) enum CapitalizeTriggerPunctuation {
    #[default]
    Standard,
    PlusColon,
}

#[derive(Debug)]
pub struct WordIterator<'rope> {
    rope: RopeSlice<'rope>,
    offset_from_parent: usize,
    parser: WordParser,
}

pub(crate) struct WordIteratorOptions {
    pub(crate) initial_capitalize: Capitalize,
    pub(crate) break_on_punctuation: BreakOnPunctuation,
    pub(crate) capitalize_trigger_punctuation: CapitalizeTriggerPunctuation,
}

impl Default for WordIteratorOptions {
    fn default() -> Self {
        Self {
            initial_capitalize: Capitalize::False,
            break_on_punctuation: Default::default(),
            capitalize_trigger_punctuation: Default::default(),
        }
    }
}

impl<'rope> WordIterator<'rope> {
    pub fn new(
        rope: RopeSlice<'rope>,
        offset_from_parent: usize,
        options: WordIteratorOptions,
    ) -> Self {
        Self {
            rope,
            offset_from_parent,
            parser: WordParser::new(
                options.initial_capitalize,
                options.break_on_punctuation,
                options.capitalize_trigger_punctuation,
            ),
        }
    }

    pub fn curr_index(&self) -> Option<usize> {
        if let ParseState::Initial = self.parser.state {
            assert!(self.parser.word_start_offset == self.parser.tracking_offset);
            Some(self.parser.word_start_offset)
        } else {
            None
        }
    }

    pub fn next_capitalize(&self) -> Option<Capitalize> {
        if let ParseState::Initial = self.parser.state {
            Some(self.parser.capitalize)
        } else {
            None
        }
    }

    pub(crate) fn collect_remainder(self) -> Option<String> {
        assert!(self.parser.word_start_offset == self.parser.tracking_offset);
        if self.parser.word_start_offset == self.rope.byte_len() {
            None
        } else {
            Some(
                self.rope
                    .byte_slice(self.parser.word_start_offset..)
                    .to_string(),
            )
        }
    }
}

pub(crate) type WordIteratorItem<'r> = (usize, RopeSlice<'r>, Capitalize);

impl<'rope> Iterator for WordIterator<'rope> {
    type Item = WordIteratorItem<'rope>;

    fn next(&mut self) -> Option<Self::Item> {
        let next_word_data = self.parser.parse(self.rope);

        if let Some((offset, slice, capitalize)) = next_word_data {
            Some((offset + self.offset_from_parent, slice, capitalize))
        } else {
            None
        }
    }
}

#[derive(Debug)]
struct WordParser {
    state: ParseState,
    word_start_offset: usize,
    tracking_offset: usize,
    capitalize: Capitalize,
    break_on_punctuation: BreakOnPunctuation,
    capitalize_trigger_punctuation: CapitalizeTriggerPunctuation,
}

#[derive(Debug, Default)]
enum ParseState {
    #[default]
    Initial,
    AsciiAlphabetic,
    OtherAlphabetic,
    Numeric,
    Whitespace,
    Escape,
    PostEscape,
    PunctuationLeading(String),
    PunctuationTrailing(String),
    Other,
}

#[derive(Debug, Clone)]
enum ParserNext {
    Continue,
    Break(usize, usize, Capitalize),
}

impl WordParser {
    fn new(
        initial_capitalize: Capitalize,
        break_on_punctuation: BreakOnPunctuation,
        capitalize_trigger_punctuation: CapitalizeTriggerPunctuation,
    ) -> Self {
        Self {
            state: ParseState::Initial,
            word_start_offset: 0,
            tracking_offset: 0,
            capitalize: initial_capitalize,
            break_on_punctuation,
            capitalize_trigger_punctuation,
        }
    }

    fn parse<'rope>(&mut self, rope: RopeSlice<'rope>) -> Option<WordIteratorItem<'rope>> {
        assert!(self.word_start_offset == self.tracking_offset);
        if self.word_start_offset >= rope.byte_len() {
            return None;
        }

        let chars = rope.byte_slice(self.word_start_offset..).chars();
        for c in chars {
            trace!("Parser loop iteration:");
            trace!("  state: {:?}", self.state);
            trace!("  word_start_offset: {}", self.word_start_offset);
            trace!("  tracking_offset: {}", self.tracking_offset);
            trace!(
                "  word so far: {}",
                rope.byte_slice(self.word_start_offset..self.tracking_offset)
            );
            trace!("  char: {c}");

            let next = match c {
                c if c.is_ascii_alphabetic() => self.consume_ascii_alphabetic(),
                '0'..='9' => self.consume_numeric(),
                _ if c.is_alphabetic() => self.consume_other_alphabetic(c),
                _ if c.is_whitespace() => self.consume_whitespace(c),
                '\\' => self.consume_escape(),
                _ if is_punctuation(&c) => self.consume_punctuation(c),
                _ => self.consume_other(c),
            };

            if let ParserNext::Break(start, end, capitalize) = next {
                trace!("Break parser at word end with start: {start}, end: {end}");
                self.word_start_offset = self.tracking_offset;
                return Some((start, rope.byte_slice(start..end), capitalize));
            }
        }

        let saved_start_offset = self.word_start_offset;
        let word_end_offset = self.calculate_final_word_end_offset();

        // Reset state to parse next word
        self.state = ParseState::Initial;
        self.word_start_offset = self.tracking_offset;

        if saved_start_offset == word_end_offset {
            None
        } else {
            Some((
                saved_start_offset,
                rope.byte_slice(saved_start_offset..word_end_offset),
                self.capitalize,
            ))
        }
    }

    fn consume_ascii_alphabetic(&mut self) -> ParserNext {
        trace!("consume_ascii_alphabetic");
        match &self.state {
            ParseState::Escape => {
                self.state = ParseState::PostEscape;
                self.tracking_offset += 1;
                ParserNext::Continue
            }
            _ => {
                self.state = ParseState::AsciiAlphabetic;
                self.tracking_offset += 1;
                ParserNext::Continue
            }
        }
    }

    fn consume_other_alphabetic(&mut self, c: char) -> ParserNext {
        trace!("consume_other_alphabetic: {c}");
        match &self.state {
            ParseState::Escape => {
                self.state = ParseState::PostEscape;
                self.tracking_offset += c.len_utf8();
                ParserNext::Continue
            }
            _ => {
                self.state = ParseState::OtherAlphabetic;
                self.tracking_offset += c.len_utf8();
                ParserNext::Continue
            }
        }
    }

    fn consume_numeric(&mut self) -> ParserNext {
        trace!("consume_numeric");
        match &self.state {
            ParseState::Escape => {
                self.state = ParseState::PostEscape;
                self.tracking_offset += 1;
                ParserNext::Continue
            }
            _ => {
                self.state = ParseState::Numeric;
                self.tracking_offset += 1;
                ParserNext::Continue
            }
        }
    }

    fn consume_whitespace(&mut self, c: char) -> ParserNext {
        trace!("consume_whitespace: {c}");
        match &self.state {
            ParseState::Initial | ParseState::PunctuationLeading(_) => {
                self.state = ParseState::Whitespace;
                self.word_start_offset += c.len_utf8();
                self.tracking_offset += c.len_utf8();
                ParserNext::Continue
            }
            ParseState::AsciiAlphabetic
            | ParseState::OtherAlphabetic
            | ParseState::Numeric
            | ParseState::Other
            | ParseState::PostEscape => {
                let word_end_offset = self.tracking_offset;
                let curr_capitalize = self.capitalize;

                self.state = ParseState::Initial;
                self.tracking_offset += c.len_utf8();
                self.capitalize = Capitalize::False;

                ParserNext::Break(self.word_start_offset, word_end_offset, curr_capitalize)
            }
            ParseState::Whitespace => {
                self.word_start_offset += c.len_utf8();
                self.tracking_offset += c.len_utf8();
                ParserNext::Continue
            }
            ParseState::Escape => {
                self.state = ParseState::PostEscape;
                self.tracking_offset += c.len_utf8();
                ParserNext::Continue
            }
            ParseState::PunctuationTrailing(punctuation) => {
                // If the word ends with a hyphen, preserve the hyphen so we
                // can capture bare prefixes like `pre-` and `post-`
                let preserve_punctuation = punctuation == "-";

                let word_end_offset = if preserve_punctuation {
                    self.tracking_offset
                } else {
                    self.tracking_offset.saturating_sub(punctuation.len())
                };
                let curr_capitalize = self.capitalize;

                if let Some(p) = punctuation.chars().last() {
                    self.capitalize = self.punc_triggers_capitalization(&p);
                }
                self.state = ParseState::Initial;
                self.tracking_offset += c.len_utf8();

                ParserNext::Break(self.word_start_offset, word_end_offset, curr_capitalize)
            }
        }
    }

    fn consume_punctuation(&mut self, c: char) -> ParserNext {
        trace!("consume_punctuation: {c}");
        match &self.state {
            ParseState::Initial | ParseState::Whitespace => {
                self.state = ParseState::PunctuationLeading(c.to_string());
                self.word_start_offset += c.len_utf8();
                self.tracking_offset += c.len_utf8();
                ParserNext::Continue
            }
            ParseState::AsciiAlphabetic
            | ParseState::OtherAlphabetic
            | ParseState::Numeric
            | ParseState::Other
            | ParseState::PostEscape => {
                if self.break_word_immediately_on_puncutation(&c) {
                    let word_end_offset = self.tracking_offset;
                    let curr_capitalize = self.capitalize;

                    self.capitalize = self.punc_triggers_capitalization(&c);
                    self.state = ParseState::Initial;
                    self.tracking_offset += c.len_utf8();

                    ParserNext::Break(self.word_start_offset, word_end_offset, curr_capitalize)
                } else {
                    self.state = ParseState::PunctuationTrailing(c.to_string());
                    self.tracking_offset += c.len_utf8();
                    ParserNext::Continue
                }
            }
            ParseState::Escape => {
                self.state = ParseState::PostEscape;
                self.tracking_offset += c.len_utf8();
                ParserNext::Continue
            }
            ParseState::PunctuationLeading(punctuation) => {
                self.state = ParseState::PunctuationLeading(format!("{punctuation}{c}"));
                self.word_start_offset += c.len_utf8();
                self.tracking_offset += c.len_utf8();
                ParserNext::Continue
            }
            ParseState::PunctuationTrailing(punctuation) => {
                if self.break_word_immediately_on_puncutation(&c) {
                    let word_end_offset = self.tracking_offset.saturating_sub(punctuation.len());
                    let curr_capitalize = self.capitalize;

                    self.capitalize = self.punc_triggers_capitalization(&c);
                    self.state = ParseState::Initial;
                    self.tracking_offset += c.len_utf8();

                    ParserNext::Break(self.word_start_offset, word_end_offset, curr_capitalize)
                } else {
                    self.state = ParseState::PunctuationTrailing(format!("{punctuation}{c}"));
                    self.tracking_offset += c.len_utf8();
                    ParserNext::Continue
                }
            }
        }
    }

    fn consume_escape(&mut self) -> ParserNext {
        trace!("consume_escape");
        match &self.state {
            ParseState::Escape => {
                self.state = ParseState::PostEscape;
                self.tracking_offset += 1;
                ParserNext::Continue
            }
            _ => {
                self.state = ParseState::Escape;
                self.tracking_offset += 1;
                ParserNext::Continue
            }
        }
    }

    fn consume_other(&mut self, c: char) -> ParserNext {
        trace!("consume_other: {c}");
        match &self.state {
            ParseState::Escape => {
                self.state = ParseState::PostEscape;
                self.tracking_offset += c.len_utf8();
                ParserNext::Continue
            }
            _ => {
                self.state = ParseState::Other;
                self.tracking_offset += c.len_utf8();
                ParserNext::Continue
            }
        }
    }

    fn calculate_final_word_end_offset(&self) -> usize {
        match &self.state {
            ParseState::PunctuationTrailing(punctuation) => {
                self.tracking_offset.saturating_sub(punctuation.len())
            }
            _ => self.tracking_offset,
        }
    }

    fn punc_triggers_capitalization_std(c: &char) -> bool {
        *c == '!' || *c == '?' || *c == '.'
    }

    fn punc_triggers_capitalization(&self, c: &char) -> Capitalize {
        if Self::punc_triggers_capitalization_std(c)
            || *c == ':'
                && matches!(
                    self.capitalize_trigger_punctuation,
                    CapitalizeTriggerPunctuation::PlusColon
                )
        {
            Capitalize::True
        } else {
            Capitalize::False
        }
    }

    fn break_on_hyphens(&self) -> bool {
        matches!(self.break_on_punctuation, BreakOnPunctuation::Hyphen)
    }

    fn break_word_immediately_on_puncutation(&self, c: &char) -> bool {
        match c {
            '–' | '—' | '―' => true,
            '-' => self.break_on_hyphens(),
            _ => false,
        }
    }
}

pub fn is_punctuation(c: &char) -> bool {
    *c == '!'
        || *c == '-'
        || *c == '–'
        || *c == '—'
        || *c == '―'
        || *c == '('
        || *c == ')'
        || *c == '['
        || *c == ']'
        || *c == '{'
        || *c == '}'
        || *c == ':'
        || *c == '\''
        || *c == '‘'
        || *c == '’'
        || *c == '“'
        || *c == '”'
        || *c == '"'
        || *c == '?'
        || *c == ','
        || *c == '.'
        || *c == ';'
}

const SENTENCE_ENDING_PUNCTUATION: &[char] = &['.', '!', '?', '…'];

fn is_sentence_ending_punctuation(c: &char) -> bool {
    SENTENCE_ENDING_PUNCTUATION.contains(c)
}

#[builder]
pub(crate) fn is_sentence_start(
    slice: RopeSlice<'_>,
    query_offset: usize,
    #[builder(default = true)] count_beginning_as_sentence_start: bool,
) -> bool {
    #[cfg(debug_assertions)]
    log::trace!("Checking if offset {query_offset} is at sentence start");

    let mut iter = WordIterator::new(slice, 0, Default::default())
        .enumerate()
        .peekable();

    let (preceding_offset, preceding_word, queried_offset, queried_word) = loop {
        match (iter.next(), iter.peek()) {
            (Some((0, _)), _) if query_offset == 0 && count_beginning_as_sentence_start => {
                return count_beginning_as_sentence_start;
            }
            (
                Some((_, (preceding_offset, preceding_word, _))),
                Some((_, (next_word_offset, next_word, _))),
            ) => {
                if *next_word_offset == query_offset {
                    break (
                        preceding_offset,
                        preceding_word,
                        next_word_offset,
                        next_word,
                    );
                }
            }
            _ => {
                return false;
            }
        }
    };

    // A word in the middle of a text is at the start of a sentence if it is
    // proceeded by a word immediately followed by punctuation. The punctuation
    // _must_ include a sentence-closing punctuation mark, which may be
    // surrounded by other punctuation. For example, `".)` would be a valid
    // sentence-ending punctuation cluster.
    //
    // We're also going to check for capitalization to avoid false positives
    // from punctuation clusters such as `(T.B.D.)`, though this will give us
    // false negatives for some special cases of words that are allowed to
    // be lowercase at sentence start. The number of these exceptions is
    // relatively small, and for simplicity's sake we will ignore them.
    if !(queried_word.is_char_boundary(0)
        && queried_word
            .chars()
            .next()
            .is_some_and(|c: char| c.is_uppercase()))
    {
        return false;
    }

    let between = slice
        .byte_slice(preceding_offset + preceding_word.byte_len()..*queried_offset)
        .chars();
    #[cfg(debug_assertions)]
    trace!(
        "Parsing the between-sentence text: \"{}\"",
        between.clone().collect::<String>()
    );
    between_sentence_parser::BetweenSentenceParser::new().parse(between)
}

mod between_sentence_parser {
    #[cfg(debug_assertions)]
    use log::trace;

    #[derive(Debug)]
    enum BetweenSentenceParserState {
        Initial,
        PrecedingPunctuation,
        SentenceEndingPunctuation(EndingPunctuationType),
        FollowingPunctuation,
        Whitespace,
        SentenceStartPunctuation,
    }

    #[derive(Debug)]
    enum EndingPunctuationType {
        Mixable,
        NonMixable(char),
    }

    #[derive(Debug)]
    pub(super) struct BetweenSentenceParser {
        state: BetweenSentenceParserState,
    }

    impl BetweenSentenceParser {
        pub(super) fn new() -> Self {
            Self {
                state: BetweenSentenceParserState::Initial,
            }
        }

        pub(super) fn parse(&mut self, chars: impl Iterator<Item = char>) -> bool {
            use BetweenSentenceParserState::*;

            for char in chars {
                #[cfg(debug_assertions)]
                trace!("Parser state: {:?}", self.state);

                match char {
                    c if c.is_whitespace() => match self.state {
                        SentenceEndingPunctuation(_) | FollowingPunctuation => {
                            self.state = Whitespace;
                        }
                        Whitespace => {}
                        _ => return false,
                    },
                    c if super::is_sentence_ending_punctuation(&c) => {
                        let r#type = match c {
                            '.' => EndingPunctuationType::NonMixable(c),
                            _ => EndingPunctuationType::Mixable,
                        };
                        match self.state {
                            Initial | PrecedingPunctuation => {
                                self.state = SentenceEndingPunctuation(r#type);
                            }
                            SentenceEndingPunctuation(EndingPunctuationType::Mixable)
                                if matches!(r#type, EndingPunctuationType::Mixable) => {}
                            SentenceEndingPunctuation(EndingPunctuationType::NonMixable(old_c))
                                if matches!(r#type, EndingPunctuationType::NonMixable(c) if c == old_c) =>
                                {}
                            _ => return false,
                        }
                    }
                    c if super::is_punctuation(&c) => match self.state {
                        Initial => {
                            self.state = PrecedingPunctuation;
                        }
                        PrecedingPunctuation | FollowingPunctuation | SentenceStartPunctuation => {}
                        SentenceEndingPunctuation(_) => {
                            self.state = FollowingPunctuation;
                        }
                        Whitespace => self.state = SentenceStartPunctuation,
                    },
                    _ => return false,
                }
            }

            matches!(self.state, Whitespace | SentenceStartPunctuation)
        }
    }
}

pub(crate) mod extras {
    use std::collections::VecDeque;

    use super::*;

    pub(crate) struct WordIteratorExtension<'a, I> {
        prefix: Option<I>,
        inner: WordIterator<'a>,
    }

    impl<'a, I> From<WordIterator<'a>> for WordIteratorExtension<'a, I> {
        fn from(inner: WordIterator<'a>) -> Self {
            Self {
                prefix: None,
                inner,
            }
        }
    }

    impl<'a, I> WordIteratorExtension<'a, I>
    where
        I: Iterator<Item = WordIteratorItem<'a>>,
    {
        pub(crate) fn extend_on_prefix(self, prefix: I) -> Self {
            Self {
                prefix: Some(prefix),
                inner: self.into_inner().1,
            }
        }

        pub(crate) fn into_inner(self) -> (Option<I>, WordIterator<'a>) {
            (self.prefix, self.inner)
        }
    }

    impl<'a, I> Iterator for WordIteratorExtension<'a, I>
    where
        I: Iterator<Item = WordIteratorItem<'a>>,
    {
        type Item = WordIteratorItem<'a>;

        fn next(&mut self) -> Option<Self::Item> {
            match self.prefix {
                Some(ref mut prefix) => prefix.next().or_else(|| self.inner.next()),
                None => self.inner.next(),
            }
        }
    }

    #[cfg(test)]
    pub(crate) struct UnitIterator<'a> {
        _marker: std::marker::PhantomData<&'a ()>,
    }

    #[cfg(test)]
    impl<'a> Iterator for UnitIterator<'a> {
        type Item = WordIteratorItem<'a>;

        fn next(&mut self) -> Option<Self::Item> {
            None
        }
    }

    pub(crate) struct WordIteratorPrefix<'a> {
        inner: VecDeque<WordIteratorItem<'a>>,
    }

    impl<'a> WordIteratorPrefix<'a> {
        pub(crate) fn new<I>(inner: I) -> Self
        where
            I: IntoIterator<Item = WordIteratorItem<'a>>,
        {
            Self {
                inner: inner.into_iter().collect(),
            }
        }
    }

    impl<'a> Iterator for WordIteratorPrefix<'a> {
        type Item = WordIteratorItem<'a>;

        fn next(&mut self) -> Option<Self::Item> {
            self.inner.pop_front()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crop::Rope;

    #[test]
    fn test_word_iterator_basic() {
        let rope = Rope::from("hello world");
        let slice = rope.byte_slice(..);
        let mut iter = WordIterator::new(slice, 0, Default::default());

        let (offset, word, cap) = iter.next().unwrap();
        assert_eq!(offset, 0);
        assert_eq!(word.to_string(), "hello");
        assert_eq!(cap, Capitalize::False);

        let (offset, word, cap) = iter.next().unwrap();
        assert_eq!(offset, 6);
        assert_eq!(word.to_string(), "world");
        assert_eq!(cap, Capitalize::False);

        assert!(iter.next().is_none());
    }

    #[test]
    fn test_word_iterator_with_punctuation() {
        let rope = Rope::from("hello, world!");
        let slice = rope.byte_slice(..);
        let mut iter = WordIterator::new(slice, 0, Default::default());

        let (offset, word, cap) = iter.next().unwrap();
        assert_eq!(offset, 0);
        assert_eq!(word.to_string(), "hello");
        assert_eq!(cap, Capitalize::False);

        let (offset, word, cap) = iter.next().unwrap();
        assert_eq!(offset, 7);
        assert_eq!(word.to_string(), "world");
        assert_eq!(cap, Capitalize::False);

        assert!(iter.next().is_none());
    }

    #[test]
    fn test_word_iterator_with_multiple_spaces() {
        let rope = Rope::from("hello   world");
        let slice = rope.byte_slice(..);
        let mut iter = WordIterator::new(slice, 0, Default::default());

        let (offset, word, cap) = iter.next().unwrap();
        assert_eq!(offset, 0);
        assert_eq!(word.to_string(), "hello");
        assert_eq!(cap, Capitalize::False);

        let (offset, word, cap) = iter.next().unwrap();
        assert_eq!(offset, 8);
        assert_eq!(word.to_string(), "world");
        assert_eq!(cap, Capitalize::False);

        assert!(iter.next().is_none());
    }

    #[test]
    fn test_word_iterator_with_numbers() {
        let rope = Rope::from("test123 456");
        let slice = rope.byte_slice(..);
        let mut iter = WordIterator::new(slice, 0, Default::default());

        let (offset, word, cap) = iter.next().unwrap();
        assert_eq!(offset, 0);
        assert_eq!(word.to_string(), "test123");
        assert_eq!(cap, Capitalize::False);

        let (offset, word, cap) = iter.next().unwrap();
        assert_eq!(offset, 8);
        assert_eq!(word.to_string(), "456");
        assert_eq!(cap, Capitalize::False);

        assert!(iter.next().is_none());
    }

    #[test]
    fn test_word_iterator_with_quotes() {
        let rope = Rope::from("hello \"world\"");
        let slice = rope.byte_slice(..);
        let mut iter = WordIterator::new(slice, 0, Default::default());

        let (offset, word, cap) = iter.next().unwrap();
        assert_eq!(offset, 0);
        assert_eq!(word.to_string(), "hello");
        assert_eq!(cap, Capitalize::False);

        let (offset, word, cap) = iter.next().unwrap();
        assert_eq!(offset, 7);
        assert_eq!(word.to_string(), "world");
        assert_eq!(cap, Capitalize::False);

        assert!(iter.next().is_none());
    }

    #[test]
    fn test_word_iterator_include_hyphen_on_bare_prefixes() {
        let rope = Rope::from("pre- and post-world");
        let slice = rope.byte_slice(..);
        let mut iter = WordIterator::new(slice, 0, Default::default());

        let (offset, word, _cap) = iter.next().unwrap();
        assert_eq!(offset, 0);
        assert_eq!(word.to_string(), "pre-");

        let (offset, word, _cap) = iter.nth(1).unwrap();
        assert_eq!(offset, 9);
        assert_eq!(word.to_string(), "post-world");
    }

    #[test]
    fn test_word_iterator_with_emoji() {
        let rope = Rope::from("hello 🤝 world");
        let slice = rope.byte_slice(..);
        let mut iter = WordIterator::new(slice, 0, Default::default());

        let (offset, word, cap) = iter.next().unwrap();
        assert_eq!(offset, 0);
        assert_eq!(word.to_string(), "hello");
        assert_eq!(cap, Capitalize::False);

        let (offset, word, cap) = iter.next().unwrap();
        assert_eq!(offset, 6);
        assert_eq!(word.to_string(), "🤝");
        assert_eq!(cap, Capitalize::False);

        let (offset, word, cap) = iter.next().unwrap();
        assert_eq!(offset, 11);
        assert_eq!(word.to_string(), "world");
        assert_eq!(cap, Capitalize::False);

        assert!(iter.next().is_none());
    }

    #[test]
    fn test_word_iterator_with_cjk() {
        let rope = Rope::from("hello 你好 world");
        let slice = rope.byte_slice(..);
        let mut iter = WordIterator::new(slice, 0, Default::default());

        let (offset, word, cap) = iter.next().unwrap();
        assert_eq!(offset, 0);
        assert_eq!(word.to_string(), "hello");
        assert_eq!(cap, Capitalize::False);

        let (offset, word, cap) = iter.next().unwrap();
        assert_eq!(offset, 6);
        assert_eq!(word.to_string(), "你好");
        assert_eq!(cap, Capitalize::False);

        let (offset, word, cap) = iter.next().unwrap();
        assert_eq!(offset, 13);
        assert_eq!(word.to_string(), "world");
        assert_eq!(cap, Capitalize::False);

        assert!(iter.next().is_none());
    }

    #[test]
    fn test_word_iterator_initial_capitalization() {
        let rope = Rope::from("hello world");
        let slice = rope.byte_slice(..);
        let mut iter = WordIterator::new(
            slice,
            0,
            WordIteratorOptions {
                initial_capitalize: Capitalize::True,
                ..Default::default()
            },
        );

        let (offset, word, cap) = iter.next().unwrap();
        assert_eq!(offset, 0);
        assert_eq!(word.to_string(), "hello");
        assert_eq!(cap, Capitalize::True);

        let (offset, word, cap) = iter.next().unwrap();
        assert_eq!(offset, 6);
        assert_eq!(word.to_string(), "world");
        assert_eq!(cap, Capitalize::False);

        assert!(iter.next().is_none());
    }

    #[test]
    fn test_word_iterator_subsequent_capitalization() {
        let rope = Rope::from("some thing. Sentence. World.");
        let slice = rope.byte_slice(..);
        let mut iter = WordIterator::new(slice, 0, Default::default());

        let (offset, word, cap) = iter.nth(2).unwrap();
        assert_eq!(offset, 12);
        assert_eq!(word.to_string(), "Sentence");
        assert_eq!(cap, Capitalize::True);

        let (offset, word, cap) = iter.next().unwrap();
        assert_eq!(offset, 22);
        assert_eq!(word.to_string(), "World");
        assert_eq!(cap, Capitalize::True);

        assert!(iter.next().is_none());
    }

    #[test]
    fn test_word_iterator_break_on_hyphens() {
        let rope = Rope::from("hello-world");
        let slice = rope.byte_slice(..);
        let mut iter = WordIterator::new(slice, 0, Default::default());

        let (_offset, word, _cap) = iter.next().unwrap();
        assert_eq!(word.to_string(), "hello-world");
        assert!(iter.next().is_none());

        let slice = rope.byte_slice(..);
        let mut iter = WordIterator::new(
            slice,
            0,
            WordIteratorOptions {
                break_on_punctuation: BreakOnPunctuation::Hyphen,
                ..Default::default()
            },
        );

        let (offset, word, _cap) = iter.next().unwrap();
        assert_eq!(offset, 0);
        assert_eq!(word.to_string(), "hello");

        let (offset, word, _cap) = iter.next().unwrap();
        assert_eq!(offset, 6);
        assert_eq!(word.to_string(), "world");

        assert!(iter.next().is_none());
    }

    #[test]
    fn test_word_iterator_capitalize_on_colons() {
        let rope = Rope::from("hello: world");
        let slice = rope.byte_slice(..);
        let mut iter = WordIterator::new(slice, 0, Default::default());

        let (_offset, word, cap) = iter.next().unwrap();
        assert_eq!(word.to_string(), "hello");
        assert_eq!(cap, Capitalize::False);

        let (_offset, word, cap) = iter.next().unwrap();
        assert_eq!(word.to_string(), "world");
        assert_eq!(cap, Capitalize::False);

        let slice = rope.byte_slice(..);
        let mut iter = WordIterator::new(
            slice,
            0,
            WordIteratorOptions {
                capitalize_trigger_punctuation: CapitalizeTriggerPunctuation::PlusColon,
                ..Default::default()
            },
        );

        let (_offset, word, cap) = iter.next().unwrap();
        assert_eq!(word.to_string(), "hello");
        assert_eq!(cap, Capitalize::False);

        let (_offset, word, cap) = iter.next().unwrap();
        assert_eq!(word.to_string(), "world");
        assert_eq!(cap, Capitalize::True);
    }

    #[test]
    fn test_word_iterator_complex_sentence() {
        let rope = Rope::from(
        "Each of these open source tools are amazing, but they all had a major drawback - we couldn't use Postgres as the server's datastore. If you haven't noticed yet, our team likes Postgres a lot 😉."
        );
        let slice = rope.byte_slice(..);

        let iter = WordIterator::new(slice, 0, Default::default());
        let mut offsets: Vec<usize> = Vec::new();
        let mut words: Vec<String> = Vec::new();
        let mut caps: Vec<Capitalize> = Vec::new();

        for (offset, word, cap) in iter {
            offsets.push(offset);
            words.push(word.to_string());
            caps.push(cap);
        }

        assert_eq!(
            offsets,
            vec![
                0, 5, 8, 14, 19, 26, 32, 36, 45, 49, 54, 58, 62, 64, 70, 81, 84, 93, 97, 106, 109,
                113, 122, 133, 136, 140, 148, 156, 161, 165, 170, 176, 185, 187, 191
            ]
        );
        assert_eq!(
            words,
            vec![
                "Each",
                "of",
                "these",
                "open",
                "source",
                "tools",
                "are",
                "amazing",
                "but",
                "they",
                "all",
                "had",
                "a",
                "major",
                "drawback",
                "we",
                "couldn't",
                "use",
                "Postgres",
                "as",
                "the",
                "server's",
                "datastore",
                "If",
                "you",
                "haven't",
                "noticed",
                "yet",
                "our",
                "team",
                "likes",
                "Postgres",
                "a",
                "lot",
                "😉"
            ]
        );
        for (idx, cap) in caps.iter().enumerate() {
            assert_eq!(
                *cap,
                if idx == 23 {
                    Capitalize::True
                } else {
                    Capitalize::False
                }
            );
        }
    }

    #[test]
    fn test_word_iterator_collect_remainder() {
        let rope = Rope::from("hello everybody in the world");
        let slice = rope.byte_slice(..);
        let mut iter = WordIterator::new(slice, 0, Default::default());

        iter.next();
        assert_eq!(
            iter.collect_remainder(),
            Some("everybody in the world".to_string())
        );
    }

    #[test]
    fn test_word_iterator_no_remainder() {
        let rope = Rope::from("hello");
        let slice = rope.byte_slice(..);
        let mut iter = WordIterator::new(slice, 0, Default::default());

        iter.next();
        assert!(iter.collect_remainder().is_none());
    }

    #[test]
    fn test_word_iterator_wrapper() {
        let rope = Rope::from("hello world");
        let slice = rope.byte_slice(..);
        let mut iter: extras::WordIteratorExtension<'_, extras::UnitIterator> =
            WordIterator::new(slice, 0, Default::default()).into();

        let (offset, word, cap) = iter.next().unwrap();
        assert_eq!(offset, 0);
        assert_eq!(word.to_string(), "hello");
        assert_eq!(cap, Capitalize::False);

        let (offset, word, cap) = iter.next().unwrap();
        assert_eq!(offset, 6);
        assert_eq!(word.to_string(), "world");
        assert_eq!(cap, Capitalize::False);

        assert!(iter.next().is_none());
    }

    #[test]
    fn test_word_iterator_wrapper_with_prefix() {
        let rope = Rope::from("hello world keep going");
        let slice = rope.byte_slice(..);

        let mut orig_iter: extras::WordIteratorExtension<'_, extras::WordIteratorPrefix> =
            WordIterator::new(slice, 0, Default::default()).into();

        let mut consumed = vec![];
        consumed.push(orig_iter.next().unwrap());
        consumed.push(orig_iter.next().unwrap());

        let mut new_iter = orig_iter.extend_on_prefix(extras::WordIteratorPrefix::new(consumed));

        let next = new_iter.next().unwrap();
        assert_eq!(next.0, 0);
        assert_eq!(next.1.to_string(), "hello");
        let next = new_iter.next().unwrap();
        assert_eq!(next.0, 6);
        assert_eq!(next.1.to_string(), "world");
        let next = new_iter.next().unwrap();
        assert_eq!(next.0, 12);
        assert_eq!(next.1.to_string(), "keep");
        let next = new_iter.next().unwrap();
        assert_eq!(next.0, 17);
        assert_eq!(next.1.to_string(), "going");
        assert!(new_iter.next().is_none());
    }

    #[test]
    fn test_is_sentence_start() {
        let rope = Rope::from("Hello world! What a wonderful day. What's up?");
        assert!(is_sentence_start()
            .slice(rope.byte_slice(..))
            .query_offset(0)
            .call());
        assert!(is_sentence_start()
            .slice(rope.byte_slice(..))
            .query_offset(13)
            .call());
        assert!(is_sentence_start()
            .slice(rope.byte_slice(..))
            .query_offset(35)
            .call());
        assert!(!is_sentence_start()
            .slice(rope.byte_slice(..))
            .query_offset(6)
            .call());
        assert!(!is_sentence_start()
            .slice(rope.byte_slice(..))
            .query_offset(11)
            .call());
        assert!(!is_sentence_start()
            .slice(rope.byte_slice(..))
            .query_offset(12)
            .call());
        assert!(!is_sentence_start()
            .slice(rope.byte_slice(..))
            .query_offset(40)
            .call());
    }

    #[test]
    fn test_is_sentence_start_handles_ellipsis() {
        let rope = Rope::from("Hello... world!");
        assert!(!is_sentence_start()
            .slice(rope.byte_slice(..))
            .query_offset(9)
            .call());

        let rope = Rope::from("Hello... World!");
        assert!(is_sentence_start()
            .slice(rope.byte_slice(..))
            .query_offset(9)
            .call());
    }

    #[test]
    fn test_is_sentence_start_handles_mixed_punctuation() {
        let rope = Rope::from("Hello?!?!?! World!");
        assert!(is_sentence_start()
            .slice(rope.byte_slice(..))
            .query_offset(12)
            .call());

        let rope = Rope::from("Hello.!?. What?");
        assert!(!is_sentence_start()
            .slice(rope.byte_slice(..))
            .query_offset(10)
            .call());
    }

    #[test]
    fn test_is_sentence_start_gracefully_fails_on_empty_rope() {
        assert!(!is_sentence_start()
            .slice(Rope::from("").byte_slice(..))
            .query_offset(0)
            .call());
    }

    #[test]
    fn test_is_sentence_start_gracefully_fails_on_out_of_bounds() {
        assert!(!is_sentence_start()
            .slice(Rope::from("Hello").byte_slice(..))
            .query_offset(1000)
            .call());
    }
}
