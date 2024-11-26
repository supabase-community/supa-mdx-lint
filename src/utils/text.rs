use crop::RopeSlice;

pub(crate) enum LetterCase {
    Lower,
    Upper,
}

pub fn is_recapitalizing_break(c: char) -> bool {
    c == ':' || c == '.' || c == '!' || c == '?'
}

pub fn split_first_word_at_break(s: &str) -> (&str, &str, bool) {
    let next_whitespace = s.find(char::is_whitespace);
    let next_recapitalizing_break = s.find(is_recapitalizing_break);
    match (next_whitespace, next_recapitalizing_break) {
        (Some(idx), None) => (&s[..idx], &s[idx..], false),
        (None, Some(idx)) => {
            if s[idx + 1..].starts_with(char::is_whitespace) {
                (&s[..idx], &s[idx..], true)
            } else {
                (s, "", false)
            }
        }
        (None, None) => (s, "", false),
        (Some(idx_whitespace), Some(idx_colon)) => {
            if idx_whitespace < idx_colon || !s[idx_colon + 1..].starts_with(char::is_whitespace) {
                (&s[..idx_whitespace], &s[idx_whitespace..], false)
            } else {
                (&s[..idx_colon], &s[idx_colon..], true)
            }
        }
    }
}

/// An iterator over the words in a RopeSlice. Also returns the byte offset of
/// the start of the word, relative to the parent Rope.
///
/// The iterator elides emojis, but preserves non-ASCII alphanumeric
/// characters, such as CJK and accented characters.
#[derive(Debug, Clone)]
pub(crate) struct WordTokenIterator<'rope> {
    offset_in_parent: usize,
    rope: RopeSlice<'rope>,
    curr_line: Option<CurrLine<'rope>>,
}

#[derive(Debug, Clone)]
struct CurrLine<'rope> {
    line_num: usize,
    line: RopeSlice<'rope>,
    offset: usize,
    split_on_hyphens: bool,
}

enum IncrementCurrLineResult {
    LineIncremented,
    AlreadyAtEnd,
}

impl<'rope> CurrLine<'rope> {
    fn increment_line(&mut self, rope: RopeSlice<'rope>) -> IncrementCurrLineResult {
        if self.line_num == rope.line_len() - 1 {
            return IncrementCurrLineResult::AlreadyAtEnd;
        }

        self.line_num += 1;
        self.line = rope.line(self.line_num);
        self.offset = 0;
        IncrementCurrLineResult::LineIncremented
    }
}

