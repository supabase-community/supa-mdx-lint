import * as wasm from '../pkg/supa_mdx_lint';

export type LintTarget = wasm.JsLintTarget & { _type: 'fileOrDirectory' | 'string' };
export type LintError = wasm.JsLintError;

export class Linter {
  private linter: wasm.Linter;

  constructor(options?: Record<string, unknown>) {
    const linterBuilder = new wasm.LinterBuilder();
    this.linter = linterBuilder.configure(options ?? {}).build();
  }

  lint(target: LintTarget): Promise<LintError[]> {
    return this.linter.lint(target);
  }
}
