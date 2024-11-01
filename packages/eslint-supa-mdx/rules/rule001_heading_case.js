import { createRule } from "../rules.js";

const rule = createRule({
  ruleId: "Rule001HeadingCase",
  type: "problem",
  description: "Headings should be in sentence case",
  fixable: true,
});

export default rule;
