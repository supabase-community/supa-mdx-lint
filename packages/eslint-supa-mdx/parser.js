/**
 * Create a dummy parser to satisfy ESLint.
 *
 * Takes the whole document and turns it into a Program node. Satisfies
 * ESLint's requirements for the syntax tree, while delegating actual parsing
 * to supa-mdx-lint during rule checking.
 *
 * This is an adapted version of the original parser from
 * eslint-plugin-markdownlint:
 * Copyright (c) 2021 Paweł BB Drozd (MIT License)
 * @see {@link https://github.com/paweldrozd/eslint-plugin-markdownlint/blob/main/LICENSE} original license
 */

import { createRequire } from "node:module";

/**
 * @param {string} code
 */
function parse(code) {
  const charsCount = code.length;
  const lines = code.split(/\r\n?|\n/g);
  const linesCount = lines.length;
  const lastLineLength = lines[linesCount - 1].length;

  return {
    type: "Program",
    start: 0,
    end: 0,
    loc: {
      start: {
        line: 1,
        column: 0,
      },
      end: {
        line: linesCount,
        column: lastLineLength,
      },
    },
    range: [0, charsCount],
    body: [],
    comments: [],
    tokens: [],
    code,
  };
}

const require = createRequire(import.meta.url);
const pkgJson = require("./package.json");

export default {
  meta: {
    name: pkgJson.name,
    version: pkgJson.version,
  },
  parse,
};
