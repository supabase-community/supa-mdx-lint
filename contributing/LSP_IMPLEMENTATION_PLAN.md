# LSP Implementation Plan for supa-mdx-lint

This document describes the plan for implementing a Language Server Protocol (LSP) server for supa-mdx-lint, along with a VS Code extension.

## Overview

| Phase | Scope | Deliverables |
|-------|-------|--------------|
| **Phase 1** | Diagnostics + VS Code | LSP binary, VS Code extension, hover support |
| **Phase 2** | Code Actions | Quick fixes from existing `LintCorrection` types |
| **Phase 3** | Editor Expansion | Neovim & Zed support |

## Design Decisions

| Decision | Choice |
|----------|--------|
| LSP framework | `lsp-server` (same as rust-analyzer) |
| Crate location | `crates/supa-mdx-lint-lsp/` |
| Extension location | `editors/vscode/` |
| Settings mechanism | `initializationOptions` + `didChangeConfiguration` |
| File types | `.md`, `.mdx` |
| Lint trigger | Configurable (`save` or `type`, default `type`) |
| Multi-root workspaces | Not supported |
| Workspace diagnostics | Only lint open files (not all files in workspace) |
| Config file watching | Re-lint all open files when config changes |
| Severity mapping | `LintLevel::Error` → `DiagnosticSeverity::ERROR`, `LintLevel::Warning` → `DiagnosticSeverity::WARNING` |
| Distribution | Local builds only (no crates.io or VS Code Marketplace) |

---

## Phase 1: Diagnostics & VS Code Extension

### 1.1 Convert to Cargo Workspace

The first step is converting the repository to a Cargo workspace. This enables:
- Workspace-level dependency versioning (single source of truth for dependency versions)
- Shared build artifacts between crates
- Easier management of multiple crates (linter library, LSP binary, macros)

#### 1.1.1 New Root Cargo.toml

Replace the root `Cargo.toml` with a workspace manifest. The main linter becomes a workspace member.

**New `/Cargo.toml`:**

```toml
[workspace]
members = ["crates/*"]
resolver = "2"

[workspace.package]
edition = "2021"
license = "MIT"
repository = "https://github.com/your-org/supa-mdx-lint"

[workspace.dependencies]
# Only dependencies shared across multiple crates go here
anyhow = "1.0.89"
env_logger = "0.11.5"
log = "0.4.22"
serde = { version = "1.0.210", features = ["derive"] }
serde_json = "1.0.128"
thiserror = "2.0.3"

# Internal crates
supa-mdx-lint = { path = "crates/supa-mdx-lint" }
supa_mdx_macros = { path = "crates/supa-mdx-macros" }
```

Note: Dependencies only used by a single crate (e.g., `lsp-server`, `clap`, `markdown`) stay in that crate's own `Cargo.toml` with their versions specified there. Only truly shared dependencies are centralized in `[workspace.dependencies]`.

#### 1.1.2 Move Main Linter and Macros to Crates

Move both the main linter and the macros crate under `crates/`:

```bash
mkdir -p crates
mv src tests crates/supa-mdx-lint/      # Create and move linter
mv supa-mdx-macros crates/              # Move macros crate
```

**New `/crates/supa-mdx-lint/Cargo.toml`:**

```toml
[package]
name = "supa-mdx-lint"
description = "Lint MDX files according to the Supabase style guide"
version = "0.3.1"
edition.workspace = true

[[bin]]
name = "supa-mdx-lint"
path = "src/main.rs"

[dependencies]
# Workspace dependencies (shared across crates)
anyhow.workspace = true
log.workspace = true
serde.workspace = true
serde_json.workspace = true
supa_mdx_macros.workspace = true
thiserror.workspace = true

# Crate-specific dependencies
bon = "3.3.2"
clap = { version = "4.5.20", features = ["derive"] }
crop = { version = "0.4.2", features = ["graphemes"] }
dialoguer = { version = "0.11.0", optional = true }
either = { version = "1.14.0", features = ["serde"] }
exitcode = "1.1.2"
fastrand = "2.3.0"
gag = "1.0.0"
glob = "0.3.1"
indexmap = "2.7.1"
itertools = "0.13.0"
markdown = "1.0.0-alpha.21"
miette = { version = "7.5.0", optional = true, features = ["fancy"] }
owo-colors = { version = "4.1.0", optional = true }
regex = "1.11.0"
regex-syntax = { version = "0.8.5", features = ["std", "unicode-perl"] }
serde_yaml = "0.9.34"
simplelog = "0.12.2"
symspell = "0.4.3"
toml = "0.8.19"

[dev-dependencies]
env_logger.workspace = true

assert_cmd = "2.0.16"
ctor = "0.2.8"
insta = "1.42.2"
predicates = "3.1.2"
public-api = "0.44.2"
rustdoc-json = "0.9.5"
rustup-toolchain = "0.1.10"
tempfile = "3.13.0"

[features]
interactive = ["dep:dialoguer", "dep:owo-colors", "pretty"]
pretty = ["dep:miette"]
```

