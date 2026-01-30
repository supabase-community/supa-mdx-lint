import * as path from 'path';
import * as os from 'os';
import * as fs from 'fs';
import { workspace, ExtensionContext, window } from 'vscode';
import {
  LanguageClient,
  LanguageClientOptions,
  ServerOptions,
} from 'vscode-languageclient/node';

let client: LanguageClient | undefined;

export function activate(context: ExtensionContext): void {
  const serverPath = getServerPath(context);

  if (!serverPath) {
    window.showErrorMessage(
      'supa-mdx-lint: Could not find the language server binary. ' +
        'Please set supa-mdx-lint.serverPath in your settings.'
    );
    return;
  }

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

  // Start the client. This will also launch the server
  client.start();
}

export function deactivate(): Thenable<void> | undefined {
  if (!client) {
    return undefined;
  }
  return client.stop();
}

function getServerPath(context: ExtensionContext): string | undefined {
  // First, check if user has specified a custom path
  const config = workspace.getConfiguration('supa-mdx-lint');
  const customPath = config.get<string>('serverPath');
  if (customPath && fs.existsSync(customPath)) {
    return customPath;
  }

  // Otherwise, use the bundled binary
  const platform = os.platform();
  const arch = os.arch();

  let binaryName: string;
  if (platform === 'darwin') {
    binaryName =
      arch === 'arm64'
        ? 'supa-mdx-lint-lsp-darwin-arm64'
        : 'supa-mdx-lint-lsp-darwin-x64';
  } else if (platform === 'linux') {
    binaryName = 'supa-mdx-lint-lsp-linux-x64';
  } else if (platform === 'win32') {
    binaryName = 'supa-mdx-lint-lsp-win32-x64.exe';
  } else {
    window.showErrorMessage(`supa-mdx-lint: Unsupported platform: ${platform}`);
    return undefined;
  }

  const bundledPath = path.join(context.extensionPath, 'bin', binaryName);
  if (fs.existsSync(bundledPath)) {
    return bundledPath;
  }

  // For development, try to find the binary in the workspace
  const workspacePath = path.join(
    context.extensionPath,
    '..',
    '..',
    'target',
    'release',
    'supa-mdx-lint-lsp'
  );
  if (fs.existsSync(workspacePath)) {
    return workspacePath;
  }

  const debugPath = path.join(
    context.extensionPath,
    '..',
    '..',
    'target',
    'debug',
    'supa-mdx-lint-lsp'
  );
  if (fs.existsSync(debugPath)) {
    return debugPath;
  }

  return undefined;
}
