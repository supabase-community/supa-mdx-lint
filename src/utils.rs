mod char_tree;
pub(crate) mod mdast;
pub(crate) mod path;
pub(crate) mod regex;
pub(crate) mod words;

use std::path::Path;

pub fn is_lintable(path: impl AsRef<Path>) -> bool {
    let path = path.as_ref();
    path.is_dir() || path.extension().map_or(false, |ext| ext == "mdx")
}
