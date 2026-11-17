/**
 * Names that look like each other (IDENTIFIER_SPOOFING_AND_DISPLAY IS-01, ADR-0197). A UTS #39 skeleton of a name: two names with the same
 * skeleton can be mistaken for each other on a screen. The table is the pinned extract of Unicode's confusables.txt in
 * `confusables.generated.ts`; nothing is fetched. Display only: no name decides identity, and nothing here blocks anything.
 */
import { CONFUSABLES_PAIRS, SCRIPT_BLOCKS } from "./confusables.generated";

/** At most this many code points of a name are looked at, so a very long name costs a bounded amount of work. */
export const MAX_LOOKALIKE_CODE_POINTS = 256;
/** A skeleton shorter than this is never matched as a lookalike (one character is too little to resemble anything). Equal names still collide. */
export const MIN_LOOKALIKE_CODE_POINTS = 2;

const MAP: ReadonlyMap<string, string> = new Map(CONFUSABLES_PAIRS.split(" ").map((p): [string, string] => {
  const [hex = "", to = ""] = p.split("=");
  return [String.fromCodePoint(parseInt(hex, 16)), to];
}));

export type ScriptName = keyof typeof SCRIPT_BLOCKS;

/** The script of a letter among Latin, Cyrillic, Greek and Armenian, else null (every other script, and anything that is not a letter, is null). */
export function scriptOf(char: string): ScriptName | null {
  if (!/^\p{L}$/u.test(char)) return null;
  const cp = char.codePointAt(0) ?? 0;
  for (const [name, blocks] of Object.entries(SCRIPT_BLOCKS)) if (blocks.some(([a, b]) => cp >= a && cp <= b)) return name as ScriptName;
  return null;
}

/** The scripts, among the four, that the letters of a name use. */
export const scriptsIn = (name: string): ReadonlySet<ScriptName> => new Set(Array.from(name.normalize("NFKC")).slice(0, MAX_LOOKALIKE_CODE_POINTS).flatMap((c) => { const s = scriptOf(c); return s === null ? [] : [s]; }));

/** Letters of two or more of Latin, Cyrillic, Greek and Armenian in one name (a lookalike substitution, or an ordinary name with a foreign word: this alone is never a cue). */
export const isMixedScript = (name: string): boolean => scriptsIn(name).size >= 2;

const IGNORABLE = /^\p{Default_Ignorable_Code_Point}$/u;
const isJoiner = (c: string): boolean => c === "‌" || c === "‍";
/** Letters and digits of the four scripts (and ASCII). A joiner beside one of them joins nothing real, so it is dropped; in emoji, Arabic or Indic text it stays. */
const plain = (c: string | undefined): boolean => c !== undefined && (scriptOf(c) !== null || /^[0-9]$/.test(c));

/**
 * UTS #39 skeleton: NFKC, drop invisible default-ignorable characters (zero-width, bidi controls, variation selectors, soft hyphen; a joiner stays only
 * next to something that is not a plain letter or digit), NFD, map each character through the table, NFD, collapse white space, and fold case.
 */
export function lookalikeKey(name: string): string {
  const cps = Array.from(name.normalize("NFKC")).slice(0, MAX_LOOKALIKE_CODE_POINTS);
  const kept = cps.filter((c, i) => !IGNORABLE.test(c) || (isJoiner(c) && !plain(cps[i - 1]) && !plain(cps[i + 1])));
  const mapped = Array.from(kept.join("").normalize("NFD")).map((c) => MAP.get(c) ?? c).join("").normalize("NFD");
  return mapped.replace(/\s+/gu, " ").trim().toUpperCase().toLowerCase();
}

/** Whether a lookalike key is long enough to be matched. */
export const matchable = (key: string): boolean => Array.from(key).length >= MIN_LOOKALIKE_CODE_POINTS;