#### 1.1.3 Update Macros Crate Path

**New location:** `/crates/supa-mdx-macros/Cargo.toml` (unchanged content):

```toml
[package]
name = "supa_mdx_macros"
version = "0.1.0"
edition.workspace = true

[lib]
proc-macro = true

[dependencies]
quote = "1.0.37"
syn = "2.0.79"
```

Note: `quote` and `syn` are only used by this crate, so they stay here with their versions.

#### 1.1.4 Directory Structure After Conversion

```
supa-mdx-lint/
├── Cargo.toml                    # Workspace manifest
├── Cargo.lock                    # Shared lockfile
├── crates/
│   ├── supa-mdx-lint/            # Main linter (moved from root)
│   │   ├── Cargo.toml
│   │   ├── src/
│   │   └── tests/
│   ├── supa-mdx-lint-lsp/        # LSP binary (added in Step 3)
│   │   ├── Cargo.toml
│   │   └── src/
│   └── supa-mdx-macros/          # Proc macros (moved from root)
│       ├── Cargo.toml
│       └── src/
├── editors/                      # Editor extensions (added in Step 5)
│   └── vscode/
└── contributing/
    └── LSP_IMPLEMENTATION_PLAN.md
```

#### 1.1.5 Update Other References

After moving files, update any paths in:
- `.github/workflows/` CI files (if they reference `src/`, `tests/`, or `supa-mdx-macros/`)
- Any scripts that reference the old structure
- The root `supa-mdx-lint.config.toml` (should stay at root for dogfooding)
- Update the path in the main linter's `Cargo.toml` for `supa_mdx_macros` (now `../supa-mdx-macros` relative, or use workspace dep)

#### 1.1.6 Verify Migration

Run these commands to verify the workspace is set up correctly:

```bash
cargo check --workspace
cargo test --workspace
cargo build --release -p supa-mdx-lint
```

### 1.2 Add Rule Descriptions

Add a `description()` method to the `Rule` trait so hover can display rule documentation.

**File:** `crates/supa-mdx-lint/src/rules.rs`

Add to the `Rule` trait:

```rust
pub trait Rule: Debug + RuleName {
    /// Returns a human-readable description of what this rule checks.
    fn description(&self) -> &'static str;

    fn default_level(&self) -> LintLevel;
    fn setup(&mut self, settings: Option<&mut RuleSettings>) {}
    fn check(&self, ast: &Node, context: &Context, level: LintLevel) -> Option<Vec<LintError>>;
}
```

Implement for each rule:

| Rule | Description |
|------|-------------|
| Rule001HeadingCase | "Enforces sentence case in headings" |
| Rule002AdmonitionTypes | "Validates that admonition types are recognized" |
| Rule003Spelling | "Checks for spelling errors" |
| Rule004ExcludeWords | "Detects forbidden words or phrases" |
| Rule005AdmonitionNewlines | "Enforces proper spacing in admonition tags" |
| Rule006NoAbsoluteUrls | "Prevents absolute URLs for internal links" |

### 1.3 LSP Binary Crate

**Location:** `crates/supa-mdx-lint-lsp/`

**Cargo.toml:**

```toml
[package]
name = "supa-mdx-lint-lsp"
version = "0.1.0"
edition.workspace = true

[[bin]]
name = "supa-mdx-lint-lsp"
path = "src/main.rs"

[dependencies]
# Workspace dependencies (shared across crates)
anyhow.workspace = true
env_logger.workspace = true
log.workspace = true
serde.workspace = true
serde_json.workspace = true
supa-mdx-lint.workspace = true

# LSP-specific dependencies (only used by this crate)
crossbeam-channel = "0.5"
lsp-server = "0.7"
lsp-types = "0.97"
```

**Source structure:**

```
crates/supa-mdx-lint-lsp/src/
├── main.rs              # Entry point: stdio transport, main loop setup
├── server.rs            # Main server loop, request/notification dispatch
├── state.rs             # ServerState: open documents, config, linter instance
├── config.rs            # LSP-specific settings (LspSettings struct)
├── diagnostics.rs       # LintError → lsp_types::Diagnostic conversion
├── handlers/
│   ├── mod.rs           # Handler exports
│   ├── initialize.rs    # initialize/initialized handlers
│   ├── text_document.rs # didOpen, didChange, didSave, didClose
│   ├── hover.rs         # textDocument/hover handler
│   └── workspace.rs     # workspace/didChangeConfiguration, config file watching
└── conversions.rs       # Location conversions (linter types ↔ LSP types)
```

