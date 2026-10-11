import { Linter } from "eslint";
import tseslint from "typescript-eslint";
import { describe, expect, it } from "vitest";
import { noInjection } from "../../../eslint.policy.js";

// Syntax-level policy only (no type information needed), so this runs on virtual files.
const linter = new Linter({ configType: "flat" });
const lint = (code: string, file = "x.tsx") =>
  linter.verify(
    code,
    [
      {
        files: ["**/*.tsx", "**/*.ts"],
        languageOptions: { parser: tseslint.parser, parserOptions: { ecmaFeatures: { jsx: true } } },
        plugins: { "@typescript-eslint": tseslint.plugin },
        rules: {
          "no-restricted-syntax": ["error", ...noInjection],
          "@typescript-eslint/no-explicit-any": "error",
          "no-restricted-globals": ["error", "localStorage"],
        },
      },
    ],
    file,
  );

describe("lint policy fails closed (UW-0)", () => {
  const bad: [string, string][] = [
    ["dangerouslySetInnerHTML", "const a = <div dangerouslySetInnerHTML={{ __html: x }} />;"],
    ["innerHTML", "el.innerHTML = x;"],
    ["insertAdjacentHTML", "el.insertAdjacentHTML('beforeend', x);"],
    ["document.write", "document.write(x);"],
    ["eval", "eval(x);"],
    ["new Function", "new Function(x);"],
    ["string timer", "setTimeout('x()', 1);"],
    ["remote URL literal", "const u = 'https://cdn.example.com/a.js';"],
    ["remote URL template", "const u = `https://cdn.example.com/${x}`;"],
    ["any", "const a: any = 1;"],
    ["localStorage", "localStorage.setItem('k', 'v');"],
  ];
  it.each(bad)("rejects %s", (_name, code) => {
    expect(lint(code).length).toBeGreaterThan(0);
  });
  it("accepts ordinary code", () => {
    expect(lint("const a = <p>{String(1)}</p>; const b: unknown = 1;")).toEqual([]);
  });
});
