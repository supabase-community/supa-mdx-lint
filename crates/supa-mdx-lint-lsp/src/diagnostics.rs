use lsp_types::{Diagnostic, DiagnosticSeverity, NumberOrString, Position, Range};
use supa_mdx_lint::{LintError, LintLevel, LintOutput};

/// Convert LintOutput to a list of LSP Diagnostics
pub fn to_lsp_diagnostics(output: &LintOutput) -> Vec<Diagnostic> {
    output
        .errors()
        .iter()
        .map(to_lsp_diagnostic)
        .collect()
}

/// Convert a single LintError to an LSP Diagnostic
pub fn to_lsp_diagnostic(error: &LintError) -> Diagnostic {
    Diagnostic {
        range: to_lsp_range(error),
        severity: Some(match error.level() {
            LintLevel::Error => DiagnosticSeverity::ERROR,
            LintLevel::Warning => DiagnosticSeverity::WARNING,
        }),
        code: Some(NumberOrString::String(error.rule_name().to_string())),
        source: Some("supa-mdx-lint".to_string()),
        message: error.message().to_string(),
        ..Default::default()
    }
}

/// Convert LintError location to LSP Range
///
/// Both the linter and LSP use 0-based lines and columns.
pub fn to_lsp_range(error: &LintError) -> Range {
    let start = Position {
        line: error.start_row() as u32,
        character: error.start_column() as u32,
    };
    let end = Position {
        line: error.end_row() as u32,
        character: error.end_column() as u32,
    };
    Range { start, end }
}

/// Check if a position is within a range
pub fn position_in_range(position: Position, range: Range) -> bool {
    if position.line < range.start.line || position.line > range.end.line {
        return false;
    }

    if position.line == range.start.line && position.character < range.start.character {
        return false;
    }

    if position.line == range.end.line && position.character >= range.end.character {
        return false;
    }

    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_position_in_range() {
        let range = Range {
            start: Position { line: 1, character: 5 },
            end: Position { line: 1, character: 10 },
        };

        // Before range
        assert!(!position_in_range(Position { line: 1, character: 4 }, range));
        // At start
        assert!(position_in_range(Position { line: 1, character: 5 }, range));
        // In middle
        assert!(position_in_range(Position { line: 1, character: 7 }, range));
        // At end (exclusive, so not in range)
        assert!(!position_in_range(Position { line: 1, character: 10 }, range));
        // After range
        assert!(!position_in_range(Position { line: 1, character: 11 }, range));
        // Wrong line
        assert!(!position_in_range(Position { line: 0, character: 7 }, range));
    }

    #[test]
    fn test_position_in_multiline_range() {
        let range = Range {
            start: Position { line: 1, character: 5 },
            end: Position { line: 3, character: 10 },
        };

        // Before start line
        assert!(!position_in_range(Position { line: 0, character: 7 }, range));
        // Start line, before start char
        assert!(!position_in_range(Position { line: 1, character: 4 }, range));
        // Start line, at start char
        assert!(position_in_range(Position { line: 1, character: 5 }, range));
        // Middle line
        assert!(position_in_range(Position { line: 2, character: 0 }, range));
        // End line, at end char (exclusive, so not in range)
        assert!(!position_in_range(Position { line: 3, character: 10 }, range));
        // End line, after end char
        assert!(!position_in_range(Position { line: 3, character: 11 }, range));
        // After end line
        assert!(!position_in_range(Position { line: 4, character: 0 }, range));
    }
}
