import { Linter } from "supa-mdx-lint";
import { runAsWorker } from "synckit";

/**
 * @param {string} ruleId - The ID of the rule to check.
 * @param {string} sourceCode - The source code to lint.
 */
async function lintRule(ruleId, sourceCode) {
  const linter = await Linter.create();
  const errors = await linter.lint(
    {
      _type: "string",
      text: sourceCode,
      path: null,
    },
    ruleId,
  );
  return errors;
}

runAsWorker(lintRule);
