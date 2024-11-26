use markdown::mdast::Node;
use regex::Regex;

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

/// For some reason, `export const .* =` is parsed as a Text node. We need to
/// this out to prevent running lints on it.
pub(crate) fn is_export_const(node: &markdown::mdast::Node) -> bool {
    match node {
        markdown::mdast::Node::Text(text) => {
            let regex = Regex::new(r"^export\s+const\s+[a-zA-Z0-9_$-]+\s+=").unwrap();
            regex.is_match(&text.value)
        }
        _ => false,
    }
}
