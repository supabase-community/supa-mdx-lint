mod char_tree;
pub(crate) mod lru;
pub(crate) mod mdast;
pub(crate) mod path;
pub(crate) mod regex;
pub(crate) mod words;

use std::borrow::Cow;

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

pub(crate) fn pluralize(num: usize) -> &'static str {
    if num == 1 {
        ""
    } else {
        "s"
    }
}

pub(crate) fn escape_backticks(s: &str) -> Cow<'_, str> {
    if s.contains('`') {
        Cow::Owned(s.replace('`', "\\`"))
    } else {
        Cow::Borrowed(s)
    }
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
