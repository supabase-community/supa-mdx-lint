use anyhow::{anyhow, Result};
use log::debug;
use markdown::{mdast::Node, to_mdast, Constructs, ParseOptions};
use std::any::Any;

use crate::document::AdjustedPoint;

type Frontmatter = Box<dyn Any>;

#[derive(Debug)]
pub struct ParseResult {
    pub ast: Node,
    pub frontmatter_lines: usize,
    pub frontmatter: Option<Frontmatter>,
}

pub fn parse(input: &str) -> Result<ParseResult> {
    let (frontmatter_lines, frontmatter, content) = extract_frontmatter(input);
    let ast = parse_internal(content)?;
    Ok(ParseResult {
        ast,
        frontmatter_lines,
        frontmatter,
    })
}

fn extract_frontmatter(input: &str) -> (usize, Option<Frontmatter>, &str) {
    let mut frontmatter = None;
    let mut content = input;

    let mut frontmatter_end = AdjustedPoint::default();

    if content.trim_start().starts_with("---") {
        let start_offset = content.find("---").unwrap() + 3;

        if let Some(end_offset) = content[start_offset..].find("---") {
            let mut end_offset = start_offset + end_offset;
            let frontmatter_str = content[start_offset..end_offset].to_string();

            if let Ok(toml_frontmatter) = toml::from_str::<toml::Value>(&frontmatter_str) {
                debug!("Parsed as TOML: {toml_frontmatter:?}");
                frontmatter = Some(Box::new(toml_frontmatter) as Frontmatter);
            } else if let Ok(yaml_frontmatter) =
                serde_yaml::from_str::<serde_yaml::Value>(&frontmatter_str)
            {
                debug!("Parsed as YAML: {yaml_frontmatter:?}");
                frontmatter = Some(Box::new(yaml_frontmatter) as Frontmatter);
            } else {
                debug!("Failed to parse frontmatter as TOML or YAML")
            }

            // If both parse attempts fail, frontmatter remains None

            // Update end_offset to include the closing "---" and following blank lines
            end_offset += 3; // Move past the closing "---"
            let remaining = &content[end_offset..];
            let mut newline_offset = 0;

            // Skip all whitespace and newlines after the closing "---"
            while newline_offset < remaining.len() {
                if remaining[newline_offset..].starts_with(|x: char| x == '\n' || x.is_whitespace())
                {
                    newline_offset += 1;
                } else {
                    break;
                }
            }

            end_offset += newline_offset;

            frontmatter_end =
                AdjustedPoint::new(content[..end_offset].lines().count() + 1, 1, end_offset);
        }
    }

    if frontmatter.is_some() {
        content = &input[frontmatter_end.offset..];
    }

    let frontmatter_lines: usize = if frontmatter.is_some() {
        frontmatter_end.line.get() - 1
    } else {
        0
    };

    (frontmatter_lines, frontmatter, content)
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
    .map_err(|e| anyhow!("Markdown parsing error: {:?}", e))?;

    Ok(mdast)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_markdown_without_frontmatter() {
        let input = r#"# Heading

Content here."#;
        let result = parse(input).unwrap();

        assert_eq!(result.frontmatter_lines, 0);
        assert!(result.frontmatter.is_none());

        let root = result.ast;
        let heading = root.children().unwrap().first().unwrap();
        assert_eq!(heading.position().unwrap().start.line, 1);
        assert_eq!(heading.position().unwrap().start.column, 1);
    }

    #[test]
    fn test_parse_markdown_with_yaml_frontmatter() {
        let input = r#"---
title: Test
---

# Heading

Content here."#;
        let result = parse(input).unwrap();

        assert_eq!(result.frontmatter_lines, 4);
        assert!(result.frontmatter.is_some());

        let frontmatter = result.frontmatter.unwrap();
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

        assert_eq!(result.frontmatter_lines, 6);
        assert!(result.frontmatter.is_some());

        let frontmatter = result.frontmatter.unwrap();
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
        assert_eq!(result.frontmatter_lines, 5);
        assert!(result.frontmatter.is_some());

        let root = result.ast;
        let heading = root.children().unwrap().first().unwrap();
        assert_eq!(heading.position().unwrap().start.line, 1);
        assert_eq!(heading.position().unwrap().start.column, 1);
    }
}
