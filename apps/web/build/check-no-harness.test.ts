import { mkdirSync, mkdtempSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { afterEach, describe, expect, it } from "vitest";
import { findHarness } from "./check-no-harness.mjs";

const dirs: string[] = [];
const make = (files: Record<string, string>): string => {
  const d = mkdtempSync(join(tmpdir(), "om-nh-"));
  dirs.push(d);
  for (const [name, body] of Object.entries(files)) { mkdirSync(join(d, name, ".."), { recursive: true }); writeFileSync(join(d, name), body); }
  return d;
};
afterEach(() => { for (const d of dirs.splice(0)) rmSync(d, { recursive: true, force: true }); });

describe("check-no-harness", () => {
  it("passes a clean bundle", () => {
    expect(findHarness(make({ "index.html": "<html></html>", "assets/a.js": "console.log(1)" }))).toEqual([]);
  });
  it("fails a bundle containing the marker or the package name, in nested files", () => {
    const hits = findHarness(make({ "assets/a.js": "x='OM-DEV-HARNESS-NOT-FOR-PRODUCTION'", "assets/deep/b.js": "import '@om/core-harness'" }));
    expect(hits.length).toBe(2);
    expect(hits.join()).toContain("assets/a.js");
  });
  it("fails on a harness file name", () => {
    expect((findHarness(make({ "assets/core-harness-1.js": "" }))).length).toBe(1);
  });
});