#### 1.3.1 Main Entry Point (`main.rs`)

```rust
use anyhow::Result;
use lsp_server::{Connection, ExtractError, Message};
use lsp_types::InitializeParams;

mod config;
mod conversions;
mod diagnostics;
mod handlers;
mod server;
mod state;

fn main() -> Result<()> {
    env_logger::init();

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
```

#### 1.3.2 Server Capabilities

The LSP should advertise these capabilities:

```rust
pub fn capabilities() -> lsp_types::ServerCapabilities {
    lsp_types::ServerCapabilities {
        text_document_sync: Some(lsp_types::TextDocumentSyncCapability::Options(
            lsp_types::TextDocumentSyncOptions {
                open_close: Some(true),
                change: Some(lsp_types::TextDocumentSyncKind::FULL),
                save: Some(lsp_types::TextDocumentSyncSaveOptions::SaveOptions(
                    lsp_types::SaveOptions {
                        include_text: Some(true),
                    },
                )),
                ..Default::default()
            },
        )),
        hover_provider: Some(lsp_types::HoverProviderCapability::Simple(true)),
        // Phase 2: Add code_action_provider here
        ..Default::default()
    }
}
```

#### 1.3.3 Server State (`state.rs`)

```rust
use std::collections::HashMap;
use std::path::PathBuf;
use lsp_types::Url;
use supa_mdx_lint::{Linter, LintOutput};

use crate::config::LspSettings;

pub struct ServerState {
    /// Open document contents, keyed by URI
    pub documents: HashMap<Url, DocumentState>,

    /// Workspace root path
    pub workspace_root: Option<PathBuf>,

    /// Path to discovered config file (if any)
    pub config_path: Option<PathBuf>,

    /// The linter instance (rebuilt when config changes)
    pub linter: Option<Linter>,

    /// LSP-specific settings
    pub settings: LspSettings,
}

pub struct DocumentState {
    /// Current document content
    pub content: String,

    /// Most recent diagnostics (needed for hover to find rule info)
    pub diagnostics: Vec<LintOutput>,
}

impl ServerState {
    pub fn new() -> Self {
        Self {
            documents: HashMap::new(),
            workspace_root: None,
            config_path: None,
            linter: None,
            settings: LspSettings::default(),
        }
    }

    /// Discover and load config file from workspace root
    pub fn discover_config(&mut self) -> anyhow::Result<()> {
        // Look for supa-mdx-lint.config.toml in workspace_root
        // Build Linter instance if found
        // ...
    }

    /// Reload config and re-lint all open documents
    pub fn reload_config(&mut self) -> anyhow::Result<Vec<(Url, Vec<lsp_types::Diagnostic>)>> {
        // Rebuild linter
        // Re-lint all documents in self.documents
        // Return diagnostics to publish
        // ...
    }
}
```

#### 1.3.4 LSP Settings (`config.rs`)

```rust
use serde::Deserialize;

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LspSettings {
    /// When to lint: "save" or "type"
    #[serde(default = "default_lint_on")]
    pub lint_on: LintOn,
}

#[derive(Debug, Clone, Default, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum LintOn {
    Save,
    #[default]
    Type,
}

fn default_lint_on() -> LintOn {
    LintOn::Type
}

impl Default for LspSettings {
    fn default() -> Self {
        Self {
            lint_on: LintOn::Type,
        }
    }
}
```

Settings are received via:
1. `InitializeParams.initialization_options` at startup
2. `workspace/didChangeConfiguration` notifications during runtime

#### 1.3.5 Diagnostic Conversion (`diagnostics.rs`)

```rust
use lsp_types::{Diagnostic, DiagnosticSeverity, NumberOrString, Position, Range};
use supa_mdx_lint::{LintError, LintLevel, LintOutput};

pub fn to_lsp_diagnostics(output: &LintOutput) -> Vec<Diagnostic> {
    output
        .errors
        .iter()
        .map(|error| to_lsp_diagnostic(error))
        .collect()
}

pub fn to_lsp_diagnostic(error: &LintError) -> Diagnostic {
    Diagnostic {
        range: to_lsp_range(error),
        severity: Some(match error.level {
            LintLevel::Error => DiagnosticSeverity::ERROR,
            LintLevel::Warning => DiagnosticSeverity::WARNING,
        }),
        code: Some(NumberOrString::String(error.rule_name.to_string())),
        source: Some("supa-mdx-lint".to_string()),
        message: error.message.clone(),
        ..Default::default()
    }
}

fn to_lsp_range(error: &LintError) -> Range {
    // Convert from linter's 1-based lines to LSP's 0-based
    // Use error.location to get start/end positions
    let start = Position {
        line: error.location.start.row.saturating_sub(1) as u32,
        character: error.location.start.column.saturating_sub(1) as u32,
    };
    let end = Position {
        line: error.location.end.row.saturating_sub(1) as u32,
        character: error.location.end.column.saturating_sub(1) as u32,
    };
    Range { start, end }
}
```

