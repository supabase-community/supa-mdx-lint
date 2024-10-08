use markdown::unist::{Point as UnistPoint, Position};
use serde::{Deserialize, Serialize};
use std::num::NonZeroUsize;
use tsify::Tsify;

use crate::rules::RuleContext;

/// A point in the source document, adjusted for frontmatter lines.
#[derive(Debug, Clone, Eq, PartialEq, Deserialize, Serialize, Tsify)]
#[tsify(into_wasm_abi)]
pub struct AdjustedPoint {
    /// 1-indexed
    pub line: NonZeroUsize,
    /// 1-indexed
    pub column: NonZeroUsize,
    /// 0-indexed
    pub offset: usize,
}

impl AdjustedPoint {
    pub fn new(line: usize, column: usize, offset: usize) -> Self {
        Self {
            line: NonZeroUsize::new(line).expect("Line numbers are 1-indexed"),
            column: NonZeroUsize::new(column).expect("Column numbers are 1-indexed"),
            offset,
        }
    }

    pub fn from_unadjusted_point(point: UnadjustedPoint, context: &RuleContext) -> Self {
        let mut this = Self::new(point.line.get(), point.column.get(), point.offset);
        this.add_lines(context.frontmatter_lines());
        this
    }
}

impl Default for AdjustedPoint {
    fn default() -> Self {
        Self::new(1, 1, 0)
    }
}

/// A point in the source document, not adjusted for frontmatter lines.
#[derive(Debug, Clone, Eq, PartialEq)]
pub struct UnadjustedPoint {
    /// 1-indexed
    pub line: NonZeroUsize,
    /// 1-indexed
    pub column: NonZeroUsize,
    /// 0-indexed
    pub offset: usize,
}

impl From<&UnistPoint> for UnadjustedPoint {
    fn from(point: &UnistPoint) -> Self {
        Self {
            line: NonZeroUsize::new(point.line).expect("Line numbers are 1-indexed"),
            column: NonZeroUsize::new(point.column).expect("Column numbers are 1-indexed"),
            offset: point.offset,
        }
    }
}

pub trait Point {
    fn column(&self) -> usize;
    fn offset(&self) -> usize;
    fn set_column(&mut self, column: usize);
    fn set_offset(&mut self, offset: usize);
    fn add_lines(&mut self, lines: usize);

    fn move_over_text(&mut self, text: &str) {
        let mut chars = text.chars().peekable();
        while let Some(c) = chars.next() {
            match c {
                '\r' => {
                    if chars.peek() == Some(&'\n') {
                        chars.next();
                        self.add_lines(1);
                        self.set_column(1);
                        self.set_offset(self.offset() + 2);
                    } else {
                        self.add_lines(1);
                        self.set_column(1);
                        self.set_offset(self.offset() + 1);
                    }
                }
                '\n' => {
                    self.add_lines(1);
                    self.set_column(1);
                    self.set_offset(self.offset() + 1);
                }
                _ => {
                    self.set_column(self.column() + 1);
                    self.set_offset(self.offset() + 1);
                }
            }
        }
    }
}

impl Point for AdjustedPoint {
    fn column(&self) -> usize {
        self.column.get()
    }

    fn offset(&self) -> usize {
        self.offset
    }

    fn set_column(&mut self, column: usize) {
        self.column = NonZeroUsize::new(column).expect("Column numbers are 1-indexed");
    }

    fn set_offset(&mut self, offset: usize) {
        self.offset = offset;
    }

    fn add_lines(&mut self, lines: usize) {
        self.line = NonZeroUsize::new(self.line.get() + lines)
            .expect("Line number after adding should be greater than 0");
    }
}

impl Point for UnadjustedPoint {
    fn column(&self) -> usize {
        self.column.get()
    }

    fn offset(&self) -> usize {
        self.offset
    }

    fn set_column(&mut self, column: usize) {
        self.column = NonZeroUsize::new(column).expect("Column numbers are 1-indexed");
    }

    fn set_offset(&mut self, offset: usize) {
        self.offset = offset;
    }

    fn add_lines(&mut self, lines: usize) {
        self.line = NonZeroUsize::new(self.line.get() + lines)
            .expect("Line number after adding should be greater than 0");
    }
}

