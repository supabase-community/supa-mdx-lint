use std::any::Any;

use anyhow::{anyhow, Result};
use log::{debug, trace};
use markdown::{mdast::Node, to_mdast, Constructs, ParseOptions};

use crate::{geometry::AdjustedOffset, rope::Rope};

type Frontmatter = Box<dyn Any>;

#[derive(Debug)]
pub(crate) struct ParseMetadata {
    content_start_offset: AdjustedOffset,
    #[allow(unused)]
    frontmatter: Option<Frontmatter>,
}

#[derive(Debug)]
pub(crate) struct ParseResult {
    ast: Node,
    rope: Rope,
    metadata: ParseMetadata,
}

impl ParseResult {
    pub(crate) fn ast(&self) -> &Node {
        &self.ast
    }

    pub(crate) fn rope(&self) -> &Rope {
        &self.rope
    }

    pub(crate) fn content_start_offset(&self) -> AdjustedOffset {
        self.metadata.content_start_offset
    }
}

pub(crate) fn parse(input: &str) -> Result<ParseResult> {
    let (content, rope, content_start_offset, frontmatter) = process_raw_content_string(input);
    let ast = parse_internal(content)?;

    trace!("AST: {:#?}", ast);

    Ok(ParseResult {
        ast,
        rope,
        metadata: ParseMetadata {
            content_start_offset,
            frontmatter,
        },
    })
}

fn process_raw_content_string(input: &str) -> (&str, Rope, AdjustedOffset, Option<Frontmatter>) {
    let rope = Rope::from(input);
    let mut frontmatter = None;
    let mut content = input;

    let mut content_start_offset = AdjustedOffset::default();

    if content.trim_start().starts_with("---") {
        let frontmatter_start_offset: AdjustedOffset = (content.find("---").unwrap() + 3).into();

        if let Some(frontmatter_end_index) = content[frontmatter_start_offset.into()..].find("---")
        {
            let mut end_offset: AdjustedOffset =
                (Into::<usize>::into(frontmatter_start_offset) + frontmatter_end_index).into();

            let frontmatter_str = &content[frontmatter_start_offset.into()..end_offset.into()];

            if let Ok(toml_frontmatter) = toml::from_str::<toml::Value>(frontmatter_str) {
                debug!("Parsed as TOML: {toml_frontmatter:#?}");
                frontmatter = Some(Box::new(toml_frontmatter) as Frontmatter);
            } else if let Ok(yaml_frontmatter) =
                serde_yaml::from_str::<serde_yaml::Value>(frontmatter_str)
            {
                debug!("Parsed as YAML: {yaml_frontmatter:#?}");
                frontmatter = Some(Box::new(yaml_frontmatter) as Frontmatter);
            } else {
                debug!("Failed to parse frontmatter as TOML or YAML: {frontmatter_str}")
            }

            // Update end_offset to include the closing "---" and following blank lines

            // Move past the closing "---"
            end_offset.increment(3);

            // Skip all whitespace and newlines after the closing "---"
            let mut remaining_index = 0;
            let remaining = &content[end_offset.into()..];
            while remaining_index < remaining.len() {
                if remaining[remaining_index..].starts_with(char::is_whitespace) {
                    remaining_index += 1;
                } else {
                    break;
                }
            }
            end_offset.increment(remaining_index);

            content_start_offset = end_offset;
        }
    }

    content = &input[content_start_offset.into()..];

    (content, rope, content_start_offset, frontmatter)
}

fn parse_internal(input: &str) -> Result<Node> {
    let mdast = to_mdast(
        input,
        &ParseOptions {
            constructs: Constructs {
                autolink: false,
                code_indented: false,
                frontmatter: true,
                gfm_footnote_definition: true,
                gfm_label_start_footnote: true,
                gfm_table: true,
                html_flow: false,
                html_text: false,
                mdx_esm: true,
                mdx_expression_flow: true,
                mdx_expression_text: true,
                mdx_jsx_flow: true,
                mdx_jsx_text: true,
                ..Default::default()
            },
            ..Default::default()
        },
    )
    .map_err(|e| anyhow!("Not valid Markdown: {:?}", e))?;

    Ok(mdast)
}

