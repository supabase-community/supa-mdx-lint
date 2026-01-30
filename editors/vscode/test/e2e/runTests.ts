import * as path from 'path';
import { runTests } from '@vscode/test-electron';

async function main(): Promise<void> {
  try {
    // The folder containing the Extension Manifest package.json
    // After compilation, __dirname is dist/test/e2e, so we go up 3 levels
    const extensionDevelopmentPath = path.resolve(__dirname, '../../../');

    // The path to the extension test runner (compiled JS)
    const extensionTestsPath = path.resolve(__dirname, './index');

    // Create a test workspace
    const testWorkspacePath = path.resolve(__dirname, '../../../test-workspace');

    // Download VS Code, unzip it and run the integration test
    await runTests({
      extensionDevelopmentPath,
      extensionTestsPath,
      launchArgs: [testWorkspacePath, '--disable-extensions'],
    });
  } catch (err) {
    console.error('Failed to run tests:', err);
    process.exit(1);
  }
}

main();