#### 1.3.6 Request Handlers

**Initialize (`handlers/initialize.rs`):**
- Extract `workspace_root` from `InitializeParams.root_uri`
- Parse `initialization_options` into `LspSettings`
- Call `state.discover_config()`

**Text Document Sync (`handlers/text_document.rs`):**

```rust
// didOpen: Store document, lint, publish diagnostics
pub fn handle_did_open(state: &mut ServerState, params: DidOpenTextDocumentParams) -> Option<PublishDiagnosticsParams> {
    let uri = params.text_document.uri;
    let content = params.text_document.text;

    // Store document
    state.documents.insert(uri.clone(), DocumentState {
        content: content.clone(),
        diagnostics: vec![],
    });

    // Lint and return diagnostics
    lint_document(state, &uri)
}

// didChange: Update content, lint if lintOn == Type
pub fn handle_did_change(state: &mut ServerState, params: DidChangeTextDocumentParams) -> Option<PublishDiagnosticsParams> {
    let uri = params.text_document.uri;

    if let Some(doc) = state.documents.get_mut(&uri) {
        // Full sync, so take the whole content
        if let Some(change) = params.content_changes.into_iter().next() {
            doc.content = change.text;
        }
    }

    if state.settings.lint_on == LintOn::Type {
        lint_document(state, &uri)
    } else {
        None
    }
}

// didSave: Lint if lintOn == Save
pub fn handle_did_save(state: &mut ServerState, params: DidSaveTextDocumentParams) -> Option<PublishDiagnosticsParams> {
    if state.settings.lint_on == LintOn::Save {
        lint_document(state, &params.text_document.uri)
    } else {
        None
    }
}

// didClose: Remove document from state
pub fn handle_did_close(state: &mut ServerState, params: DidCloseTextDocumentParams) {
    state.documents.remove(&params.text_document.uri);
}

fn lint_document(state: &mut ServerState, uri: &Url) -> Option<PublishDiagnosticsParams> {
    let doc = state.documents.get(uri)?;
    let linter = state.linter.as_ref()?;

    let output = linter.lint(&LintTarget::String(&doc.content)).ok()?;
    let diagnostics = output.iter().flat_map(to_lsp_diagnostics).collect();

    // Store for hover lookups
    if let Some(doc) = state.documents.get_mut(uri) {
        doc.diagnostics = output;
    }

    Some(PublishDiagnosticsParams {
        uri: uri.clone(),
        diagnostics,
        version: None,
    })
}
```

**Hover (`handlers/hover.rs`):**

```rust
pub fn handle_hover(state: &ServerState, params: HoverParams) -> Option<Hover> {
    let uri = &params.text_document_position_params.text_document.uri;
    let position = params.text_document_position_params.position;

    let doc = state.documents.get(uri)?;

    // Find diagnostic at this position
    for output in &doc.diagnostics {
        for error in &output.errors {
            let range = to_lsp_range(error);
            if position_in_range(position, range) {
                // Get rule description
                let description = get_rule_description(&error.rule_name);

                let contents = format!(
                    "**{}**\n\n{}\n\n{}",
                    error.rule_name,
                    description,
                    error.message
                );

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

fn get_rule_description(rule_name: &str) -> &'static str {
    // Map rule name to description
    // This should use the Rule::description() method
    // May need to store rule registry in state or have a static lookup
    match rule_name {
        "Rule001HeadingCase" => "Enforces sentence case in headings",
        "Rule002AdmonitionTypes" => "Validates that admonition types are recognized",
        // ... etc
        _ => "No description available",
    }
}
```

**Workspace (`handlers/workspace.rs`):**
- `didChangeConfiguration`: Update `state.settings`, no re-lint needed (lint timing just changes)
- Config file watcher: When `supa-mdx-lint.config.toml` changes, call `state.reload_config()` and publish all diagnostics

#### 1.3.7 Main Server Loop (`server.rs`)

