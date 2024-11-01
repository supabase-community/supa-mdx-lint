import { RuleTester } from "eslint";

import parser from "../parser.js";

export const ruleTester = new RuleTester({
  languageOptions: {
    parser,
  },
});
