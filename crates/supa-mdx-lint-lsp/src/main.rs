use anyhow::Result;
use log::info;
use lsp_server::Connection;
use lsp_types::InitializeParams;

mod config;
mod diagnostics;
mod handlers;
mod server;
mod state;

fn main() -> Result<()> {
    env_logger::init();

    info!("Starting supa-mdx-lint-lsp");

    let (connection, io_threads) = Connection::stdio();
    let (initialize_id, initialize_params) = connection.initialize_start()?;

    let initialize_params: InitializeParams = serde_json::from_value(initialize_params)?;

    let server_capabilities = server::capabilities();
    let initialize_result = lsp_types::InitializeResult {
        capabilities: server_capabilities,
        server_info: Some(lsp_types::ServerInfo {
            name: "supa-mdx-lint-lsp".to_string(),
            version: Some(env!("CARGO_PKG_VERSION").to_string()),
        }),
    };

    connection.initialize_finish(initialize_id, serde_json::to_value(initialize_result)?)?;

    server::run(connection, initialize_params)?;

    io_threads.join()?;
    Ok(())
}
