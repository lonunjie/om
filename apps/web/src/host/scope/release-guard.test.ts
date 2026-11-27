// SPDX-License-Identifier: MPL-2.0
import { readFileSync } from "node:fs";
import { preProcessFile } from "typescript";
import { describe, expect, it } from "vitest";
import { isScopeModule, scopeReleaseGuard } from "./release-guard";

describe("release scope guard", () => {
  it("keeps the release entry free of scope adapter and Wasm imports", () => {
    const entry = readFileSync(new URL("../../main.tsx", import.meta.url), "utf8");
    const imports = preProcessFile(entry).importedFiles.map(({ fileName }) => fileName);
    expect(imports).toContain("./core/ConfiguredApp");
    expect(imports.filter(isScopeModule)).toEqual([]);
  });

  it("matches scope source and the hashed Wasm asset while allowing the ordinary Core", () => {
    expect(isScopeModule("C:\\work\\apps\\web\\src\\host\\scope\\bridge.ts")).toBe(true);
    expect(isScopeModule("@om/core-wasm/om_wasm_scope.wasm?url")).toBe(true);
    expect(isScopeModule("assets/om_wasm_scope-C2x7Bz.wasm")).toBe(true);
    expect(isScopeModule("assets/om_wasm_groups-C2x7Bz.wasm")).toBe(false);
    expect(scopeReleaseGuard().generateBundle).toBeTypeOf("function");
  });
});
