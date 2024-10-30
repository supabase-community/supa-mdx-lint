import { describe, it, expect } from "vitest";
import { Linter, type LintTarget } from "../src";

describe("Linter", () => {
  it("should lint a valid mdx file", async () => {
    const linter = await Linter.create();
    const target: LintTarget = {
      _type: "string",
      path: null,
      text: `# Hello

This is a valid mdx file.
`,
    };
    const errors = await linter.lint(target);
    expect(errors).toEqual([]);
  });

  it("should lint an invalid mdx file", async () => {
    const linter = await Linter.create();
    const target: LintTarget = {
      _type: "string",
      path: null,
      text: `# Hello Bad Heading

This is an invalid mdx file.
`,
    };
    const errors = await linter.lint(target);
    expect(errors.length).toEqual(1);
  });
});
