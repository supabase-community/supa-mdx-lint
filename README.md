# supa-mdx-lint

Work in progress.

An MDX linter meant to enforce the Supabase Docs style guide.

## Usage

```
Usage: supa-mdx-lint [OPTIONS] <TARGET>

Arguments:
  <TARGET>  (Glob of) files or directories to lint

Options:
  -c, --config <FILE>    Sets a custom config file
  -f, --fix              Auto-fix any fixable errors
      --format <FORMAT>  Output format [default: simple]
  -d, --debug            Turn debugging information on
  -s, --silent           Do not write anything to the output
  -h, --help             Print help
  -V, --version          Print version
```

## Configuration

The default configuration file is `supa-mdx-lint.config.toml`, relative to the
root of your working directory. You can point to a different config file using
the `--config` option.

Use the config file to define ignore patterns:

```
ignore_patterns = []
```

Or configure rule-specific settings:

```
[Rule001HeadingCase]
may_uppercase = []
```

Or configure rule error levels:

```
[Rule001HeadingCase]
level = "warn"
```

Or turn rules off entirely:

```
Rule001HeadingCase = false
```
