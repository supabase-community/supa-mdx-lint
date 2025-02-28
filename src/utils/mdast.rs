use markdown::{
    mdast::{MdxFlowExpression, Node},
    unist::Position,
};
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

pub(crate) trait MaybePosition {
    fn position(&self) -> Option<&Position>;
}

impl<T: MaybePosition> MaybePosition for &T {
    fn position(&self) -> Option<&Position> {
        (*self).position()
    }
}

impl MaybePosition for Node {
    fn position(&self) -> Option<&Position> {
        self.position()
    }
}

impl MaybePosition for MdxFlowExpression {
    fn position(&self) -> Option<&Position> {
        self.position.as_ref()
    }
}

pub(crate) trait VariantName {
    fn variant_name(&self) -> String;
}

impl<T: VariantName> VariantName for &T {
    fn variant_name(&self) -> String {
        (*self).variant_name()
    }
}

impl VariantName for Node {
    fn variant_name(&self) -> String {
        match self {
            Node::Root(_) => "Root".to_string(),
            Node::Blockquote(_) => "Blockquote".to_string(),
            Node::FootnoteDefinition(_) => "FootnoteDefinition".to_string(),
            Node::MdxJsxFlowElement(_) => "MdxJsxFlowElement".to_string(),
            Node::List(_) => "List".to_string(),
            Node::MdxjsEsm(_) => "MdxjsEsm".to_string(),
            Node::Toml(_) => "Toml".to_string(),
            Node::Yaml(_) => "Yaml".to_string(),
            Node::Break(_) => "Break".to_string(),
            Node::InlineCode(_) => "InlineCode".to_string(),
            Node::InlineMath(_) => "InlineMath".to_string(),
            Node::Delete(_) => "Delete".to_string(),
            Node::Emphasis(_) => "Emphasis".to_string(),
            Node::MdxTextExpression(_) => "MdxTextExpression".to_string(),
            Node::FootnoteReference(_) => "FootnoteReference".to_string(),
            Node::Html(_) => "Html".to_string(),
            Node::Image(_) => "Image".to_string(),
            Node::ImageReference(_) => "ImageReference".to_string(),
            Node::MdxJsxTextElement(_) => "MdxJsxTextElement".to_string(),
            Node::Link(_) => "Link".to_string(),
            Node::LinkReference(_) => "LinkReference".to_string(),
            Node::Strong(_) => "Strong".to_string(),
            Node::Text(_) => "Text".to_string(),
            Node::Code(_) => "Code".to_string(),
            Node::Math(_) => "Math".to_string(),
            Node::MdxFlowExpression(_) => "MdxFlowExpression".to_string(),
            Node::Heading(_) => "Heading".to_string(),
            Node::Table(_) => "Table".to_string(),
            Node::ThematicBreak(_) => "ThematicBreak".to_string(),
            Node::TableRow(_) => "TableRow".to_string(),
            Node::TableCell(_) => "TableCell".to_string(),
            Node::ListItem(_) => "ListItem".to_string(),
            Node::Definition(_) => "Definition".to_string(),
            Node::Paragraph(_) => "Paragraph".to_string(),
        }
    }
}

impl VariantName for MdxFlowExpression {
    fn variant_name(&self) -> String {
        "MdxFlowExpression".to_string()
    }
}
