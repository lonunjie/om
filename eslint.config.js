// OM web workspace lint policy (UW-0). These rules enforce ENGINEERING_REPOSITORY_LAYOUT.md "TypeScript/web rules".
import js from "@eslint/js";
import tseslint from "typescript-eslint";
import { noInjection } from "./eslint.policy.js";


const SRC = ["packages/**/*.{ts,tsx}", "apps/**/*.{ts,tsx}"];

export default tseslint.config(
  { ignores: [".claude/**", "**/dist/**", "**/dist-types/**", "**/node_modules/**", "docs/**", "verification/**", "tools/**", "**/*.config.js", "eslint.policy.js", "**/__fixtures__/**"] },
  {
    files: SRC,
    extends: [js.configs.recommended, ...tseslint.configs.strictTypeChecked],
    languageOptions: { parserOptions: { projectService: true, tsconfigRootDir: import.meta.dirname } },
    rules: {
      "@typescript-eslint/no-explicit-any": "error",
      "@typescript-eslint/no-unsafe-assignment": "error",
      "@typescript-eslint/no-unsafe-member-access": "error",
      "@typescript-eslint/no-unsafe-argument": "error",
      "@typescript-eslint/no-unsafe-return": "error",
      "@typescript-eslint/no-unsafe-call": "error",
      "@typescript-eslint/consistent-type-imports": "error",
      "no-restricted-syntax": ["error", ...noInjection],
      "no-restricted-globals": ["error", { name: "localStorage", message: "localStorage is not the message database or a secret store; use the storage adapter." }],
    },
  },
  {
    // Tests may build fixtures with URLs and poke at deliberately bad code.
    files: ["**/*.test.ts", "**/*.test.tsx"],
    rules: { "no-restricted-syntax": "off" },
  },
);