impl<'slice> Iterator for CurrLine<'slice> {
    // usize represents the byte offset within the current line
    type Item = (usize, RopeSlice<'slice>);

    fn next(&mut self) -> Option<Self::Item> {
        if self.offset >= self.line.byte_len() {
            return None;
        }

        assert!(
            self.line.is_char_boundary(self.offset),
            "Offset into current line is not at a character boundary"
        );

        let is_word_boundary = if self.split_on_hyphens {
            |c: char| !c.is_alphanumeric() && c != '_'
        } else {
            |c: char| !c.is_alphanumeric() && c != '_' && c != '-'
        };

        let mut start_offset = self.offset;
        let mut remaining = self.line.byte_slice(start_offset..).chars().peekable();
        while let Some(c) = remaining.next() {
            if is_word_boundary(c) && (c == '\'' || c == '’') && start_offset != self.offset {
                log::trace!("Checking apostrophe for contraction or possessive");
                match remaining.peek() {
                    Some(&'t') => {
                        remaining.next();
                        self.offset += c.len_utf8();
                        self.offset += 1;

                        match remaining.peek() {
                            Some(d) if d.is_alphanumeric() => {
                                // This was not a possessive, backtrack.
                                self.offset -= c.len_utf8();
                                self.offset -= 1;
                                break;
                            }
                            None | Some(_) => {
                                break;
                            }
                        }
                    }
                    Some(&'s') => {
                        remaining.next();
                        self.offset += c.len_utf8();
                        self.offset += 1;

                        match remaining.peek() {
                            Some(d) if d.is_alphanumeric() => {
                                // This was not a possessive, backtrack.
                                self.offset -= c.len_utf8();
                                self.offset -= 1;
                                break;
                            }
                            None | Some(_) => {
                                break;
                            }
                        }
                    }
                    Some(&'v') => {
                        remaining.next();
                        self.offset += c.len_utf8();
                        self.offset += 1;

                        match remaining.peek() {
                            Some('e') => {
                                remaining.next();
                                self.offset += 1;

                                match remaining.peek() {
                                    Some(d) if d.is_alphanumeric() => {
                                        // This was not a contraction, backtrack.
                                        self.offset -= c.len_utf8();
                                        self.offset -= 2;
                                        break;
                                    }
                                    None | Some(_) => {
                                        break;
                                    }
                                }
                            }
                            _ => {
                                // This was not a contraction, backtrack.
                                self.offset -= c.len_utf8();
                                self.offset -= 1;
                                break;
                            }
                        }
                    }
                    Some(&'l') => {
                        remaining.next();
                        self.offset += c.len_utf8();
                        self.offset += 1;

                        match remaining.peek() {
                            Some('l') => {
                                remaining.next();
                                self.offset += 1;

                                match remaining.peek() {
                                    Some(d) if d.is_alphanumeric() => {
                                        // This was not a contraction, backtrack.
                                        self.offset -= c.len_utf8();
                                        self.offset -= 2;
                                        break;
                                    }
                                    None | Some(_) => {
                                        break;
                                    }
                                }
                            }
                            _ => {
                                // This was not a contraction, backtrack.
                                self.offset -= c.len_utf8();
                                self.offset -= 1;
                                break;
                            }
                        }
                    }
                    _ => {}
                }
            }

            if is_word_boundary(c) && start_offset == self.offset {
                start_offset += c.len_utf8();
                self.offset += c.len_utf8();
            } else if is_word_boundary(c) && start_offset != self.offset {
                break;
            } else {
                self.offset += c.len_utf8();
            }
        }

        if start_offset == self.offset {
            None
        } else {
            Some((
                start_offset,
                self.line.byte_slice(start_offset..self.offset),
            ))
        }
    }
}

impl<'rope> WordTokenIterator<'rope> {
    pub(crate) fn new(
        rope: RopeSlice<'rope>,
        offset_in_parent: usize,
        split_on_hyphens: bool,
    ) -> Self {
        let curr_line = if rope.is_empty() {
            None
        } else {
            let line = rope.line(0);
            Some(CurrLine {
                line_num: 0,
                line,
                offset: 0,
                split_on_hyphens,
            })
        };

        Self {
            offset_in_parent,
            rope,
            curr_line,
        }
    }
}

impl<'rope> Iterator for WordTokenIterator<'rope> {
    // usize represents the byte offset within the parent
    type Item = (usize, RopeSlice<'rope>);

    fn next(&mut self) -> Option<Self::Item> {
        self.read_next_word()
    }
}

impl<'rope> WordTokenIterator<'rope> {
    fn read_next_word(&mut self) -> Option<(usize, RopeSlice<'rope>)> {
        match self.curr_line.as_mut() {
            None => None,
            Some(curr_line) => match curr_line.next() {
                Some((offset_in_line, word)) => {
                    let line_start = self.rope.byte_of_line(curr_line.line_num);
                    Some((self.offset_in_parent + line_start + offset_in_line, word))
                }
                None => match curr_line.increment_line(self.rope) {
                    IncrementCurrLineResult::LineIncremented => self.read_next_word(),
                    IncrementCurrLineResult::AlreadyAtEnd => {
                        self.curr_line = None;
                        None
                    }
                },
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::rope::Rope;

    use super::*;

    #[test]
    fn test_iterate_words_from_empty_rope() {
        let rope = Rope::from("");
        let mut iter = WordTokenIterator::new(rope.byte_slice(..), 0, false);
        assert_eq!(iter.next(), None);
    }

    #[test]
    fn test_iterate_words_from_single_word() {
        let rope = Rope::from("hello");
        let mut iter = WordTokenIterator::new(rope.byte_slice(..), 0, false);
        assert_eq!(
            iter.next().map(|(offset, s)| (offset, s.to_string())),
            Some((0, "hello".to_string()))
        );
        assert_eq!(iter.next(), None);
    }

    #[test]
    fn test_iterate_words_from_multiple_words() {
        let rope = Rope::from("hello world");
        let mut iter = WordTokenIterator::new(rope.byte_slice(..), 0, false);
        assert_eq!(
            iter.next().map(|(offset, s)| (offset, s.to_string())),
            Some((0, "hello".to_string()))
        );
        assert_eq!(
            iter.next().map(|(offset, s)| (offset, s.to_string())),
            Some((6, "world".to_string()))
        );
        assert_eq!(iter.next(), None);
    }

    #[test]
    fn test_iterate_words_from_multiple_lines() {
        let rope = Rope::from("hello\nworld");
        let mut iter = WordTokenIterator::new(rope.byte_slice(..), 0, false);
        assert_eq!(
            iter.next().map(|(offset, s)| (offset, s.to_string())),
            Some((0, "hello".to_string()))
        );
        assert_eq!(
            iter.next().map(|(offset, s)| (offset, s.to_string())),
            Some((6, "world".to_string()))
        );
        assert_eq!(iter.next(), None);
    }

    #[test]
    fn test_iterate_words_from_hyphenated_words() {
        let rope = Rope::from("well-known word");
        let mut iter = WordTokenIterator::new(rope.byte_slice(..), 0, false);
        assert_eq!(
            iter.next().map(|(offset, s)| (offset, s.to_string())),
            Some((0, "well-known".to_string()))
        );
        assert_eq!(
            iter.next().map(|(offset, s)| (offset, s.to_string())),
            Some((11, "word".to_string()))
        );
        assert_eq!(iter.next(), None);
    }

    #[test]
    fn test_iterate_words_from_punctuation() {
        let rope = Rope::from("hello, world!");
        let mut iter = WordTokenIterator::new(rope.byte_slice(..), 0, false);
        assert_eq!(
            iter.next().map(|(offset, s)| (offset, s.to_string())),
            Some((0, "hello".to_string()))
        );
        assert_eq!(
            iter.next().map(|(offset, s)| (offset, s.to_string())),
            Some((7, "world".to_string()))
        );
        assert_eq!(iter.next(), None);
    }

    #[test]
    fn test_iterate_over_words_with_emoji() {
        let rope = Rope::from("hello 👋 world");
        let mut iter = WordTokenIterator::new(rope.byte_slice(..), 0, false);
        assert_eq!(
            iter.next().map(|(offset, s)| (offset, s.to_string())),
            Some((0, "hello".to_string()))
        );
        assert_eq!(
            iter.next().map(|(offset, s)| (offset, s.to_string())),
            Some((11, "world".to_string()))
        );
        assert_eq!(iter.next(), None);
    }

    #[test]
    fn test_iterate_over_words_with_cjk() {
        let rope = Rope::from("你好 world");
        let mut iter = WordTokenIterator::new(rope.byte_slice(..), 0, false);
        assert_eq!(
            iter.next().map(|(offset, s)| (offset, s.to_string())),
            Some((0, "你好".to_string()))
        );
        assert_eq!(
            iter.next().map(|(offset, s)| (offset, s.to_string())),
            Some((7, "world".to_string()))
        );
        assert_eq!(iter.next(), None);
    }

    #[test]
    fn test_iterate_over_words_with_accented_characters() {
        let rope = Rope::from("à world");
        let mut iter = WordTokenIterator::new(rope.byte_slice(..), 0, false);
        assert_eq!(
            iter.next().map(|(offset, s)| (offset, s.to_string())),
            Some((0, "à".to_string()))
        );
        assert_eq!(
            iter.next().map(|(offset, s)| (offset, s.to_string())),
            Some((3, "world".to_string()))
        );
        assert_eq!(iter.next(), None);
    }

    #[test]
    fn test_iterate_over_words_with_numerals() {
        let rope = Rope::from("123 world");
        let mut iter = WordTokenIterator::new(rope.byte_slice(..), 0, false);
        assert_eq!(
            iter.next().map(|(offset, s)| (offset, s.to_string())),
            Some((0, "123".to_string()))
        );
        assert_eq!(
            iter.next().map(|(offset, s)| (offset, s.to_string())),
            Some((4, "world".to_string()))
        );
        assert_eq!(iter.next(), None);
    }

    #[test]
    fn test_iterate_over_words_with_apostrophes() {
        let rope = Rope::from("hello's world");
        let mut iter = WordTokenIterator::new(rope.byte_slice(..), 0, false);
        assert_eq!(
            iter.next().map(|(offset, s)| (offset, s.to_string())),
            Some((0, "hello's".to_string()))
        );
        assert_eq!(
            iter.next().map(|(offset, s)| (offset, s.to_string())),
            Some((8, "world".to_string()))
        );
        assert_eq!(iter.next(), None);
    }

    #[test]
    fn test_iterate_over_words_with_non_possessive_apostrophes() {
        let rope = Rope::from("hello 'world'");
        let mut iter = WordTokenIterator::new(rope.byte_slice(..), 0, false);
        assert_eq!(
            iter.next().map(|(offset, s)| (offset, s.to_string())),
            Some((0, "hello".to_string()))
        );
        assert_eq!(
            iter.next().map(|(offset, s)| (offset, s.to_string())),
            Some((7, "world".to_string()))
        );
        assert_eq!(iter.next(), None);
    }

    #[test]
    fn test_iterate_over_words_with_apostrophe_contractions() {
        let rope = Rope::from("they'll say don't you haven't you");
        let mut iter = WordTokenIterator::new(rope.byte_slice(..), 0, false);
        assert_eq!(
            iter.next().map(|(offset, s)| (offset, s.to_string())),
            Some((0, "they'll".to_string()))
        );
        iter.next();
        assert_eq!(
            iter.next().map(|(offset, s)| (offset, s.to_string())),
            Some((12, "don't".to_string()))
        );
        iter.next();
        assert_eq!(
            iter.next().map(|(offset, s)| (offset, s.to_string())),
            Some((22, "haven't".to_string()))
        );
        iter.next();
        assert_eq!(iter.next(), None);
    }

    #[test]
    fn test_iterate_over_words_with_parent_offset() {
        let rope = Rope::from("hello world");
        let mut iter = WordTokenIterator::new(rope.byte_slice(5..), 5, false);
        assert_eq!(
            iter.next().map(|(offset, s)| (offset, s.to_string())),
            Some((6, "world".to_string()))
        );
        assert_eq!(iter.next(), None);
    }

    #[test]
    fn test_iterate_over_words_with_parent_offset_and_line_break() {
        let rope = Rope::from("hello\nhola\nnihao\nworld");
        let mut iter = WordTokenIterator::new(rope.byte_slice(8..), 8, false);
        assert_eq!(
            iter.next().map(|(offset, s)| (offset, s.to_string())),
            Some((8, "la".to_string()))
        );
        assert_eq!(
            iter.next().map(|(offset, s)| (offset, s.to_string())),
            Some((11, "nihao".to_string()))
        );
        assert_eq!(
            iter.next().map(|(offset, s)| (offset, s.to_string())),
            Some((17, "world".to_string()))
        );
        assert_eq!(iter.next(), None);
    }

    #[test]
    fn test_iterate_over_words_splitting_on_hyphens() {
        let rope = Rope::from("hello-world");
        let mut iter = WordTokenIterator::new(rope.byte_slice(..), 0, true);
        assert_eq!(
            iter.next().map(|(offset, s)| (offset, s.to_string())),
            Some((0, "hello".to_string()))
        );
        assert_eq!(
            iter.next().map(|(offset, s)| (offset, s.to_string())),
            Some((6, "world".to_string()))
        );
        assert_eq!(iter.next(), None);
    }
}
