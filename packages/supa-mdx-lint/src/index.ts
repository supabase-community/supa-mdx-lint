import { readFile } from 'node:fs/promises';

import * as wasm from '../pkg/supa_mdx_lint';
import { join } from 'node:path';

export type LintTarget = wasm.JsLintTarget & { _type: 'fileOrDirectory' | 'string' };
export type LintError = wasm.JsLintError;

const CONFIGURATION_FILE = "supa-mdx-lint.json";

export class Linter {
  private linter: wasm.Linter;

  static async create(options?: Record<string, unknown> | string) {
    const _options = (!!options && typeof options === 'object')
      ? options
      : (await getOptionsFromFile(join(process.cwd(), options ?? CONFIGURATION_FILE))) ?? {};

    return new Linter(_options);
  }

  constructor(options: any) {
    const linterBuilder = new wasm.LinterBuilder();
    this.linter = linterBuilder.configure(options).build();
  }

  lint(target: LintTarget, rule?: string): Promise<LintError[]> {
    return rule
      ? this.linter.lint_only_rule(rule, target)
      : this.linter.lint(target);
  }
}

async function getOptionsFromFile(filePath: string) {
  try {
    const file = await readFile(filePath, 'utf8');
    const options = JSON.parse(file);
    return options;
  } catch (err) {
    console.error(`Could not read a valid options file at ${filePath}. Proceeding with default (empty) options.`)
    console.error(err);
  }
}
