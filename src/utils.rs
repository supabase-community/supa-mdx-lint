use std::{num::NonZeroUsize, path::Path};

use markdown::mdast::Node;

pub fn set_panic_hook() {
    #[cfg(feature = "console_error_panic_hook")]
    console_error_panic_hook::set_once();
}

#[macro_export]
macro_rules! log {
    ( $( $t:tt )* ) => {
        web_sys::console::log_1(&format!( $( $t )* ).into());
    }
}

pub trait HasChildren {
    fn get_children(&self) -> &Vec<Node>;
}

impl HasChildren for markdown::mdast::Heading {
    fn get_children(&self) -> &Vec<Node> {
        &self.children
    }
}

impl HasChildren for markdown::mdast::Strong {
    fn get_children(&self) -> &Vec<Node> {
        &self.children
    }
}

impl HasChildren for markdown::mdast::Emphasis {
    fn get_children(&self) -> &Vec<Node> {
        &self.children
    }
}

impl HasChildren for markdown::mdast::LinkReference {
    fn get_children(&self) -> &Vec<Node> {
        &self.children
    }
}

impl HasChildren for markdown::mdast::Link {
    fn get_children(&self) -> &Vec<Node> {
        &self.children
    }
}

pub fn split_first_word_at_whitespace_and_colons(s: &str) -> (&str, &str, bool) {
    let next_whitespace = s.find(char::is_whitespace);
    let next_colon = s.find(':');
    match (next_whitespace, next_colon) {
        (Some(idx), None) => (&s[..idx], &s[idx..], false),
        (None, Some(idx)) => {
            if s[idx + 1..].starts_with(char::is_whitespace) {
                (&s[..idx], &s[idx..], true)
            } else {
                (s, "", false)
            }
        }
        (None, None) => (s, "", false),
        (Some(idx_whitespace), Some(idx_colon)) => {
            if idx_whitespace < idx_colon || !s[idx_colon + 1..].starts_with(char::is_whitespace) {
                (&s[..idx_whitespace], &s[idx_whitespace..], false)
            } else {
                (&s[..idx_colon], &s[idx_colon..], true)
            }
        }
    }
}

pub fn is_lintable(path: impl AsRef<Path>) -> bool {
    let path = path.as_ref();
    path.is_dir() || path.extension().map_or(false, |ext| ext == "mdx")
}

pub trait NonZeroLineRange {
    fn start_line(&self) -> NonZeroUsize;
    fn end_line(&self) -> Option<NonZeroUsize>;
}
