/**
 * Build script for the supa-mdx-lint-lsp binary.
 *
 * By default, builds only for the current platform.
 * Use --all flag to attempt cross-compilation for all platforms.
 *
 * Note: Cross-compilation (e.g., building Windows binary on macOS) requires
 * either:
 * 1. The `cross` tool (https://github.com/cross-rs/cross) which uses Docker
 * 2. Properly configured cross-toolchains
 * 3. Running builds on each target platform (recommended for CI)
 *
 * For CI, we recommend using GitHub Actions matrix builds to build on
 * macOS, Linux, and Windows runners separately.
 */

import { execSync } from 'child_process';
import * as fs from 'fs';
import * as path from 'path';

const targets = [
  { rust: 'aarch64-apple-darwin', output: 'supa-mdx-lint-lsp-darwin-arm64' },
  { rust: 'x86_64-apple-darwin', output: 'supa-mdx-lint-lsp-darwin-x64' },
  { rust: 'x86_64-unknown-linux-gnu', output: 'supa-mdx-lint-lsp-linux-x64' },
  {
    rust: 'x86_64-pc-windows-msvc',
    output: 'supa-mdx-lint-lsp-win32-x64.exe',
  },
];

const binDir = path.join(__dirname, '..', 'bin');
const rootDir = path.join(__dirname, '..', '..', '..');

// Create bin directory
fs.mkdirSync(binDir, { recursive: true });

// Check if we should build all targets or just the current platform
const buildAll = process.argv.includes('--all');

if (buildAll) {
  console.log('Building for all platforms...');

  for (const target of targets) {
    console.log(`\nBuilding for ${target.rust}...`);

    try {
      // Check if the target is installed
      try {
        execSync(`rustup target add ${target.rust}`, {
          cwd: rootDir,
          stdio: 'inherit',
        });
      } catch {
        console.warn(`  Warning: Could not add target ${target.rust}`);
      }

      execSync(
        `cargo build --release --target ${target.rust} -p supa-mdx-lint-lsp`,
        { cwd: rootDir, stdio: 'inherit' }
      );

      const ext = target.output.endsWith('.exe') ? '.exe' : '';
      const sourcePath = path.join(
        rootDir,
        'target',
        target.rust,
        'release',
        `supa-mdx-lint-lsp${ext}`
      );
      const destPath = path.join(binDir, target.output);

      if (fs.existsSync(sourcePath)) {
        fs.copyFileSync(sourcePath, destPath);
        fs.chmodSync(destPath, 0o755);
        console.log(`  -> ${destPath}`);
      } else {
        console.warn(`  Warning: Binary not found at ${sourcePath}`);
      }
    } catch (e) {
      console.error(`Failed to build for ${target.rust}:`, e);
    }
  }
} else {
  // Build for current platform only
  console.log('Building for current platform...');

  try {
    execSync('cargo build --release -p supa-mdx-lint-lsp', {
      cwd: rootDir,
      stdio: 'inherit',
    });

    // Determine the output filename based on current platform
    const platform = process.platform;
    const arch = process.arch;

    let outputName: string;
    let ext = '';
    if (platform === 'darwin') {
      outputName =
        arch === 'arm64'
          ? 'supa-mdx-lint-lsp-darwin-arm64'
          : 'supa-mdx-lint-lsp-darwin-x64';
    } else if (platform === 'linux') {
      outputName = 'supa-mdx-lint-lsp-linux-x64';
    } else if (platform === 'win32') {
      outputName = 'supa-mdx-lint-lsp-win32-x64.exe';
      ext = '.exe';
    } else {
      console.error(`Unsupported platform: ${platform}`);
      process.exit(1);
    }

    const sourcePath = path.join(
      rootDir,
      'target',
      'release',
      `supa-mdx-lint-lsp${ext}`
    );
    const destPath = path.join(binDir, outputName);

    if (fs.existsSync(sourcePath)) {
      fs.copyFileSync(sourcePath, destPath);
      fs.chmodSync(destPath, 0o755);
      console.log(`Built: ${destPath}`);
    } else {
      console.error(`Binary not found at ${sourcePath}`);
      process.exit(1);
    }
  } catch (e) {
    console.error('Build failed:', e);
    process.exit(1);
  }
}

console.log('\nDone!');
