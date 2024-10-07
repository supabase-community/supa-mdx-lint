use markdown::mdast::{Code, InlineCode, InlineMath, Math, MdxTextExpression, Node, Text};

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

pub fn get_text_content(node: &Node) -> String {
    match node {
        Node::Text(Text { value, .. }) => value.clone(),
        Node::Code(Code { value, .. }) => value.clone(),
        Node::InlineCode(InlineCode { value, .. }) => value.clone(),
        Node::Root(root) => root.children.iter().map(get_text_content).collect(),
        Node::Paragraph(paragraph) => paragraph.children.iter().map(get_text_content).collect(),
        Node::Heading(heading) => heading.children.iter().map(get_text_content).collect(),
        Node::List(list) => list.children.iter().map(get_text_content).collect(),
        Node::ListItem(list_item) => list_item.children.iter().map(get_text_content).collect(),
        Node::Blockquote(blockquote) => blockquote.children.iter().map(get_text_content).collect(),
        Node::Link(link) => link.children.iter().map(get_text_content).collect(),
        Node::Emphasis(emphasis) => emphasis.children.iter().map(get_text_content).collect(),
        Node::Strong(strong) => strong.children.iter().map(get_text_content).collect(),
        Node::FootnoteDefinition(footnote_definition) => footnote_definition
            .children
            .iter()
            .map(get_text_content)
            .collect(),
        Node::MdxJsxFlowElement(mdx_jsx_flow_element) => mdx_jsx_flow_element
            .children
            .iter()
            .map(get_text_content)
            .collect(),
        Node::InlineMath(InlineMath { value, .. }) => value.clone(),
        Node::MdxTextExpression(MdxTextExpression { value, .. }) => value.clone(),
        Node::MdxJsxTextElement(mdx_jsx_text_element) => mdx_jsx_text_element
            .children
            .iter()
            .map(get_text_content)
            .collect(),
        Node::LinkReference(link_reference) => link_reference
            .children
            .iter()
            .map(get_text_content)
            .collect(),
        Node::Math(Math { value, .. }) => value.clone(),
        _ => String::new(),
    }
}

pub fn split_first_word(s: &str) -> (&str, &str) {
    match s.find(char::is_whitespace) {
        Some(idx) => (&s[..idx], &s[idx..]),
        None => (s, ""),
    }
}

#[cfg(test)]
mod tests {
    use crate::parser::parse;

    use super::*;

    #[test]
    fn test_get_text_content_on_heading() {
        let ast = parse(r#"# Heading"#).unwrap().ast;
        let text_content = get_text_content(&ast);
        assert_eq!(text_content, "Heading");

        let ast = parse(r#"# Heading [#special-id]"#).unwrap().ast;
        let text_content = get_text_content(&ast);
        assert_eq!(text_content, "Heading [#special-id]");

        let ast = parse(r#"# Heading `with some code`"#).unwrap().ast;
        let text_content = get_text_content(&ast);
        assert_eq!(text_content, "Heading with some code");

        let ast = parse(r#"# Heading **with bold**"#).unwrap().ast;
        let text_content = get_text_content(&ast);
        assert_eq!(text_content, "Heading with bold");

        let ast = parse(r#"# **Heading**"#).unwrap().ast;
        let text_content = get_text_content(&ast);
        assert_eq!(text_content, "Heading");

        let ast = parse(r#"# **Head**ing"#).unwrap().ast;
        let text_content = get_text_content(&ast);
        assert_eq!(text_content, "Heading");
    }
}
