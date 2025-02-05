use std::ops::{Deref, DerefMut};

/// This is publicly exposed because we need it for the interactive fixing
/// feature, but should _not_ be considered part of the public API. There are
/// no guarantees about the stability of this type and its methods.
#[doc(hidden)]
#[derive(Debug, Default, Clone, Eq, PartialEq)]
pub struct Rope(crop::Rope);

/// This is publicly exposed because we need it for the interactive fixing
/// feature, but should _not_ be considered part of the public API. There are
/// no guarantees about the stability of this type and its methods.
#[doc(hidden)]
pub use crop::RopeSlice;

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
    pub fn line_column_of_byte(&self, byte_offset: usize) -> (usize, usize) {
        self.byte_slice(..).line_column_of_byte(byte_offset)
    }
}

/// This is publicly exposed because we need it for the interactive fixing
/// feature, but should _not_ be considered part of the public API. There are
/// no guarantees about the stability of this type and its methods.
#[doc(hidden)]
pub trait RopeSliceExt {
    fn eq_str(&self, s: &str) -> bool;
    fn line_column_of_byte(&self, byte_offset: usize) -> (usize, usize);
}

impl RopeSliceExt for RopeSlice<'_> {
    fn eq_str(&self, s: &str) -> bool {
        let mut this = self.bytes();
        let mut s = s.as_bytes().iter();

        loop {
            match (this.next(), s.next()) {
                (Some(this_byte), Some(s_byte)) => {
                    if this_byte != *s_byte {
                        return false;
                    }
                    continue;
                }
                (None, None) => return true,
                _ => return false,
            }
        }
    }

    fn line_column_of_byte(&self, byte_offset: usize) -> (usize, usize) {
        let line = self.line_of_byte(byte_offset);
        let start_of_line = self.byte_of_line(line);
        let column = byte_offset - start_of_line;
        (line, column)
    }
}

#[cfg(test)]
mod tests {
    use crate::rope::{Rope, RopeSliceExt as _};

    #[test]
    fn test_eq_str() {
        let rope = Rope::from("hello world");
        assert!(rope.byte_slice(0..5).eq_str("hello"));
        assert!(rope.byte_slice(6..11).eq_str("world"));
        assert!(rope.byte_slice(..).eq_str("hello world"));
        assert!(!rope.byte_slice(0..4).eq_str("hello"));
        assert!(!rope.byte_slice(0..5).eq_str("world"));
        assert!(!rope.byte_slice(6..11).eq_str("hello worlds"));
    }
}