pub(crate) trait CommentString {
    fn is_comment(&self) -> bool;
    fn as_comment(&self) -> Option<&str>;
}

impl CommentString for str {
    fn is_comment(&self) -> bool {
        let trimmed = self.trim();
        trimmed.starts_with("/*") && trimmed.ends_with("*/")
    }

    fn as_comment(&self) -> Option<&str> {
        let trimmed = self.trim();
        if !self.is_comment() {
            return None;
        }

        Some(
            trimmed
                .trim_start_matches("/*")
                .trim_end_matches("*/")
                .trim(),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_markdown_without_frontmatter() {
        let input = r#"# Heading

Content here."#;
        let result = parse(input).unwrap();

        assert_eq!(
            result.metadata.content_start_offset,
            AdjustedOffset::from(0)
        );
        assert!(result.metadata.frontmatter.is_none());

        let root = result.ast;
        let heading = root.children().unwrap().first().unwrap();
        assert_eq!(heading.position().unwrap().start.line, 1);
        assert_eq!(heading.position().unwrap().start.column, 1);
        assert_eq!(heading.position().unwrap().start.offset, 0);
    }

    #[test]
    fn test_parse_markdown_with_yaml_frontmatter() {
        let input = r#"---
title: Test
---

# Heading

Content here."#;
        let result = parse(input).unwrap();

        assert_eq!(
            result.metadata.content_start_offset,
            AdjustedOffset::from(21)
        );
        assert!(result.metadata.frontmatter.is_some());

        let frontmatter = result.metadata.frontmatter.unwrap();
        let yaml = frontmatter.downcast_ref::<serde_yaml::Value>().unwrap();
        if let serde_yaml::Value::Mapping(map) = yaml {
            assert_eq!(map.len(), 1);
            assert!(map.contains_key(&serde_yaml::Value::String("title".to_string())));
        } else {
            panic!("Expected YAML frontmatter to be a mapping");
        }

        let root = result.ast;
        let heading = root.children().unwrap().first().unwrap();
        assert_eq!(heading.position().unwrap().start.line, 1);
        assert_eq!(heading.position().unwrap().start.column, 1);
    }

    #[test]
    fn test_parse_markdown_with_toml_frontmatter() {
        let input = r#"---
title = "TOML Test"
[author]
name = "John Doe"
---

# TOML Heading

Content with TOML frontmatter."#;
        let result = parse(input).unwrap();

        assert_eq!(
            result.metadata.content_start_offset,
            AdjustedOffset::from(56)
        );
        assert!(result.metadata.frontmatter.is_some());

        let frontmatter = result.metadata.frontmatter.unwrap();
        let toml = frontmatter.downcast_ref::<toml::Value>().unwrap();

        assert!(toml.is_table());
        let table = toml.as_table().unwrap();

        assert!(table.contains_key("title"));

        let root = result.ast;
        let heading = root.children().unwrap().first().unwrap();
        assert_eq!(heading.position().unwrap().start.line, 1);
        assert_eq!(heading.position().unwrap().start.column, 1);
    }

    #[test]
    fn test_parse_markdown_with_frontmatter_and_multiple_newlines() {
        let input = r#"---
title: Test
---


# Heading

Content here."#;
        let result = parse(input).unwrap();
        assert_eq!(
            result.metadata.content_start_offset,
            AdjustedOffset::from(22)
        );
        assert!(result.metadata.frontmatter.is_some());

        let root = result.ast;
        let heading = root.children().unwrap().first().unwrap();
        assert_eq!(heading.position().unwrap().start.line, 1);
        assert_eq!(heading.position().unwrap().start.column, 1);
    }
}
