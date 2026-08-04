use markdown::mdast::Node;
use supa_mdx_macros::RuleName;

use crate::{
    context::Context,
    errors::{LintError, LintLevel},
};

use super::{Rule, RuleName, RuleSettings};

const MESSAGE: &str =
    "Admonitions cannot contain headings. Use the `title` prop for a callout title, or move the heading and section content outside the Admonition.";

/// Admonitions are callouts and asides, not document sections.
///
/// Use the `title` prop for an optional callout title. Move content that needs
/// structural headings into the surrounding document.
#[derive(Debug, Default, RuleName)]
pub struct Rule007NoHeadingsInAdmonitions;

impl Rule for Rule007NoHeadingsInAdmonitions {
    fn default_level(&self) -> LintLevel {
        LintLevel::Error
    }

    fn setup(&mut self, _settings: Option<&mut RuleSettings>) {}

    fn check(&self, ast: &Node, context: &Context, level: LintLevel) -> Option<Vec<LintError>> {
        let Node::MdxJsxFlowElement(element) = ast else {
            return None;
        };

        if !element
            .name
            .as_ref()
            .is_some_and(|name| name == "Admonition")
        {
            return None;
        }

        let mut headings = Vec::new();
        for child in &element.children {
            collect_heading_nodes(child, &mut headings);
        }

        let errors = headings
            .into_iter()
            .filter_map(|heading| {
                LintError::from_node()
                    .node(heading)
                    .context(context)
                    .rule(self.name())
                    .level(level)
                    .message(MESSAGE)
                    .call()
            })
            .collect::<Vec<_>>();

        (!errors.is_empty()).then_some(errors)
    }
}

fn collect_heading_nodes<'node>(node: &'node Node, headings: &mut Vec<&'node Node>) {
    if is_heading(node) {
        headings.push(node);
    }

    // Let a nested Admonition report its own headings to avoid duplicate errors.
    if is_admonition(node) {
        return;
    }

    if let Some(children) = node.children() {
        for child in children {
            collect_heading_nodes(child, headings);
        }
    }
}

fn is_heading(node: &Node) -> bool {
    match node {
        Node::Heading(_) => true,
        Node::MdxJsxFlowElement(element) => element.name.as_deref().is_some_and(is_html_heading),
        Node::MdxJsxTextElement(element) => element.name.as_deref().is_some_and(is_html_heading),
        _ => false,
    }
}

fn is_admonition(node: &Node) -> bool {
    matches!(
        node,
        Node::MdxJsxFlowElement(element)
            if element.name.as_ref().is_some_and(|name| name == "Admonition")
    )
}

fn is_html_heading(name: &str) -> bool {
    matches!(name, "h1" | "h2" | "h3" | "h4" | "h5" | "h6")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{context::Context, parser::parse};

    fn check(mdx: &str) -> Option<Vec<LintError>> {
        let parse_result = parse(mdx).unwrap();
        let context = Context::builder()
            .parse_result(&parse_result)
            .build()
            .unwrap();
        let admonition = context
            .parse_result
            .ast()
            .children()
            .unwrap()
            .iter()
            .find(|node| {
                matches!(
                    node,
                    Node::MdxJsxFlowElement(element)
                        if element.name.as_ref().is_some_and(|name| name == "Admonition")
                )
            })
            .unwrap();

        Rule007NoHeadingsInAdmonitions.check(admonition, &context, LintLevel::Error)
    }

    #[test]
    fn rejects_all_markdown_heading_levels() {
        for level in 1..=6 {
            let mdx = format!(
                "<Admonition type=\"note\">\n\n{} Title\n\nBody.\n\n</Admonition>",
                "#".repeat(level)
            );
            let errors = check(&mdx).unwrap();

            assert_eq!(errors.len(), 1, "expected h{level} to be rejected");
            assert_eq!(errors[0].location.start.row, 2);
            assert_eq!(errors[0].location.start.column, 0);
            assert_eq!(errors[0].message(), MESSAGE);
            assert!(errors[0].fix.is_none());
        }
    }

    #[test]
    fn rejects_all_jsx_heading_elements() {
        for level in 1..=6 {
            let mdx = format!(
                "<Admonition type=\"note\">\n\n<h{level}>Title</h{level}>\n\n</Admonition>"
            );
            let errors = check(&mdx).unwrap();

            assert_eq!(errors.len(), 1, "expected <h{level}> to be rejected");
            assert_eq!(errors[0].location.start.row, 2);
            assert_eq!(errors[0].location.start.column, 0);
        }
    }

    #[test]
    fn rejects_nested_headings() {
        let errors = check(
            r#"<Admonition type="note">

<div>

### Nested title

</div>

</Admonition>"#,
        )
        .unwrap();

        assert_eq!(errors.len(), 1);
        assert_eq!(errors[0].location.start.row, 4);
    }

    #[test]
    fn allows_title_and_rich_body_content() {
        let result = check(
            r#"<Admonition type="note" title="Optional title">

Body copy with [a link](/docs), `inline code`, and a list:

- First item
- Second item

</Admonition>"#,
        );

        assert!(result.is_none());
    }

    #[test]
    fn allows_heading_syntax_in_code_fences() {
        let result = check(
            r#"<Admonition type="note">

```md
### Example heading
```

</Admonition>"#,
        );

        assert!(result.is_none());
    }

    #[test]
    fn ignores_headings_outside_admonitions() {
        let mdx = r#"## Document section

<Admonition type="note">

Body copy.

</Admonition>"#;

        assert!(check(mdx).is_none());
    }

    #[test]
    fn reports_each_heading_in_an_admonition() {
        let errors = check(
            r#"<Admonition type="note">

## First heading

### Second heading

</Admonition>"#,
        )
        .unwrap();

        assert_eq!(errors.len(), 2);
        assert_eq!(errors[0].location.start.row, 2);
        assert_eq!(errors[1].location.start.row, 4);
    }
}