```rust
pub fn run(connection: Connection, init_params: InitializeParams) -> Result<()> {
    let mut state = ServerState::new();

    // Initialize state from params
    handlers::initialize::initialize(&mut state, init_params)?;

    for msg in &connection.receiver {
        match msg {
            Message::Request(req) => {
                if connection.handle_shutdown(&req)? {
                    return Ok(());
                }

                // Dispatch requests
                let response = match req.method.as_str() {
                    "textDocument/hover" => {
                        let params = serde_json::from_value(req.params)?;
                        let result = handlers::hover::handle_hover(&state, params);
                        Some(serde_json::to_value(result)?)
                    }
                    _ => None,
                };

                if let Some(result) = response {
                    connection.sender.send(Message::Response(Response {
                        id: req.id,
                        result: Some(result),
                        error: None,
                    }))?;
                }
            }
            Message::Notification(notif) => {
                // Dispatch notifications
                match notif.method.as_str() {
                    "textDocument/didOpen" => {
                        let params = serde_json::from_value(notif.params)?;
                        if let Some(diags) = handlers::text_document::handle_did_open(&mut state, params) {
                            publish_diagnostics(&connection, diags)?;
                        }
                    }
                    "textDocument/didChange" => {
                        let params = serde_json::from_value(notif.params)?;
                        if let Some(diags) = handlers::text_document::handle_did_change(&mut state, params) {
                            publish_diagnostics(&connection, diags)?;
                        }
                    }
                    "textDocument/didSave" => {
                        let params = serde_json::from_value(notif.params)?;
                        if let Some(diags) = handlers::text_document::handle_did_save(&mut state, params) {
                            publish_diagnostics(&connection, diags)?;
                        }
                    }
                    "textDocument/didClose" => {
                        let params = serde_json::from_value(notif.params)?;
                        handlers::text_document::handle_did_close(&mut state, params);
                    }
                    "workspace/didChangeConfiguration" => {
                        let params = serde_json::from_value(notif.params)?;
                        handlers::workspace::handle_did_change_configuration(&mut state, params);
                    }
                    _ => {}
                }
            }
            Message::Response(_) => {}
        }
    }

    Ok(())
}

fn publish_diagnostics(connection: &Connection, params: PublishDiagnosticsParams) -> Result<()> {
    connection.sender.send(Message::Notification(Notification {
        method: "textDocument/publishDiagnostics".to_string(),
        params: serde_json::to_value(params)?,
    }))?;
    Ok(())
}
```

### 1.4 VS Code Extension

**Location:** `editors/vscode/`

**Structure:**

```
editors/vscode/
├── package.json
├── tsconfig.json
├── esbuild.js              # Bundle extension
├── src/
│   ├── extension.ts        # Main entry point
│   └── config.ts           # Configuration handling
├── bin/                    # LSP binaries (gitignored)
│   ├── supa-mdx-lint-lsp-darwin-arm64
│   ├── supa-mdx-lint-lsp-darwin-x64
│   ├── supa-mdx-lint-lsp-linux-x64
│   └── supa-mdx-lint-lsp-win32-x64.exe
├── scripts/
│   └── build-lsp.ts        # Build LSP for all platforms
└── test/
    └── e2e/
        ├── runTests.ts
        └── extension.test.ts
```

#### 1.4.1 package.json

```json
{
  "name": "supa-mdx-lint",
  "displayName": "supa-mdx-lint",
  "description": "MDX/Markdown linter",
  "version": "0.1.0",
  "publisher": "your-publisher-name",
  "engines": {
    "vscode": "^1.85.0"
  },
  "categories": ["Linters"],
  "activationEvents": [],
  "main": "./dist/extension.js",
  "contributes": {
    "configuration": {
      "title": "supa-mdx-lint",
      "properties": {
        "supa-mdx-lint.lintOn": {
          "type": "string",
          "enum": ["save", "type"],
          "default": "type",
          "description": "When to lint documents: on save or as you type"
        }
      }
    },
    "languages": [
      {
        "id": "mdx",
        "extensions": [".mdx"],
        "aliases": ["MDX"]
      }
    ]
  },
  "scripts": {
    "vscode:prepublish": "npm run build",
    "build": "esbuild src/extension.ts --bundle --outfile=dist/extension.js --external:vscode --platform=node --format=cjs",
    "build:lsp": "ts-node scripts/build-lsp.ts",
    "test": "node ./test/e2e/runTests.js"
  },
  "dependencies": {
    "vscode-languageclient": "^9.0.1"
  },
  "devDependencies": {
    "@types/vscode": "^1.85.0",
    "@vscode/test-electron": "^2.3.8",
    "esbuild": "^0.27.2",
    "typescript": "^5.3.0"
  }
}
```

#### 1.4.2 Extension Entry Point (`src/extension.ts`)

