use log::{debug, warn};
use lsp_types::{
    DidChangeTextDocumentParams, DidCloseTextDocumentParams, DidOpenTextDocumentParams,
    DidSaveTextDocumentParams, PublishDiagnosticsParams, Uri,
};
use supa_mdx_lint::LintTarget;

use crate::config::LintOn;
use crate::diagnostics::to_lsp_diagnostics;
use crate::state::{DocumentState, ServerState};

/// Handle textDocument/didOpen notification
pub fn handle_did_open(
    state: &mut ServerState,
    params: DidOpenTextDocumentParams,
) -> Option<PublishDiagnosticsParams> {
    let uri = params.text_document.uri;
    let content = params.text_document.text;

    debug!("Document opened: {:?}", uri);

    // Store document
    state.documents.insert(
        uri.clone(),
        DocumentState {
            content,
            lint_output: vec![],
        },
    );

    // Always lint on open
    lint_document(state, &uri)
}

/// Handle textDocument/didChange notification
pub fn handle_did_change(
    state: &mut ServerState,
    params: DidChangeTextDocumentParams,
) -> Option<PublishDiagnosticsParams> {
    let uri = params.text_document.uri;

    if let Some(doc) = state.documents.get_mut(&uri) {
        // Full sync, so take the whole content
        if let Some(change) = params.content_changes.into_iter().next() {
            doc.content = change.text;
        }
    }

    // Only lint on change if lintOn == Type
    if state.settings.lint_on == LintOn::Type {
        lint_document(state, &uri)
    } else {
        None
    }
}

/// Handle textDocument/didSave notification
pub fn handle_did_save(
    state: &mut ServerState,
    params: DidSaveTextDocumentParams,
) -> Option<PublishDiagnosticsParams> {
    // Only lint on save if lintOn == Save
    if state.settings.lint_on == LintOn::Save {
        lint_document(state, &params.text_document.uri)
    } else {
        None
    }
}

/// Handle textDocument/didClose notification
pub fn handle_did_close(state: &mut ServerState, params: DidCloseTextDocumentParams) {
    debug!("Document closed: {:?}", params.text_document.uri);
    state.documents.remove(&params.text_document.uri);
}

/// Lint a document and return diagnostics to publish
pub fn lint_document(state: &mut ServerState, uri: &Uri) -> Option<PublishDiagnosticsParams> {
    let doc = state.documents.get(uri)?;
    let linter = state.linter.as_ref()?;

    debug!("Linting document: {:?}", uri);

    match linter.lint(&LintTarget::String(&doc.content)) {
        Ok(output) => {
            let diagnostics: Vec<_> = output.iter().flat_map(to_lsp_diagnostics).collect();

            debug!("Found {} diagnostics", diagnostics.len());

            // Store for hover lookups
            if let Some(doc) = state.documents.get_mut(uri) {
                doc.lint_output = output;
            }

            Some(PublishDiagnosticsParams {
                uri: uri.clone(),
                diagnostics,
                version: None,
            })
        }
        Err(e) => {
            warn!("Failed to lint document {:?}: {}", uri, e);
            None
        }
    }
}
