# Creating New Lint Rules in supa-mdx-lint

This document explains how to create new lint rules for the supa-mdx-lint tool, which is a Rust-based MDX linter designed to enforce the Supabase Docs style guide.

## Architecture Overview

The lint rule system is built around a trait-based architecture where each rule implements the `Rule` trait defined in `src/rules.rs`. Rules are automatically registered and run against parsed MDX AST nodes.

### Core Components

- **Rule Trait**: Defines the interface all rules must implement
- **RuleName Derive Macro**: Auto-generates rule names from struct names
- **Rule Registry**: Manages rule lifecycle and execution
- **Configuration System**: Handles rule-specific settings via TOML

## Step-by-Step Guide

### 1. Create the Rule File

Create a new file in `src/rules/` following the naming convention `rule[NUMBER]_[description].rs`:

```rust
// src/rules/rule005_my_new_rule.rs
use markdown::mdast::Node;
use supa_mdx_macros::RuleName;

use crate::{
    context::Context,
    errors::{LintError, LintLevel},
};

use super::{Rule, RuleName, RuleSettings};

/// Brief description of what this rule checks.
///
/// ## Examples
///
/// ### Valid
/// ```markdown
/// # Example of valid content
/// ```
///
/// ### Invalid
/// ```markdown
/// # Example of invalid content
/// ```
#[derive(Debug, Default, RuleName)]
pub struct Rule005MyNewRule {
    // Rule-specific configuration fields
}

impl Rule for Rule005MyNewRule {
    fn default_level(&self) -> LintLevel {
        LintLevel::Error  // or LintLevel::Warn
    }

    fn setup(&mut self, settings: Option<&mut RuleSettings>) {
        // Optional: Configure rule from TOML settings
        if let Some(settings) = settings {
            // Extract configuration values
        }
    }

    fn check(&self, ast: &Node, context: &Context, level: LintLevel) -> Option<Vec<LintError>> {
        // Early return if this node type isn't relevant
        if !matches!(ast, Node::YourTargetNodeType(_)) {
            return None;
        }

        // Implement your rule logic here
        // Return Some(vec![error]) if rule is violated, None otherwise
        None
    }
}
```

### 2. Register the Rule

Add the new rule to the rules module:

**In `src/rules.rs`:**

```rust
// Add module declaration
mod rule005_my_new_rule;

// Add public use statement
pub use rule005_my_new_rule::Rule005MyNewRule;

// Add to get_all_rules() function
fn get_all_rules() -> Vec<Box<dyn Rule>> {
    vec![
        // ... existing rules
        Box::new(Rule005MyNewRule::default()),
    ]
}
```

### 3. Implement Rule Logic

Rules typically follow these patterns:

#### Pattern 1: Simple Node Type Check
```rust
fn check(&self, ast: &Node, context: &Context, level: LintLevel) -> Option<Vec<LintError>> {
    match ast {
        Node::Heading(heading) => {
            // Check heading-specific logic
            if violates_rule(heading) {
                return Some(vec![create_error(ast, context, level)]);
            }
        }
        _ => return None,
    }
    None
}
```

#### Pattern 2: Text Content Analysis
```rust
fn check(&self, ast: &Node, context: &Context, level: LintLevel) -> Option<Vec<LintError>> {
    if let Node::Text(text) = ast {
        if let Some(position) = text.position.as_ref() {
            let range = AdjustedRange::from_unadjusted_position(position, context);
            let rope = context.rope().byte_slice(Into::<Range<usize>>::into(range));
            
            // Analyze text content
            if rope.to_string().contains("problematic_pattern") {
                return Some(vec![create_error(ast, context, level)]);
            }
        }
    }
    None
}
```

#### Pattern 3: Recursive AST Traversal
```rust
fn check(&self, ast: &Node, context: &Context, level: LintLevel) -> Option<Vec<LintError>> {
    let mut errors = Vec::new();
    
    // Check current node
    if let Some(node_errors) = check_current_node(ast, context, level) {
        errors.extend(node_errors);
    }
    
    // Traverse children
    if let Some(children) = ast.children() {
        for child in children {
            if let Some(child_errors) = self.check(child, context, level) {
                errors.extend(child_errors);
            }
        }
    }
    
    if errors.is_empty() { None } else { Some(errors) }
}
```

### 4. Error Creation

Use the builder pattern to create lint errors:

```rust
fn create_error(&self, node: &Node, context: &Context, level: LintLevel) -> LintError {
    LintError::from_node()
        .node(node)
        .context(context)
        .rule(self.name())
        .level(level)
        .message("Description of the violation")
        .call()
        .unwrap()
}
```

For auto-fixes, add corrections:

```rust
use crate::fix::{LintCorrection, LintCorrectionReplace};

