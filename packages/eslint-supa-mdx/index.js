import parser from "./parser.js";
import rule001 from "./rules/rule001_heading_case.js";

import pkg from "./package.json" with { type: "json" };

const plugin = {
  meta: {
    name: pkg.name,
    version: pkg.version,
  },
  configs: {},
  rules: {
    "001-heading-case": rule001,
  },
};

Object.assign(plugin.configs, {
  recommended: [
    {
      files: ["**/*.mdx"],
      languageOptions: {
        parser,
      },
      plugins: {
        "supa-mdx": plugin,
      },
      rules: {
        "supa-mdx/001-heading-case": "error",
      },
    },
  ],
});

export default plugin;