```typescript
import * as path from 'path';
import * as os from 'os';
import { workspace, ExtensionContext } from 'vscode';
import {
  LanguageClient,
  LanguageClientOptions,
  ServerOptions,
} from 'vscode-languageclient/node';

let client: LanguageClient;

export function activate(context: ExtensionContext) {
  const serverPath = getServerPath(context);

  const serverOptions: ServerOptions = {
    run: { command: serverPath },
    debug: { command: serverPath },
  };

  const config = workspace.getConfiguration('supa-mdx-lint');

  const clientOptions: LanguageClientOptions = {
    documentSelector: [
      { scheme: 'file', language: 'markdown' },
      { scheme: 'file', language: 'mdx' },
      { scheme: 'file', pattern: '**/*.md' },
      { scheme: 'file', pattern: '**/*.mdx' },
    ],
    initializationOptions: {
      lintOn: config.get<string>('lintOn', 'type'),
    },
    synchronize: {
      configurationSection: 'supa-mdx-lint',
    },
  };

  client = new LanguageClient(
    'supa-mdx-lint',
    'supa-mdx-lint Language Server',
    serverOptions,
    clientOptions
  );

  client.start();
}

export function deactivate(): Thenable<void> | undefined {
  if (!client) {
    return undefined;
  }
  return client.stop();
}

function getServerPath(context: ExtensionContext): string {
  const platform = os.platform();
  const arch = os.arch();

  let binaryName: string;
  if (platform === 'darwin') {
    binaryName = arch === 'arm64'
      ? 'supa-mdx-lint-lsp-darwin-arm64'
      : 'supa-mdx-lint-lsp-darwin-x64';
  } else if (platform === 'linux') {
    binaryName = 'supa-mdx-lint-lsp-linux-x64';
  } else if (platform === 'win32') {
    binaryName = 'supa-mdx-lint-lsp-win32-x64.exe';
  } else {
    throw new Error(`Unsupported platform: ${platform}`);
  }

  return path.join(context.extensionPath, 'bin', binaryName);
}
```

#### 1.4.3 Binary Build Script (`scripts/build-lsp.ts`)

```typescript
import { execSync } from 'child_process';
import * as fs from 'fs';
import * as path from 'path';

const targets = [
  { rust: 'aarch64-apple-darwin', output: 'supa-mdx-lint-lsp-darwin-arm64' },
  { rust: 'x86_64-apple-darwin', output: 'supa-mdx-lint-lsp-darwin-x64' },
  { rust: 'x86_64-unknown-linux-gnu', output: 'supa-mdx-lint-lsp-linux-x64' },
  { rust: 'x86_64-pc-windows-msvc', output: 'supa-mdx-lint-lsp-win32-x64.exe' },
];

const binDir = path.join(__dirname, '..', 'bin');
fs.mkdirSync(binDir, { recursive: true });

for (const target of targets) {
  console.log(`Building for ${target.rust}...`);

  try {
    execSync(
      `cargo build --release --target ${target.rust} -p supa-mdx-lint-lsp`,
      { cwd: path.join(__dirname, '..', '..', '..'), stdio: 'inherit' }
    );

    const ext = target.output.endsWith('.exe') ? '.exe' : '';
    const sourcePath = path.join(
      __dirname, '..', '..', '..', 'target', target.rust, 'release',
      `supa-mdx-lint-lsp${ext}`
    );
    const destPath = path.join(binDir, target.output);

    fs.copyFileSync(sourcePath, destPath);
    fs.chmodSync(destPath, 0o755);

    console.log(`  -> ${destPath}`);
  } catch (e) {
    console.error(`Failed to build for ${target.rust}:`, e);
  }
}
```

### 1.5 Integration Tests (LSP)

**Location:** `crates/supa-mdx-lint-lsp/tests/`

**Structure:**

```
crates/supa-mdx-lint-lsp/tests/
├── common/
│   └── mod.rs            # Test utilities, LSP client helpers
├── initialization.rs     # Initialize handshake tests
├── diagnostics.rs        # Diagnostic publishing tests
├── hover.rs              # Hover tests
└── config.rs             # Config discovery and reload tests
```

**Test Approach:**

Use `lsp-server`'s testing utilities or create a test harness that:
1. Spawns the LSP binary
2. Sends JSON-RPC messages via stdin
3. Reads responses from stdout
4. Asserts on the results

**Example Test:**

```rust
#[test]
fn test_diagnostics_on_open() {
    let mut client = TestClient::new();

    client.initialize(json!({
        "rootUri": "file:///tmp/test-workspace",
        "initializationOptions": { "lintOn": "type" }
    }));

    client.notify("textDocument/didOpen", json!({
        "textDocument": {
            "uri": "file:///tmp/test-workspace/test.md",
            "languageId": "markdown",
            "version": 1,
            "text": "# hello world\n"  // Should trigger Rule001 (heading case)
        }
    }));

    let diagnostics = client.wait_for_diagnostics("file:///tmp/test-workspace/test.md");

    assert_eq!(diagnostics.len(), 1);
    assert_eq!(diagnostics[0].code, Some(NumberOrString::String("Rule001HeadingCase".to_string())));
    assert_eq!(diagnostics[0].severity, Some(DiagnosticSeverity::WARNING));
}
```

