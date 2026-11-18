// SPDX-License-Identifier: MPL-2.0
/**
 * Mentions in the composer (ADR-0222). The draft stays plain text. Choosing a person from the list inserts `@Name` and remembers
 * who it was; at send time each remembered person is looked for in the exact text being sent, so an edit that breaks or removes
 * the words simply drops that mention and a range can never point at the wrong words. Offsets are in code points, as Core counts them.
 */

export interface MentionCandidate { readonly id: string; readonly name: string }
export interface MentionPick { readonly account: string; readonly label: string }
export interface MentionRange { readonly start: number; readonly end: number; readonly account: string }

const MAX_QUERY = 40;
export const MAX_SUGGESTIONS = 6;
const wordChar = /[\p{L}\p{N}_]/u;
const isWord = (c: string | undefined): boolean => c !== undefined && wordChar.test(c);

/** The `@word` that ends at `caret` (UTF-16 index) and starts a word, or null. */
export function activeMentionQuery(text: string, caret: number): { readonly from: number; readonly query: string } | null {
  const upTo = text.slice(0, caret);
  for (let i = upTo.length - 1; i >= 0 && upTo.length - i <= MAX_QUERY + 1; i--) {
    const c = upTo.charAt(i);
    if (c === "@") {
      const before = i === 0 ? undefined : Array.from(upTo.slice(0, i)).pop();
      if (before !== undefined && !/\s/u.test(before) && isWord(before)) return null;
      return { from: i, query: upTo.slice(i + 1) };
    }
    if (/\s/u.test(c)) return null;
  }
  return null;
}

/** Candidates whose name contains the query (case-insensitive), the best starts first, at most `MAX_SUGGESTIONS`. */
export function matchCandidates(all: readonly MentionCandidate[], query: string): readonly MentionCandidate[] {
  const q = query.normalize("NFKC").toLowerCase();
  const rank = (c: MentionCandidate): number => { const n = c.name.normalize("NFKC").toLowerCase(); const i = n.indexOf(q); return i < 0 ? -1 : n.startsWith(q) ? 0 : 1; };
  return all.map((c) => ({ c, r: rank(c) })).filter((x) => x.r >= 0).sort((a, b) => a.r - b.r || a.c.name.localeCompare(b.c.name)).slice(0, MAX_SUGGESTIONS).map((x) => x.c);
}

/** Replace `[from, caret)` with the mention label and a space; returns the new text, the caret after it and the label. */
export function insertMention(text: string, from: number, caret: number, name: string): { readonly text: string; readonly caret: number; readonly label: string } {
  const label = `@${name}`;
  const next = `${text.slice(0, from)}${label} ${text.slice(caret)}`;
  return { text: next, caret: from + label.length + 1, label };
}

/**
 * The ranges (code points, half-open) of each pick in `text`: its label at a word boundary, not overlapping another. A pick whose
 * label is gone from the text is dropped. Sorted by position, which is what Core requires.
 */
export function mentionRanges(text: string, picks: readonly MentionPick[]): readonly MentionRange[] {
  const cps = Array.from(text);
  const out: MentionRange[] = [];
  for (const p of picks) {
    const label = Array.from(p.label);
    for (let i = 0; i + label.length <= cps.length; i++) {
      if (!label.every((c, k) => cps[i + k] === c)) continue;
      const end = i + label.length;
      if (isWord(cps[i - 1]) || isWord(cps[end])) continue;
      if (out.some((r) => i < r.end && end > r.start)) continue;
      out.push({ start: i, end, account: p.account });
      break;
    }
  }
  return out.sort((a, b) => a.start - b.start);
}

/** A mention as Core reports it: a code-point range of the text, the person, and whether it names us. */
export interface ShownMention { readonly start: number; readonly end: number; readonly account?: string; readonly accountId?: string; readonly me?: boolean }

/** The text as RichTextV1 view nodes with a mention node over each range (ranges that do not fit the text are ignored). */
export function textWithMentions(text: string, mentions: readonly ShownMention[], youAccount?: string): { readonly version: 1; readonly blocks: readonly { readonly type: "paragraph"; readonly children: readonly unknown[] }[] } {
  const cps = Array.from(text);
  const sorted = [...mentions].sort((a, b) => a.start - b.start);
  const kids: unknown[] = [];
  let at = 0;
  for (const m of sorted) {
    if (m.start < at || m.end <= m.start || m.end > cps.length) continue;
    if (m.start > at) kids.push({ type: "text", text: cps.slice(at, m.start).join("") });
    const label = cps.slice(m.start, m.end).join("");
    const acct = m.account ?? m.accountId;
    kids.push({ type: "mention", label: label.startsWith("@") ? label.slice(1) : label, ...(acct === undefined ? {} : { personId: acct }), ...(m.me === true || (youAccount !== undefined && acct === youAccount) ? { me: true } : {}) });
    at = m.end;
  }
  if (at < cps.length || kids.length === 0) kids.push({ type: "text", text: cps.slice(at).join("") });
  return { version: 1, blocks: [{ type: "paragraph", children: kids }] };
}
