import { readFileSync } from "node:fs";
import { describe, expect, it } from "vitest";
import { checkBudget, limits, measure } from "./size-budget.mjs";

const budget = JSON.parse(readFileSync(new URL("./size-budget.json", import.meta.url), "utf8")) as Parameters<typeof limits>[0];

describe("ADR-0187 size budget", () => {
  it("records the ADR baseline and derives the limits from it", () => {
    expect(budget.baseline).toEqual({ deltaRawBytes: 2169546, deltaGzipBytes: 652950 });
    expect(limits({ ...budget, headroomPercent: 0 })).toEqual({ deltaRaw: 2169546, deltaGzip: 652950 });
  });
  it("accepts the measured baseline itself and refuses a growth past the limits, raw or gzip", () => {
    const base = { raw: 5131394, gzip: 1308330 };
    expect(checkBudget(base, { raw: 7300940, gzip: 1961280 }, { ...budget, headroomPercent: 0 }).ok).toBe(true);
    const raw = checkBudget(base, { raw: 7300941, gzip: 1961280 }, { ...budget, headroomPercent: 0 });
    expect(raw.ok).toBe(false);
    expect(raw.errors.join()).toMatch(/option D/);
    expect(checkBudget(base, { raw: 7300940, gzip: 1961281 }, { ...budget, headroomPercent: 0 }).ok).toBe(false);
  });
  it("refuses nonsense inputs (equal or inverted sizes)", () => {
    expect(checkBudget({ raw: 10, gzip: 5 }, { raw: 10, gzip: 5 }, budget).ok).toBe(false);
    expect(checkBudget({ raw: 0, gzip: 0 }, { raw: 10, gzip: 5 }, budget).ok).toBe(false);
  });
  it("measures raw and gzip", () => {
    const m = measure(new Uint8Array(1000));
    expect(m.raw).toBe(1000);
    expect(m.gzip).toBeLessThan(100);
  });
});
