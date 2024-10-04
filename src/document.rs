use std::num::NonZeroUsize;

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
}

impl Default for Point {
    fn default() -> Self {
        Self::new(1, 1, 0)
    }
}
