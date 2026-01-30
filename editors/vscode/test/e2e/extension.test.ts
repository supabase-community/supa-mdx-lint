import * as assert from 'assert';
import * as path from 'path';
import * as vscode from 'vscode';

suite('Extension Test Suite', () => {
  const testWorkspace = path.resolve(__dirname, '../../../test-workspace');
  const testFile = path.join(testWorkspace, 'test.md');

  // Wait for the extension to activate
  suiteSetup(async () => {
    const ext = vscode.extensions.getExtension('supabase.supa-mdx-lint');
    if (ext && !ext.isActive) {
      await ext.activate();
    }
    // Give the language server time to start
    await sleep(2000);
  });

  test('Extension activates and produces diagnostics', async () => {
    const doc = await vscode.workspace.openTextDocument(testFile);
    await vscode.window.showTextDocument(doc);

    // Wait for diagnostics to appear
    const diagnostics = await waitForDiagnostics(doc.uri, 10000);

    // Should have at least one diagnostic for the heading case
    assert.ok(
      diagnostics.length > 0,
      'Expected diagnostics for heading case violation'
    );
    assert.strictEqual(
      diagnostics[0].source,
      'supa-mdx-lint',
      'Diagnostic should come from supa-mdx-lint'
    );
  });

  test('Hover shows rule information', async () => {
    const doc = await vscode.workspace.openTextDocument(testFile);
    await vscode.window.showTextDocument(doc);

    // Wait for diagnostics first
    await waitForDiagnostics(doc.uri, 10000);

    // Get hover at position in the heading
    const hovers = await vscode.commands.executeCommand<vscode.Hover[]>(
      'vscode.executeHoverProvider',
      doc.uri,
      new vscode.Position(0, 3)
    );

    // Should have at least one hover with rule info
    assert.ok(hovers && hovers.length > 0, 'Expected hover information');

    const hoverContent = hovers[0].contents
      .map((c) => (typeof c === 'string' ? c : c.value))
      .join('\n');
    assert.ok(
      hoverContent.includes('Rule001'),
      'Hover should contain rule name'
    );
  });
});

function sleep(ms: number): Promise<void> {
  return new Promise((resolve) => setTimeout(resolve, ms));
}

async function waitForDiagnostics(
  uri: vscode.Uri,
  timeoutMs: number
): Promise<vscode.Diagnostic[]> {
  const start = Date.now();
  while (Date.now() - start < timeoutMs) {
    const diagnostics = vscode.languages.getDiagnostics(uri);
    if (diagnostics.length > 0) {
      return diagnostics;
    }
    await sleep(200);
  }
  return vscode.languages.getDiagnostics(uri);
}
