use std::num::NonZeroUsize;

use markdown::unist::Position;

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct Point {
    /// 1-indexed
    pub line: NonZeroUsize,
    /// 1-indexed
    pub column: NonZeroUsize,
    /// 0-indexed
    pub offset: usize,
}

impl Point {
    pub fn new(line: usize, column: usize, offset: usize) -> Self {
        Self {
            line: NonZeroUsize::new(line).expect("Line numbers are 1-indexed"),
            column: NonZeroUsize::new(column).expect("Column numbers are 1-indexed"),
            offset,
        }
    }

    pub fn add_lines(&mut self, lines: usize) {
        self.line = NonZeroUsize::new(self.line.get() + lines)
            .expect("Line number after adding should be greater than 0");
    }
}

impl Default for Point {
    fn default() -> Self {
        Self::new(1, 1, 0)
    }
}

impl From<&Position> for Point {
    fn from(position: &Position) -> Self {
        Self::new(
            position.start.line,
            position.start.column,
            position.start.offset,
        )
    }
}
