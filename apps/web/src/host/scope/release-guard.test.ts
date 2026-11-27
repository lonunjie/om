// SPDX-License-Identifier: MPL-2.0
import { readFileSync } from "node:fs";
import { preProcessFile } from "typescript";
import { build, type Plugin } from "vite";
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


function fixture(transitiveScope: boolean): Plugin {
  const modules: Record<string, string> = {
    "/fixture/main.ts": 'export { value } from "/fixture/dependency.ts";',
    "/fixture/dependency.ts": transitiveScope
      ? 'export { value } from "/fixture/host/scope/adapter.ts";'
      : 'export const value = "ordinary";',
    "/fixture/host/scope/adapter.ts": 'export const value = "engineering";',
  };
  return {
    name: "scope-guard-fixture",
    resolveId(id) { return Object.hasOwn(modules, id) ? id : null; },
    load(id) { return modules[id] ?? null; },
  };
}

describe("release scope bundle", () => {
  const releaseBuild = (transitiveScope: boolean, format: "es" | "iife") => build({
    configFile: false,
    mode: "production",
    logLevel: "silent",
    plugins: [fixture(transitiveScope), scopeReleaseGuard()],
    build: {
      write: false,
      lib: { entry: "/fixture/main.ts", formats: [format], name: "ScopeGuardFixture", fileName: "fixture" },
    },
  });

  it.each(["es", "iife"] as const)("rejects an emitted transitive scope module in a %s build", async (format) => {
    await expect(releaseBuild(false, format)).resolves.toBeDefined();
    await expect(releaseBuild(true, format)).rejects.toThrow("Engineering scope code in the release bundle");
  });
});
