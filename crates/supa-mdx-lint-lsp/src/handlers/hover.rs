use lsp_types::{Hover, HoverContents, HoverParams, MarkupContent, MarkupKind};

use crate::diagnostics::{position_in_range, to_lsp_range};
use crate::state::ServerState;

/// Handle textDocument/hover request
pub fn handle_hover(state: &ServerState, params: HoverParams) -> Option<Hover> {
    let uri = &params.text_document_position_params.text_document.uri;
    let position = params.text_document_position_params.position;

    let doc = state.documents.get(uri)?;

    // Find diagnostic at this position
    for output in &doc.lint_output {
        for error in output.errors() {
            let range = to_lsp_range(error);
            if position_in_range(position, range) {
                let contents = format!("**{}**\n\n{}", error.rule_name(), error.message());

                return Some(Hover {
                    contents: HoverContents::Markup(MarkupContent {
                        kind: MarkupKind::Markdown,
                        value: contents,
                    }),
                    range: Some(range),
                });
            }
        }
    }

    None
}
