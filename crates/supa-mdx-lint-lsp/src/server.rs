use anyhow::Result;
use log::{debug, info};
use lsp_server::{Connection, Message, Notification, Request, Response};
use lsp_types::{
    notification::{
        DidChangeConfiguration, DidChangeTextDocument, DidCloseTextDocument, DidOpenTextDocument,
        DidSaveTextDocument, Notification as _, PublishDiagnostics,
    },
    request::{HoverRequest, Request as _},
    InitializeParams, PublishDiagnosticsParams, SaveOptions, ServerCapabilities,
    TextDocumentSyncCapability, TextDocumentSyncKind, TextDocumentSyncOptions,
    TextDocumentSyncSaveOptions,
};

use crate::handlers;
use crate::state::ServerState;

/// Return the server capabilities to advertise to the client
pub fn capabilities() -> ServerCapabilities {
    ServerCapabilities {
        text_document_sync: Some(TextDocumentSyncCapability::Options(
            TextDocumentSyncOptions {
                open_close: Some(true),
                change: Some(TextDocumentSyncKind::FULL),
                save: Some(TextDocumentSyncSaveOptions::SaveOptions(SaveOptions {
                    include_text: Some(true),
                })),
                ..Default::default()
            },
        )),
        hover_provider: Some(lsp_types::HoverProviderCapability::Simple(true)),
        // Phase 2: Add code_action_provider here
        ..Default::default()
    }
}

/// Run the main server loop
pub fn run(connection: Connection, init_params: InitializeParams) -> Result<()> {
    let mut state = ServerState::new();

    // Initialize state from params
    handlers::initialize::initialize(&mut state, init_params)?;

    info!("Server initialized, entering main loop");

    for msg in &connection.receiver {
        match msg {
            Message::Request(req) => {
                if connection.handle_shutdown(&req)? {
                    info!("Shutdown requested, exiting");
                    return Ok(());
                }

                handle_request(&connection, &state, req)?;
            }
            Message::Notification(notif) => {
                handle_notification(&connection, &mut state, notif)?;
            }
            Message::Response(resp) => {
                debug!("Received response: {:?}", resp);
            }
        }
    }

    Ok(())
}

fn handle_request(connection: &Connection, state: &ServerState, req: Request) -> Result<()> {
    debug!("Received request: {} (id: {:?})", req.method, req.id);

    let response = match req.method.as_str() {
        HoverRequest::METHOD => {
            let params = serde_json::from_value(req.params)?;
            let result = handlers::hover::handle_hover(state, params);
            Response {
                id: req.id,
                result: Some(serde_json::to_value(result)?),
                error: None,
            }
        }
        _ => {
            debug!("Unhandled request method: {}", req.method);
            Response {
                id: req.id,
                result: None,
                error: Some(lsp_server::ResponseError {
                    code: lsp_server::ErrorCode::MethodNotFound as i32,
                    message: format!("Method not found: {}", req.method),
                    data: None,
                }),
            }
        }
    };

    connection.sender.send(Message::Response(response))?;
    Ok(())
}

fn handle_notification(
    connection: &Connection,
    state: &mut ServerState,
    notif: Notification,
) -> Result<()> {
    debug!("Received notification: {}", notif.method);

    match notif.method.as_str() {
        DidOpenTextDocument::METHOD => {
            let params = serde_json::from_value(notif.params)?;
            if let Some(diags) = handlers::text_document::handle_did_open(state, params) {
                publish_diagnostics(connection, diags)?;
            }
        }
        DidChangeTextDocument::METHOD => {
            let params = serde_json::from_value(notif.params)?;
            if let Some(diags) = handlers::text_document::handle_did_change(state, params) {
                publish_diagnostics(connection, diags)?;
            }
        }
        DidSaveTextDocument::METHOD => {
            let params = serde_json::from_value(notif.params)?;
            if let Some(diags) = handlers::text_document::handle_did_save(state, params) {
                publish_diagnostics(connection, diags)?;
            }
        }
        DidCloseTextDocument::METHOD => {
            let params = serde_json::from_value(notif.params)?;
            handlers::text_document::handle_did_close(state, params);
        }
        DidChangeConfiguration::METHOD => {
            let params = serde_json::from_value(notif.params)?;
            handlers::workspace::handle_did_change_configuration(state, params);
        }
        _ => {
            debug!("Unhandled notification method: {}", notif.method);
        }
    }

    Ok(())
}

fn publish_diagnostics(connection: &Connection, params: PublishDiagnosticsParams) -> Result<()> {
    let notif = Notification {
        method: PublishDiagnostics::METHOD.to_string(),
        params: serde_json::to_value(params)?,
    };
    connection.sender.send(Message::Notification(notif))?;
    Ok(())
}
