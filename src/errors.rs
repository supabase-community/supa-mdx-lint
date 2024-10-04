use crate::document::Point;

#[derive(Debug)]
pub struct LintError {
    message: String,
    location: Point,
}

pub enum FixType {
    Insert(LintFixInsert),
    Delete(LintFixDelete),
    Replace(LintFixReplace),
}

pub struct LintFixInsert {
    location: Point,
    text: String,
}

pub struct LintFixDelete {
    start_location: Point,
    /// Exclusive
    end_location: Point,
}

pub struct LintFixReplace {
    start_location: Point,
    /// Exclusive
    end_location: Point,
    text: String,
}

impl LintError {
    pub fn new(message: String, line: usize, column: usize, offset: usize) -> Self {
        Self {
            message,
            location: Point::new(line, column, offset),
        }
    }
}