#[derive(Debug, Clone, Deserialize, Serialize, Tsify)]
#[tsify(into_wasm_abi)]
pub struct Location {
    // Keep these fields private so that we can make sure MDAST positions are
    // adjusted for frontmatter lines before being used to create a Location.
    start: AdjustedPoint,
    /// Exclusive
    end: AdjustedPoint,
}

impl Location {
    pub fn start(&self) -> &AdjustedPoint {
        &self.start
    }

    pub fn end(&self) -> &AdjustedPoint {
        &self.end
    }

    pub fn from_unadjusted_points(
        start: UnadjustedPoint,
        end: UnadjustedPoint,
        context: &RuleContext,
    ) -> Self {
        let start = AdjustedPoint::from_unadjusted_point(start, context);
        let end = AdjustedPoint::from_unadjusted_point(end, context);
        Self { start, end }
    }

    pub fn from_position(position: &Position, context: &RuleContext) -> Self {
        let start = UnadjustedPoint::from(&position.start);
        let end = UnadjustedPoint::from(&position.end);
        let start = AdjustedPoint::from_unadjusted_point(start, context);
        let end = AdjustedPoint::from_unadjusted_point(end, context);
        Self { start, end }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_move_over_text_simple() {
        let mut point = UnadjustedPoint {
            line: NonZeroUsize::new(1).unwrap(),
            column: NonZeroUsize::new(1).unwrap(),
            offset: 0,
        };
        point.move_over_text("Hello");
        assert_eq!(point.line.get(), 1);
        assert_eq!(point.column.get(), 6);
        assert_eq!(point.offset, 5);
    }

    #[test]
    fn test_move_over_text_newline() {
        let mut point = UnadjustedPoint {
            line: NonZeroUsize::new(1).unwrap(),
            column: NonZeroUsize::new(1).unwrap(),
            offset: 0,
        };
        point.move_over_text("Hello\nWorld");
        assert_eq!(point.line.get(), 2);
        assert_eq!(point.column.get(), 6);
        assert_eq!(point.offset, 11);
    }

    #[test]
    fn test_move_over_text_carriage_return() {
        let mut point = UnadjustedPoint {
            line: NonZeroUsize::new(1).unwrap(),
            column: NonZeroUsize::new(1).unwrap(),
            offset: 0,
        };
        point.move_over_text("Hello\rWorld");
        assert_eq!(point.line.get(), 2);
        assert_eq!(point.column.get(), 6);
        assert_eq!(point.offset, 11);
    }

    #[test]
    fn test_move_over_text_crlf() {
        let mut point = UnadjustedPoint {
            line: NonZeroUsize::new(1).unwrap(),
            column: NonZeroUsize::new(1).unwrap(),
            offset: 0,
        };
        point.move_over_text("Hello\r\nWorld");
        assert_eq!(point.line.get(), 2);
        assert_eq!(point.column.get(), 6);
        assert_eq!(point.offset, 12);
    }

    #[test]
    fn test_move_over_text_multiple_lines() {
        let mut point = UnadjustedPoint {
            line: NonZeroUsize::new(1).unwrap(),
            column: NonZeroUsize::new(1).unwrap(),
            offset: 0,
        };
        point.move_over_text("Hello\nWorld\nRust");
        assert_eq!(point.line.get(), 3);
        assert_eq!(point.column.get(), 5);
        assert_eq!(point.offset, 16);
    }

    #[test]
    fn test_move_over_text_unicode() {
        let mut point = UnadjustedPoint {
            line: NonZeroUsize::new(1).unwrap(),
            column: NonZeroUsize::new(1).unwrap(),
            offset: 0,
        };
        point.move_over_text("Hello 🦀");
        assert_eq!(point.line.get(), 1);
        assert_eq!(point.column.get(), 8);
        assert_eq!(point.offset, 7);
    }

    #[test]
    fn test_move_over_text_empty() {
        let mut point = UnadjustedPoint {
            line: NonZeroUsize::new(1).unwrap(),
            column: NonZeroUsize::new(1).unwrap(),
            offset: 0,
        };
        point.move_over_text("");
        assert_eq!(point.line.get(), 1);
        assert_eq!(point.column.get(), 1);
        assert_eq!(point.offset, 0);
    }
}
