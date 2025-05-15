mod char_tree;
pub(crate) mod lru;
pub(crate) mod mdast;
pub(crate) mod path;
pub(crate) mod regex;
pub(crate) mod words;

use std::{
    borrow::Cow,
    path::{Path, PathBuf},
};

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

// https://stackoverflow.com/questions/39340924/given-two-absolute-paths-how-can-i-express-one-of-the-paths-relative-to-the-oth
pub(crate) fn path_relative_from(path: &Path, base: &Path) -> Option<PathBuf> {
    use std::path::Component;

    if path.is_absolute() != base.is_absolute() {
        if path.is_absolute() {
            Some(PathBuf::from(path))
        } else {
            None
        }
    } else {
        let mut ita = path.components();
        let mut itb = base.components();
        let mut comps: Vec<Component> = vec![];
        loop {
            match (ita.next(), itb.next()) {
                (None, None) => break,
                (Some(a), None) => {
                    comps.push(a);
                    comps.extend(ita.by_ref());
                    break;
                }
                (None, _) => comps.push(Component::ParentDir),
                (Some(a), Some(b)) if comps.is_empty() && a == b => (),
                (Some(a), Some(Component::CurDir)) => comps.push(a),
                (Some(_), Some(Component::ParentDir)) => return None,
                (Some(a), Some(_)) => {
                    comps.push(Component::ParentDir);
                    for _ in itb {
                        comps.push(Component::ParentDir);
                    }
                    comps.push(a);
                    comps.extend(ita.by_ref());
                    break;
                }
            }
        }
        Some(comps.iter().map(|c| c.as_os_str()).collect())
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

    #[test]
    fn test_path_relative_from() {
        let path = Path::new("/foo/bar/baz");
        let base = Path::new("/foo/qux");
        assert_eq!(
            path_relative_from(path, base).unwrap(),
            PathBuf::from("../bar/baz")
        );

        let path = Path::new("/foo/bar/baz");
        let base = Path::new("/foo/bar");
        assert_eq!(
            path_relative_from(path, base).unwrap(),
            PathBuf::from("baz")
        );

        let path = Path::new("/foo/bar");
        let base = Path::new("/foo/bar/baz");
        assert_eq!(path_relative_from(path, base).unwrap(), PathBuf::from(".."));

        let path = Path::new("/foo/qux/xyz");
        let base = Path::new("/foo/bar/baz");
        assert_eq!(
            path_relative_from(path, base).unwrap(),
            PathBuf::from("../../qux/xyz")
        );

        let path = Path::new("/foo/bar");
        let base = Path::new("/qux/xyz");
        assert_eq!(
            path_relative_from(path, base).unwrap(),
            PathBuf::from("../../foo/bar")
        );
    }
}
