import { createRequire } from "node:module";
import { createSyncFn } from "synckit";

const require = createRequire(import.meta.url);
const lintRule = createSyncFn(require.resolve("./worker"));

/**
 * @param {import('supa-mdx-lint').LintError} error
 * @returns {import('eslint').Rule.ReportDescriptor}
 */
function mapError(error) {
  /** @type {import('eslint').Rule.ReportDescriptor} */
  const reportDescriptor = {
    message: error.message,
    loc: {
      start: {
        line: error.location.start.line,
        // From 1-indexed to 0-indexed
        column: error.location.start.column - 1,
      },
      end: {
        line: error.location.end.line,
        // From 1-indexed to 0-indexed
        column: error.location.end.column - 1,
      },
    },
  };

  if (error.fix) {
    reportDescriptor.fix = (fixer) => {
      error.fix.forEach((fix) => {
        switch (fix._type) {
          case "insert":
            fixer.insertTextAfterRange(
              [fix.point.offset - 1, fix.point.offset],
              fix.text,
            );
            break;
          case "delete":
            fixer.removeRange([
              fix.location.start.offset,
              fix.location.end.offset,
            ]);
            break;
          case "replace":
            fixer.replaceTextRange(
              [fix.location.start.offset, fix.location.end.offset],
              fix.text,
            );
            break;
          default:
            console.error(`Encountered unknown fix type: ${fix._type}`);
        }
      });
    };
  }

  return reportDescriptor;
}

/**
 * @param {string} ruleId - The ID of the rule to check.
 */
function ruleFactory(ruleId) {
  /**
   * @param {import('eslint').Rule.RuleContext} context
   */
  return function create(context) {
    return {
      Program() {
        /**
         * @type {import('supa-mdx-lint').LintError[]}
         */
        const errors = lintRule(ruleId, context.sourceCode.getText());
        errors.forEach((error) => {
          context.report(mapError(error));
        });
      },
    };
  };
}

/**
 * @param {Object} options - The options for the rule.
 * @param {string} options.ruleId - The ID of the rule to check.
 * @param {"problem" | "suggestion" | "layout"} options.type - The type of the rule.
 * @param {string} options.description - The description of the rule.
 * @param {"code" | "whitespace" | false} [options.fixable] - Whether the rule is fixable.
 * @param {boolean} [options.deprecated] - Whether the rule is deprecated.
 *
 * @returns {import('eslint').Rule.RuleModule}
 */
export function createRule({ ruleId, type, description, fixable, deprecated }) {
  /** @type {import('eslint').Rule.RuleModule} */
  const rule = {
    meta: {
      type,
      docs: {
        description,
      },
      schema: [],
    },
    create: ruleFactory(ruleId),
  };

  if (fixable) {
    rule.meta.fixable = fixable;
  }

  if (deprecated) {
    rule.meta.deprecated = deprecated;
  }

  return rule;
}
