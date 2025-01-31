#![allow(dead_code)]

use std::collections::HashSet;

use bon::builder;
use regex_syntax::ast::{
    parse::Parser, Ast, ClassPerlKind, ClassSet, ClassSetItem, Concat, RepetitionKind,
};

use crate::utils::char_tree::CharNode;

/// Expand a regex pattern into a list of strings.
///
/// Because of the nature of regex and wanting to return (a) a finite result
/// in (b) some reasonable amount of time, the returned list of strings is _not_
/// exhaustive. Even for a valid and theoretically finite regex pattern, None
/// may be returned if a performant expansion is too difficult.
///
/// ```ignore
/// let result = expand_regex(r"test(s|ed)?");
/// assert_eq!(result, Some(vec!["test", "tests", "tested"]));
/// ```
#[builder]
pub fn expand_regex(
    regex: &str,
    /// Whether to trim non-alphabetic characters from beginning and end of
    /// expanded string. Defaults to true.
    trim_non_alphabetic: Option<bool>,
) -> Option<Vec<String>> {
    let trim_non_alphabetic = trim_non_alphabetic.unwrap_or(true);

    let ast = Parser::new().parse(regex).ok()?;
    expand_ast(&ast, trim_non_alphabetic)
}

fn expand_ast(ast: &Ast, trim_non_alphabetic: bool) -> Option<Vec<String>> {
    #[derive(Debug)]
    enum NextNode {
        Single(CharNode),
        Multiple(Vec<CharNode>),
    }

    fn expand_ast_internal(ast: &Ast, char_tree: &mut Option<CharNode>) -> Option<NextNode> {
        match ast {
            Ast::Assertion(_) => Some(NextNode::Single(
                char_tree.clone().unwrap_or_else(CharNode::initiate),
            )),
            Ast::Literal(literal) => match char_tree {
                Some(ref mut node) => {
                    let new_node = node.append(literal.c);
                    Some(NextNode::Single(new_node))
                }
                None => {
                    let new_tree = char_tree.insert(CharNode::initiate());
                    let new_node = new_tree.append(literal.c);
                    Some(NextNode::Single(new_node))
                }
            },
            Ast::ClassBracketed(class_bracketed) if !class_bracketed.negated => {
                let new_nodes = expand_class_set(char_tree, &class_bracketed.kind);
                new_nodes.map(NextNode::Multiple)
            }
            Ast::Repetition(repetition)
                if matches!(repetition.op.kind, RepetitionKind::ZeroOrOne) =>
            {
                let mut tree = char_tree.get_or_insert_with(CharNode::initiate).clone();
                if !tree.is_root() {
                    tree.mark_finished_word();
                }

                let mut next_nodes = vec![tree.clone()];

                let alt_branch = expand_ast_internal(repetition.ast.as_ref(), char_tree);
                match alt_branch {
                    Some(NextNode::Single(node)) if node != tree => {
                        next_nodes.push(node);
                    }
                    Some(NextNode::Multiple(nodes)) => {
                        for node in nodes.into_iter() {
                            if node != tree {
                                next_nodes.push(node);
                            }
                        }
                    }
                    _ => {}
                }

                Some(NextNode::Multiple(next_nodes))
            }
            Ast::Group(group) => expand_ast_internal(group.ast.as_ref(), char_tree),
            Ast::Alternation(alternation) => {
                let mut next = Vec::new();

                for ast in alternation.asts.iter() {
                    match expand_ast_internal(ast, char_tree) {
                        Some(NextNode::Single(node)) => next.push(node),
                        Some(NextNode::Multiple(nodes)) => next.extend(nodes),
                        _ => {}
                    }
                }

                Some(NextNode::Multiple(next))
            }
            Ast::Concat(concat) => expand_concat(char_tree, concat),
            Ast::ClassPerl(perl_class)
                if perl_class.kind == ClassPerlKind::Space && !perl_class.negated =>
            {
                Some(NextNode::Single(
                    char_tree.clone().unwrap_or_else(CharNode::initiate),
                ))
            }
            _ => {
                // Too complex to list all the possibilities, just abort
                if let Some(ref mut node) = char_tree {
                    node.abort();
                }
                None
            }
        }
    }

    fn expand_concat(char_tree: &mut Option<CharNode>, concat: &Concat) -> Option<NextNode> {
        let tree = char_tree.get_or_insert_with(CharNode::initiate).clone();
        let mut next_node = Some(NextNode::Single(tree));

        for ast in concat.asts.iter() {
            match next_node {
                Some(NextNode::Single(node)) => {
                    let mut node = Some(node);
                    next_node = expand_ast_internal(ast, &mut node);
                }
                Some(NextNode::Multiple(nodes)) => {
                    let mut next = Vec::new();

                    nodes.into_iter().for_each(|node| {
                        let mut node = Some(node);
                        match expand_ast_internal(ast, &mut node) {
                            Some(NextNode::Single(node)) => next.push(node),
                            Some(NextNode::Multiple(nodes)) => next.extend(nodes),
                            _ => {}
                        }
                    });

                    next_node = Some(NextNode::Multiple(next));
                }
                _ => {}
            }
        }

        next_node
    }

    fn expand_class_set(
        char_tree: &mut Option<CharNode>,
        class_set: &ClassSet,
    ) -> Option<Vec<CharNode>> {
        let mut result = None::<Vec<CharNode>>;

        match class_set {
            ClassSet::Item(ClassSetItem::Literal(literal)) => {
                let tree = char_tree.get_or_insert_with(CharNode::initiate);
                let new_node = tree.append(literal.c);
                result.get_or_insert_with(Vec::new).push(new_node);
            }
            ClassSet::Item(ClassSetItem::Union(union)) => {
                for item in union.items.iter() {
                    let class_set = ClassSet::Item(item.clone());
                    if let Some(new_nodes) = expand_class_set(char_tree, &class_set) {
                        result.get_or_insert_with(Vec::new).extend(new_nodes);
                    }
                }
            }
            _ => {
                // Too complex to list all the possibilities, just abort
                if let Some(ref mut node) = char_tree {
                    node.abort();
                }
            }
        }

        result
    }

    let mut char_tree = None::<CharNode>;
    expand_ast_internal(ast, &mut char_tree);
    char_tree.map(|tree| {
        tree.collect()
            .into_iter()
            .map(|s| {
                if trim_non_alphabetic {
                    s.trim_matches(|c: char| !c.is_alphabetic()).to_string()
                } else {
                    s
                }
            })
            .collect::<HashSet<_>>()
            .into_iter()
            .collect::<Vec<_>>()
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_expand_regex_blank_returns_none() {
        let result = expand_regex().regex("").call();
        assert_eq!(result, None);
    }

    #[test]
    fn test_expand_regex_literal_into_itself() {
        let result = expand_regex().regex("test").call();
        assert_eq!(result, Some(vec!["test".to_string()]));
    }

    #[test]
    fn test_expand_regex_alternates() {
        let mut result = expand_regex().regex("test(s|ed)").call().unwrap();
        result.sort();
        assert_eq!(result, vec!["tested".to_string(), "tests".to_string()]);
    }

    #[test]
    fn test_expand_regex_optional() {
        let mut result = expand_regex().regex("tests?").call().unwrap();
        result.sort();
        assert_eq!(result, vec!["test".to_string(), "tests".to_string()]);
    }

    #[test]
    fn test_expand_regex_alternates_optional() {
        let mut result = expand_regex().regex("test(s|ed)?").call().unwrap();
        result.sort();
        assert_eq!(
            result,
            vec![
                "test".to_string(),
                "tested".to_string(),
                "tests".to_string(),
            ]
        );
    }

    #[test]
    fn test_expand_regex_alternates_class_set() {
        let mut result = expand_regex().regex("[Aa]pple").call().unwrap();
        result.sort();
        assert_eq!(result, vec!["Apple".to_string(), "apple".to_string()]);
    }

    #[test]
    fn text_expand_regex_initial_optional() {
        let mut result = expand_regex().regex("(pre)?determine").call().unwrap();
        result.sort();
        assert_eq!(
            result,
            vec!["determine".to_string(), "predetermine".to_string()]
        )
    }

    #[test]
    fn test_expand_regex_aborted_case() {
        let result = expand_regex().regex("[^Aa]pple").call().unwrap();
        assert_eq!(result, Vec::<String>::new());

        let result = expand_regex().regex("a[^Aa]pple").call().unwrap();
        assert_eq!(result, Vec::<String>::new());
    }

    #[test]
    fn test_expand_regex_trim_non_alphabetic() {
        let result = expand_regex().regex(" test ").call();
        assert_eq!(result, Some(vec!["test".to_string()]));

        let result = expand_regex().regex("!test!").call();
        assert_eq!(result, Some(vec!["test".to_string()]));

        let result = expand_regex()
            .regex("!test!")
            .trim_non_alphabetic(false)
            .call();
        assert_eq!(result, Some(vec!["!test!".to_string()]));
    }

    #[test]
    fn test_expand_regex_deduplication() {
        let result = expand_regex().regex("test|test").call().unwrap();
        assert_eq!(result, vec!["test".to_string()]);
    }

    #[test]
    fn test_expand_regex_with_assertions() {
        let result = expand_regex().regex("^test$").call().unwrap();
        assert_eq!(result, vec!["test"]);
    }

    #[test]
    fn test_expand_regex_with_perl_classes() {
        let result = expand_regex().regex("\\stest\\s").call().unwrap();
        assert_eq!(result, vec!["test"]);
    }
}
