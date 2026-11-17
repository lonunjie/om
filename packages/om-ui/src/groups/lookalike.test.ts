import { describe, expect, it } from "vitest";
import { findSameNames } from "./sameName";
import { isMixedScript, lookalikeKey, matchable, MAX_LOOKALIKE_CODE_POINTS, scriptsIn } from "./lookalike";
import { CONFUSABLES_COUNT, CONFUSABLES_PAIRS, CONFUSABLES_VERSION } from "./confusables.generated";

// Lookalike characters are written as escapes so the source itself holds no homoglyph.
const CYR_A = "а"; // Cyrillic small a
const CYR_O = "о";
const CYR_E = "е";
const CYR_CAP_A = "А";
const GRK_OMICRON = "ο";
const alike = (a: string, b: string): boolean => findSameNames([{ id: "1", name: a }, { id: "2", name: b }]).of("1") !== null;
const kind = (a: string, b: string): string | undefined => findSameNames([{ id: "1", name: a }, { id: "2", name: b }]).of("1")?.kind;

describe("lookalike table (ADR-0197)", () => {
  it("is the pinned extract: a version, a count, pairs of one code point to ASCII letters and digits", () => {
    expect(CONFUSABLES_VERSION).toMatch(/^\d+\.\d+\.\d+$/);
    const pairs = CONFUSABLES_PAIRS.split(" ");
    expect(pairs).toHaveLength(CONFUSABLES_COUNT);
    for (const p of pairs) expect(p).toMatch(/^[0-9A-F]{4,6}=[A-Za-z0-9]+$/);
  });
});

describe("lookalike names collide (IS-01)", () => {
  it("a Cyrillic or Greek letter for a Latin one matches, as a lookalike and not as the same name", () => {
    expect(kind("Maria", `M${CYR_A}ria`)).toBe("alike");
    expect(kind("Maria", `Mari${CYR_A}`)).toBe("alike");
    expect(kind("Oscar", `${GRK_OMICRON}scar`)).toBe("alike");
    expect(kind("Leo", `L${CYR_E}${CYR_O}`)).toBe("alike");
    expect(kind("Anna", `${CYR_CAP_A}nna`)).toBe("alike");
  });
  it("mixed-script names are detected, and one written entirely in the other script matches too", () => {
    expect(isMixedScript(`M${CYR_A}ria`)).toBe(true);
    expect(isMixedScript("Maria")).toBe(false);
    expect([...scriptsIn(`M${CYR_A}ria`)].sort()).toEqual(["Cyrillic", "Latin"]);
    expect(alike("Pavel", "Раvеl")).toBe(true); // Cyrillic Er, a, Latin v, Cyrillic ie, Latin l
  });
  it("digit and letter lookalikes the table lists match", () => {
    expect(kind("Ian", "lan")).toBe("alike");
    expect(kind("Ian", "1an")).toBe("alike");
    expect(kind("Oli", "0li")).toBe("alike");
    expect(kind("Tom", "Torn")).toBe("alike"); // UTS #39 maps m to rn
    expect(kind("Al", "AI")).toBe("alike");
  });
  it("an invisible character, a joiner between letters or a bidi control does not make a name different", () => {
    for (const invisible of ["​", "‌", "‍", "⁠", "­", "‮", "⁦", "‏", "️", "͏", "᠋"]) expect(kind("Maria", `Mar${invisible}ia`)).toBe("alike");
  });
  it("white space inside a name does not hide a match", () => {
    expect(kind("Maria Gomez", "Maria   Gomez")).toBe("alike");
  });
  it("a group that holds the same name twice and a lookalike is one group and reads as alike", () => {
    const c = findSameNames([{ id: "a", name: "Maria" }, { id: "b", name: "maria" }, { id: "c", name: `M${CYR_A}ria` }]);
    expect(c.groups).toEqual([{ name: "Maria", count: 3, kind: "alike" }]);
    expect(c.of("c")).toMatchObject({ index: 3, of: 3, kind: "alike" });
  });
  it("exact equality still reads as the same name, including the case where case folding alone matches a skeleton-different pair", () => {
    expect(kind("Ian", "ian")).toBe("same");
    expect(kind("Maria", "  MARIA ")).toBe("same");
    expect(kind("Straße", "STRASSE")).toBe("same");
  });
});