let fix = LintCorrection::Replace(LintCorrectionReplace {
    location: error_location,
    text: "corrected_text".to_string(),
});

LintError::from_node()
    .node(node)
    .context(context)
    .rule(self.name())
    .level(level)
    .message("Error message")
    .fix(vec![fix])
    .call()
```

### 5. Configuration Support

To support TOML configuration:

```rust
#[derive(Debug, Default, RuleName)]
pub struct Rule005MyNewRule {
    allowed_patterns: Vec<Regex>,
    max_length: Option<usize>,
}

impl Rule for Rule005MyNewRule {
    fn setup(&mut self, settings: Option<&mut RuleSettings>) {
        if let Some(settings) = settings {
            // For regex arrays
            if let Some(patterns) = settings.get_array_of_regexes("allowed_patterns", None) {
                self.allowed_patterns = patterns;
            }
            
            // For simple values
            if let Some(length) = settings.get_deserializable::<usize>("max_length") {
                self.max_length = Some(length);
            }
        }
    }
}
```

Configuration in `supa-mdx-lint.config.toml`:
```toml
[Rule005MyNewRule]
allowed_patterns = ["pattern1", "pattern2"]
max_length = 100
```

### 6. Testing

Create comprehensive tests:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::{parser::parse, context::Context};

    #[test]
    fn test_rule_passes_valid_content() {
        let rule = Rule005MyNewRule::default();
        let mdx = "# Valid content";
        let parse_result = parse(mdx).unwrap();
        let context = Context::builder()
            .parse_result(&parse_result)
            .build()
            .unwrap();

        let result = rule.check(
            parse_result.ast().children().unwrap().first().unwrap(),
            &context,
            LintLevel::Error,
        );
        assert!(result.is_none());
    }

    #[test]
    fn test_rule_fails_invalid_content() {
        let rule = Rule005MyNewRule::default();
        let mdx = "# Invalid content";
        let parse_result = parse(mdx).unwrap();
        let context = Context::builder()
            .parse_result(&parse_result)
            .build()
            .unwrap();

        let result = rule.check(
            parse_result.ast().children().unwrap().first().unwrap(),
            &context,
            LintLevel::Error,
        );
        assert!(result.is_some());
        
        let errors = result.unwrap();
        assert_eq!(errors.len(), 1);
        assert_eq!(errors[0].message, "Expected error message");
    }
}
```

## Key Patterns and Conventions

### Rule Naming
- Files: `rule[NUMBER]_[description].rs` (e.g., `rule001_heading_case.rs`)
- Structs: `Rule[NUMBER][CamelCase]` (e.g., `Rule001HeadingCase`)
- Numbers are assigned sequentially starting from 001

### Documentation
- Include comprehensive doc comments with examples
- Show both valid and invalid cases
- Document configuration options

### Error Levels
- Use `LintLevel::Error` for style violations that should block publishing
- Use `LintLevel::Warn` for suggestions or less critical issues

### Performance
- Return early from `check()` if the node type isn't relevant
- Cache expensive operations when possible
- Use the rope data structure for efficient text operations

### Configuration
- Use descriptive configuration key names
- Provide sensible defaults
- Support regex patterns for flexible matching

## Integration Testing

Add test files in `tests/rule[NUMBER]/`:
- `mod.rs` - Test module
- `rule[NUMBER].mdx` - Test content
- `supa-mdx-lint.config.toml` - Rule configuration

## Best Practices

1. **Single Responsibility**: Each rule should check one specific style guideline
2. **Performance**: Minimize AST traversal and text processing
3. **Configurability**: Make rules configurable where it makes sense
4. **Error Messages**: Provide clear, actionable error messages
5. **Auto-fixes**: Implement fixes for mechanical violations when possible
6. **Testing**: Cover edge cases and different node types
7. **Documentation**: Include examples and configuration documentation

This architecture provides a robust foundation for extending the linter with new rules while maintaining consistency and performance.