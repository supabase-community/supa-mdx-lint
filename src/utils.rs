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

pub fn split_first_word(s: &str) -> (&str, &str) {
    match s.find(char::is_whitespace) {
        Some(idx) => (&s[..idx], &s[idx..]),
        None => (s, ""),
    }
}
