mod char_tree;
pub(crate) mod lru;
pub(crate) mod mdast;
pub(crate) mod path;
pub(crate) mod regex;
pub(crate) mod words;

use std::path::Path;

pub fn is_lintable(path: impl AsRef<Path>) -> bool {
    let path = path.as_ref();
    path.is_dir() || path.extension().map_or(false, |ext| ext == "mdx")
}

pub trait Offsets {
    fn start(&self) -> usize;
    fn end(&self) -> usize;
}

impl<T: Offsets> Offsets for &T {
    fn start(&self) -> usize {
        (*self).start()
    }

    fn end(&self) -> usize {
        (*self).end()
    }
}

pub(crate) fn num_digits(n: usize) -> usize {
    if n == 0 {
        return 1;
    }

    let mut count = 0;
    let mut num = n;

    while num > 0 {
        count += 1;
        num /= 10;
    }

    count
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_num_digits() {
        assert_eq!(num_digits(0), 1);
        assert_eq!(num_digits(1), 1);
        assert_eq!(num_digits(10), 2);
        assert_eq!(num_digits(1000), 4);
        assert_eq!(num_digits(8730240234), 10);
    }
}