**Test Cases:**

1. **Initialization**
   - Server returns correct capabilities
   - Server info includes name and version
   - Settings parsed from initializationOptions

2. **Diagnostics**
   - Diagnostics published on didOpen
   - Diagnostics published on didChange when lintOn=type
   - Diagnostics published on didSave when lintOn=save
   - Diagnostics NOT published on didChange when lintOn=save
   - Diagnostics have correct range (0-based lines/columns)
   - Diagnostics have correct severity mapping

3. **Hover**
   - Hover returns rule info when over diagnostic
   - Hover returns null when not over diagnostic

4. **Configuration**
   - Config file discovered from workspace root
   - Config changes trigger re-lint of open files
   - didChangeConfiguration updates settings

### 1.6 E2E Tests (VS Code)

**Location:** `editors/vscode/test/e2e/`

**Test Runner (`runTests.ts`):**

```typescript
import * as path from 'path';
import { runTests } from '@vscode/test-electron';

async function main() {
  const extensionDevelopmentPath = path.resolve(__dirname, '../../');
  const extensionTestsPath = path.resolve(__dirname, './extension.test');
  const testWorkspace = path.resolve(__dirname, '../../test-workspace');

  await runTests({
    extensionDevelopmentPath,
    extensionTestsPath,
    launchArgs: [testWorkspace],
  });
}

main();
```

**Test Cases (`extension.test.ts`):**

```typescript
import * as vscode from 'vscode';
import * as assert from 'assert';

suite('Extension Test Suite', () => {
  test('Extension activates on markdown file', async () => {
    const doc = await vscode.workspace.openTextDocument({
      language: 'markdown',
      content: '# hello world\n',
    });
    await vscode.window.showTextDocument(doc);

    // Wait for extension to activate and diagnostics to appear
    await sleep(2000);

    const diagnostics = vscode.languages.getDiagnostics(doc.uri);
    assert.ok(diagnostics.length > 0, 'Expected diagnostics');
    assert.strictEqual(diagnostics[0].source, 'supa-mdx-lint');
  });

  test('Hover shows rule description', async () => {
    const doc = await vscode.workspace.openTextDocument({
      language: 'markdown',
      content: '# hello world\n',
    });
    const editor = await vscode.window.showTextDocument(doc);

    await sleep(2000);

    const hovers = await vscode.commands.executeCommand<vscode.Hover[]>(
      'vscode.executeHoverProvider',
      doc.uri,
      new vscode.Position(0, 3)  // Position in the heading
    );

    assert.ok(hovers && hovers.length > 0, 'Expected hover');
  });

  test('Settings change takes effect', async () => {
    const config = vscode.workspace.getConfiguration('supa-mdx-lint');
    await config.update('lintOn', 'save', vscode.ConfigurationTarget.Workspace);

    // Verify setting changed (actual behavior tested via LSP integration tests)
    assert.strictEqual(config.get('lintOn'), 'save');
  });
});

function sleep(ms: number): Promise<void> {
  return new Promise(resolve => setTimeout(resolve, ms));
}
```

---

## Phase 2: Code Actions

### 2.1 LSP Changes

**Add capability:**

```rust
code_action_provider: Some(lsp_types::CodeActionProviderCapability::Simple(true)),
```

**New handler (`handlers/code_action.rs`):**

```rust
pub fn handle_code_action(
    state: &ServerState,
    params: CodeActionParams,
) -> Option<Vec<CodeActionOrCommand>> {
    let uri = &params.text_document.uri;
    let doc = state.documents.get(uri)?;

    let mut actions = Vec::new();

    // Find diagnostics in the requested range that have fixes
    for output in &doc.diagnostics {
        for error in &output.errors {
            if !ranges_overlap(to_lsp_range(error), params.range) {
                continue;
            }

            // Check if error has fixes
            if let Some(corrections) = &error.corrections {
                for correction in corrections {
                    let edit = correction_to_workspace_edit(uri, correction);

                    actions.push(CodeActionOrCommand::CodeAction(CodeAction {
                        title: format!("Fix: {}", error.message),
                        kind: Some(CodeActionKind::QUICKFIX),
                        diagnostics: Some(vec![to_lsp_diagnostic(error)]),
                        edit: Some(edit),
                        ..Default::default()
                    }));
                }
            }
        }
    }

    Some(actions)
}

fn correction_to_workspace_edit(uri: &Url, correction: &LintCorrection) -> WorkspaceEdit {
    let text_edit = match correction {
        LintCorrection::Replace(r) => TextEdit {
            range: offset_range_to_lsp_range(r.start_offset, r.end_offset),
            new_text: r.replacement.clone(),
        },
        LintCorrection::Insert(i) => TextEdit {
            range: Range {
                start: offset_to_position(i.offset),
                end: offset_to_position(i.offset),
            },
            new_text: i.text.clone(),
        },
        LintCorrection::Delete(d) => TextEdit {
            range: offset_range_to_lsp_range(d.start_offset, d.end_offset),
            new_text: String::new(),
        },
    };

    let mut changes = HashMap::new();
    changes.insert(uri.clone(), vec![text_edit]);

    WorkspaceEdit {
        changes: Some(changes),
        ..Default::default()
    }
}
```

