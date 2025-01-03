#![allow(dead_code)]
// The [mutable key lint](https://rust-lang.github.io/rust-clippy/master/index.html#mutable_key_type)
// has known false positives when dealing with a struct that has only partial
// interior mutability. In this module, CharNode has a children field that needs
// interior mutability to build the tree, but it's hashed on the value field,
// so the lint can be ignored.
#![allow(clippy::mutable_key_type)]

use std::{
    cell::RefCell,
    collections::HashSet,
    hash::{Hash, Hasher},
    rc::Rc,
};

#[derive(Debug, Eq, PartialEq, Hash)]
enum NodeValue {
    Initial,
    Char(char),
    /// The path leading up to this node is not a valid word and should be
    /// abandoned
    Abort,
    /// The path leading up to this node is a complete word in and of itself
    Finish,
}

#[derive(Debug)]
struct CharNodeInner {
    value: NodeValue,
    children: RefCell<Option<HashSet<CharNode>>>,
}

#[derive(Debug, Clone)]
pub(super) struct CharNode(Rc<CharNodeInner>);

impl PartialEq for CharNode {
    fn eq(&self, other: &Self) -> bool {
        self.0.value == other.0.value
    }
}

impl Eq for CharNode {}

impl Hash for CharNode {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.0.value.hash(state);
    }
}

impl CharNode {
    pub(super) fn initiate() -> Self {
        Self(Rc::new(CharNodeInner {
            value: NodeValue::Initial,
            children: RefCell::new(None),
        }))
    }

    fn new(value: char) -> Self {
        Self(Rc::new(CharNodeInner {
            value: NodeValue::Char(value),
            children: RefCell::new(None),
        }))
    }

    fn new_abort() -> Self {
        Self(Rc::new(CharNodeInner {
            value: NodeValue::Abort,
            children: RefCell::new(None),
        }))
    }

    fn new_finish() -> Self {
        Self(Rc::new(CharNodeInner {
            value: NodeValue::Finish,
            children: RefCell::new(None),
        }))
    }

    fn add_child(&mut self, child: CharNode) {
        let mut children = self.0.children.borrow_mut();
        let children_set = children.get_or_insert_with(HashSet::new);
        children_set.insert(child);
    }

    pub(super) fn is_root(&self) -> bool {
        self.0.value == NodeValue::Initial
    }

    pub(super) fn append(&mut self, value: char) -> Self {
        let new_child = Self::new(value);
        self.add_child(new_child.clone());
        new_child
    }

    pub(super) fn abort(&mut self) {
        let new_child = Self::new_abort();
        self.add_child(new_child);
    }

    pub(super) fn mark_finished_word(&mut self) {
        let new_child = Self::new_finish();
        self.add_child(new_child);
    }

    pub(super) fn collect(&self) -> Vec<String> {
        fn traverse(node: &CharNode, prefix: &str, result: &mut Vec<String>) {
            match node.0.value {
                NodeValue::Initial => {
                    if let Some(children) = &*node.0.children.borrow() {
                        let new_prefix = "";
                        for child in children {
                            traverse(child, new_prefix, result);
                        }
                    }
                }
                NodeValue::Char(value) => {
                    let new_prefix = format!("{}{}", prefix, value);
                    if let Some(children) = &*node.0.children.borrow() {
                        for child in children {
                            if child.0.value == NodeValue::Finish {
                                result.push(new_prefix.clone());
                            } else {
                                traverse(child, &new_prefix, result);
                            }
                        }
                    } else {
                        result.push(new_prefix);
                    }
                }
                _ => {}
            }
        }

        let mut result = Vec::new();
        traverse(self, "", &mut result);
        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_char_tree_collect() {
        let mut root = CharNode::initiate();
        let mut child1 = root.append('a');
        let mut child2 = child1.append('b');
        child1.append('c');
        child2.append('d');

        let collected = child1.collect();
        assert_eq!(collected.len(), 2);
        assert!(collected.contains(&"abd".to_string()));
        assert!(collected.contains(&"ac".to_string()));
    }

    #[test]
    fn test_char_tree_collect_with_aborts() {
        let mut root = CharNode::initiate();
        let mut child1 = root.append('a');
        let mut child2 = child1.append('b');
        child1.append('c');
        child2.append('d');
        let mut aborted_child = child2.append('e');
        aborted_child.abort();

        let collected = child1.collect();
        assert_eq!(collected.len(), 2);
        assert!(collected.contains(&"ac".to_string()));
        assert!(collected.contains(&"abd".to_string()));
    }

    #[test]
    fn test_char_tree_with_mid_path_finish() {
        let mut root = CharNode::initiate();
        let mut child1 = root.append('a');
        let mut child2 = child1.append('b');
        child2.mark_finished_word();
        child2.append('c');

        let collected = child1.collect();
        assert_eq!(collected.len(), 2);
        assert!(collected.contains(&"ab".to_string()));
        assert!(collected.contains(&"abc".to_string()));
    }
}
