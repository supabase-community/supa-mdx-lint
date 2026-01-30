# supa-mdx-lint VS Code Extension

This extension provides linting for MDX and Markdown files using the supa-mdx-lint language server.

## Features

- Real-time linting of MDX and Markdown files
- Hover information for lint errors with rule descriptions
- Configurable lint timing (on save or as you type)
- Supports `.md` and `.mdx` files

## Configuration

| Setting | Type | Default | Description |
|---------|------|---------|-------------|
| `supa-mdx-lint.lintOn` | `"save"` \| `"type"` | `"type"` | When to lint documents |
| `supa-mdx-lint.serverPath` | `string` | `""` | Custom path to the language server binary |

## Development

### Building

1. Install dependencies:
   ```bash
   npm install
   ```

2. Build the LSP binary for your platform:
   ```bash
   npm run build:lsp
   ```

3. Build the extension:
   ```bash
   npm run build
   ```

### Running Tests

```bash
npm test
```

### Packaging

```bash
npm run package
```

This creates a `.vsix` file that can be installed in VS Code.

## License

MIT
