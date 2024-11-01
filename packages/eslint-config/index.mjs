import js from "@eslint/js";
import eslintPluginPrettierRecommended from "eslint-plugin-prettier/recommended";
import globals from "globals";

export default [
  js.configs.recommended,
  {
    name: "base/ignores",
    ignores: ["node_modules/**", "dist/**"],
  },
  {
    name: "base/globals",
    languageOptions: {
      globals: {
        ...globals.node,
      },
    },
  },
  {
    name: "base/unused-disable-directives",
    linterOptions: {
      reportUnusedDisableDirectives: "warn",
    },
  },
  eslintPluginPrettierRecommended,
];