describe("ordinary distinct names do not match", () => {
  const distinct: readonly (readonly [string, string])[] = [
    ["Maria", "Marta"], ["Anna", "Hanna"], ["Ana Reyes", "Ana Reyna"], ["Li", "Lo"], ["Tom", "Tim"], ["Jose", "José"], ["Noor", "Nora"],
    ["Ivan", "Ilan"], ["Omar", "Omer"], ["Sam", "Sami"], ["Lee Park", "Lea Park"], ["Dan", "Dani"], ["Eli", "Ella"], ["Kim", "Kin"],
  ];
  it.each(distinct)("%s and %s", (a, b) => { expect(alike(a, b)).toBe(false); });
  it("different Cyrillic and Greek names do not match each other", () => {
    expect(alike("Вера", "Надя")).toBe(false); // Vera, Nadya
    expect(alike("Αλέξης", "Ελένη")).toBe(false); // Alexis, Eleni
  });
  it("a one-letter skeleton does not match as a lookalike (only an equal name does)", () => {
    expect(matchable(lookalikeKey("I"))).toBe(false);
    expect(alike("I", "l")).toBe(false);
    expect(alike("A", CYR_CAP_A)).toBe(false);
    expect(alike("A", "a")).toBe(true);
  });
  it("a Latin name with a foreign word is mixed-script but matches only when another name resembles it", () => {
    expect(isMixedScript("Maria Жук")).toBe(true);
    expect(alike("Maria Жук", "Maria Gomez")).toBe(false);
  });
});

describe("names in other scripts, and emoji, do not false-positive", () => {
  const names = ["王芳", "李雷", "田中太郎", "さくら", "김민준", "محمد", "فاطمة", "नमस्ते", "अनिल", "שלום", "สวัสดี", "😀", "🎉🎉", "👩‍💻"];
  it("every distinct pair is distinct", () => {
    for (let i = 0; i < names.length; i++) for (let j = i + 1; j < names.length; j++) expect(alike(names[i] ?? "", names[j] ?? ""), `${String(i)} ${String(j)}`).toBe(false);
  });
  it("such a name still matches itself written again, and an emoji sequence keeps its joiners", () => {
    expect(kind("王芳", "王芳")).toBe("same");
    expect(lookalikeKey("👩‍💻")).toContain("‍");
    expect(alike("👩‍💻", "👩💻")).toBe(false);
    expect(lookalikeKey("مح‌مد")).toContain("‌"); // ZWNJ inside Arabic-script text is meaningful
  });
  it("CJK, Arabic and Devanagari characters never map onto Latin letters or digits", () => {
    expect(lookalikeKey("ローマ〇٠०")).toBe("ローマ〇٠०");
    expect(isMixedScript("王芳 Wang")).toBe(false);
  });
});

describe("bounds", () => {
  it("combining marks stay: a different accent is a different name, and decomposed equals precomposed", () => {
    expect(alike("Jose", "José")).toBe(false);
    expect(kind("José", "José")).toBe("same");
    expect(alike("Maria", "Mariá̂̃")).toBe(false);
  });
  it("a name of any length is handled in bounded work, and only its first part counts", () => {
    const long = "a".repeat(10000);
    const t = Date.now();
    expect(Array.from(lookalikeKey(long))).toHaveLength(MAX_LOOKALIKE_CODE_POINTS);
    expect(Date.now() - t).toBeLessThan(500);
    const people = Array.from({ length: 400 }, (_, i) => ({ id: `p${String(i)}`, name: `Name ${String(i)}` }));
    const start = Date.now();
    expect(findSameNames(people).groups).toEqual([]);
    expect(Date.now() - start).toBeLessThan(1500);
  });
  it("a name made only of invisible characters never collides with anything", () => {
    expect(alike("​​", "​​​")).toBe(false);
    expect(alike("​​", "Maria")).toBe(false);
  });
});
