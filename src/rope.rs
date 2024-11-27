use std::ops::{Deref, DerefMut};

#[derive(Debug, Default, Clone, Eq, PartialEq)]
pub(crate) struct Rope(crop::Rope);

impl Deref for Rope {
    type Target = crop::Rope;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl DerefMut for Rope {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

impl From<crop::Rope> for Rope {
    fn from(rope: crop::Rope) -> Self {
        Self(rope)
    }
}

impl From<Rope> for crop::Rope {
    fn from(rope: Rope) -> Self {
        rope.0
    }
}

impl From<&str> for Rope {
    fn from(s: &str) -> Self {
        Self(crop::Rope::from(s))
    }
}

impl From<String> for Rope {
    fn from(s: String) -> Self {
        Self(crop::Rope::from(s))
    }
}

impl Rope {
    pub(crate) fn line_column_of_byte(&self, byte_offset: usize) -> (usize, usize) {
        let line = self.line_of_byte(byte_offset);
        let start_of_line = self.byte_of_line(line);
        let column = byte_offset - start_of_line;
        (line, column)
    }
}