### 2.2 Store Diagnostic Context

The `LintOutput` stored in `DocumentState` already contains the fix information. Ensure `LintError.corrections` is populated by rules that support auto-fix.

---

## Phase 3: Additional Editors

### 3.1 Neovim

**Location:** `editors/nvim/`

Create documentation and optional Lua plugin:

```
editors/nvim/
├── README.md           # Setup instructions
└── lua/
    └── supa-mdx-lint/
        └── init.lua    # Optional convenience plugin
```

**README.md content:**

````markdown
# supa-mdx-lint for Neovim

## Installation

### Using nvim-lspconfig

```lua
local lspconfig = require('lspconfig')
local configs = require('lspconfig.configs')

if not configs.supa_mdx_lint then
  configs.supa_mdx_lint = {
    default_config = {
      cmd = { '/path/to/supa-mdx-lint-lsp' },
      filetypes = { 'markdown', 'mdx' },
      root_dir = lspconfig.util.root_pattern('supa-mdx-lint.config.toml', '.git'),
      init_options = {
        lintOn = 'type',  -- or 'save'
      },
    },
  }
end

lspconfig.supa_mdx_lint.setup {}
```
````

### 3.2 Zed

**Location:** `editors/zed/`

Create Zed extension:

```
editors/zed/
├── extension.toml
├── languages/
│   └── mdx/
│       └── config.toml
└── README.md
```

**extension.toml:**

```toml
id = "supa-mdx-lint"
name = "supa-mdx-lint"
version = "0.1.0"
description = "MDX/Markdown linter"

[language_servers.supa-mdx-lint]
name = "supa-mdx-lint"
language = "Markdown"
```

---

## Implementation Checklist

### Phase 1

**Step 1: Convert to Cargo Workspace**
- [ ] Create new root `Cargo.toml` with workspace manifest and `[workspace.dependencies]` for shared deps only
- [ ] Create `crates/` directory
- [ ] Move `src/` and `tests/` to `crates/supa-mdx-lint/`
- [ ] Move `supa-mdx-macros/` to `crates/supa-mdx-macros/`
- [ ] Create `crates/supa-mdx-lint/Cargo.toml` (workspace deps for shared, local versions for crate-specific)
- [ ] Update `crates/supa-mdx-macros/Cargo.toml` to use `edition.workspace = true`
- [ ] Update CI workflows for new paths
- [ ] Verify with `cargo check --workspace && cargo test --workspace`

**Step 2: Add Rule Descriptions**
- [ ] Add `description()` method to `Rule` trait
- [ ] Implement `description()` for all 6 rules

**Step 3: Create LSP Binary Crate**
- [ ] Create `crates/supa-mdx-lint-lsp/` directory structure
- [ ] Implement `main.rs` with stdio transport
- [ ] Implement `state.rs` with document tracking
- [ ] Implement `config.rs` with LSP settings
- [ ] Implement `diagnostics.rs` with conversion functions
- [ ] Implement `handlers/initialize.rs`
- [ ] Implement `handlers/text_document.rs` (didOpen, didChange, didSave, didClose)
- [ ] Implement `handlers/hover.rs`
- [ ] Implement `handlers/workspace.rs` (config watching)
- [ ] Implement `server.rs` main loop

**Step 4: LSP Integration Tests**
- [ ] Create test harness for spawning LSP and sending JSON-RPC messages
- [ ] Write initialization tests
- [ ] Write diagnostic publishing tests
- [ ] Write hover tests
- [ ] Write config discovery/reload tests

**Step 5: VS Code Extension**
- [ ] Create `editors/vscode/` directory structure
- [ ] Create `package.json` with extension manifest
- [ ] Implement `extension.ts`
- [ ] Create build script for LSP binaries
- [ ] Write VS Code e2e tests
- [ ] Test manually in VS Code

### Phase 2

- [ ] Add `code_action_provider` capability
- [ ] Implement `handlers/code_action.rs`
- [ ] Add code action integration tests
- [ ] Test quick fixes in VS Code

### Phase 3

- [ ] Create `editors/nvim/` with documentation
- [ ] Create `editors/zed/` extension
- [ ] Test in Neovim
- [ ] Test in Zed
