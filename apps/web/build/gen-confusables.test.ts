import { readFileSync } from "node:fs";
import { createHash } from "node:crypto";
import { describe, expect, it } from "vitest";
import { EXTRACT_PATH, extract, generate, GENERATED_PATH, keep, parseLines } from "./gen-confusables.mjs";
import { CONFUSABLES_EXTRACT_SHA256 } from "../../../packages/om-ui/src/groups/confusables.generated.ts";

const extractText = readFileSync(EXTRACT_PATH, "utf8");

describe("ADR-0197 confusables table", () => {
  it("regenerating the table from the checked-in extract gives the checked-in file", () => {
    expect(generate(extractText)).toBe(readFileSync(GENERATED_PATH, "utf8"));
  });
  it("the generated file names the extract's own hash, and the extract names the version and the source hash", () => {
    expect(CONFUSABLES_EXTRACT_SHA256).toBe(createHash("sha256").update(extractText, "utf8").digest("hex"));
    expect(extractText).toMatch(/^# Version: \d+\.\d+\.\d+$/m);
    expect(extractText).toMatch(/^# Source file SHA-256: [0-9a-f]{64}$/m);
  });
  it("every extract line passes the filter, and the filter keeps what is wanted and drops the rest", () => {
    for (const p of parseLines(extractText)) expect(keep(p)).toBe(true);
    expect(keep({ cp: 0x0430, to: "a" })).toBe(true); // Cyrillic a
    expect(keep({ cp: 0x03bf, to: "o" })).toBe(true); // Greek omicron
    expect(keep({ cp: 0x006d, to: "rn" })).toBe(true);
    expect(keep({ cp: 0x0030, to: "O" })).toBe(true);
    expect(keep({ cp: 0x30ed, to: "口" })).toBe(false); // katakana: not a source script, prototype not ASCII
    expect(keep({ cp: 0x0966, to: "o" })).toBe(false); // Devanagari digit zero
    expect(keep({ cp: 0x0660, to: "." })).toBe(false); // Arabic-indic digit zero
    expect(keep({ cp: 0xff41, to: "a" })).toBe(false); // full-width a: NFKC already folds it
    expect(keep({ cp: 0x0430, to: "a'" })).toBe(false); // prototype with punctuation
  });
  it("extracting a sample of the source format keeps only the wanted lines, unchanged", () => {
    const sample = "# confusables.txt\n# Version: 99.0.0\n#\n0430 ;\t0061 ;\tMA\t# ( x → a ) CYRILLIC\t# \n30ED ;\t53E3 ;\tMA\t# KATAKANA\t# \n";
    const out = extract(sample);
    expect(out).toContain("0430 ;\t0061 ;\tMA");
    expect(out).not.toContain("30ED");
    expect(out).toContain("# Kept lines: 1");
    expect(generate(out)).toContain('CONFUSABLES_PAIRS = "0430=a"');
  });
});
