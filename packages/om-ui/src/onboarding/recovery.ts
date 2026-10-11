export const VERIFY_COUNT = 3;

/** Picks `count` distinct group indices, ascending, using unbiased rejection sampling over crypto randomness. */
export function pickVerifyIndices(groupCount: number, count = VERIFY_COUNT, random: (max: number) => number = cryptoInt): readonly number[] {
  const n = Math.min(count, groupCount);
  const pool = Array.from({ length: groupCount }, (_, i) => i);
  const out: number[] = [];
  while (out.length < n) out.push(...pool.splice(random(pool.length), 1));
  return out.sort((a, b) => a - b);
}

function cryptoInt(max: number): number {
  const limit = Math.floor(0x1_0000_0000 / max) * max;
  const buf = new Uint32Array(1);
  for (;;) {
    globalThis.crypto.getRandomValues(buf);
    const v = buf[0] ?? 0;
    if (v < limit) return v % max;
  }
}

export const normalizeGroup = (s: string): string => s.replace(/\s+/g, "").toUpperCase();
export const groupMatches = (typed: string, expected: string): boolean => typed !== "" && normalizeGroup(typed) === normalizeGroup(expected);
