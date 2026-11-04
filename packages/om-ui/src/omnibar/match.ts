export type OmniKind = "conversation" | "action" | "setting" | "message" | "file";

/** One thing the Omnibar can open or do. The host builds these from local state; nothing here touches a network. */
export interface OmniEntry {
  readonly id: string;
  readonly kind: OmniKind;
  readonly label: string;
  /** Extra words that should find it (not shown). */
  readonly keywords?: string;
  /** Shown beside the label when it helps tell two results apart. */
  readonly detail?: string;
  /** Shown with nothing typed. Entries without it appear only once the person types. */
  readonly suggested?: boolean;
  /** Runs it. The Omnibar closes first, so focus lands where the action puts it. */
  readonly run: () => void;
}

/** Case, accents and compatibility forms folded away, so "cafe" finds "Café". Shared by every local match. */
export const norm = (s: string): string => s.normalize("NFKD").replace(/\p{M}+/gu, "").toLowerCase();

/** Lower is better; null does not match. Exact, then prefix, then word prefix, then anywhere; every typed word must match. */
export function scoreEntry(entry: OmniEntry, query: string): number | null {
  const words = norm(query).split(/\s+/).filter((w) => w !== "");
  if (words.length === 0) return 0;
  const label = norm(entry.label);
  const rest = norm(`${entry.detail ?? ""} ${entry.keywords ?? ""}`);
  let total = 0;
  for (const w of words) {
    if (label === w) total += 0;
    else if (label.startsWith(w)) total += 1;
    else if (label.split(/[\s\-_/]+/).some((x) => x.startsWith(w))) total += 2;
    else if (label.includes(w)) total += 3;
    else if (rest.split(/[\s\-_/]+/).some((x) => x.startsWith(w)) || rest.includes(w)) total += 5;
    else return null;
  }
  return total;
}

const KIND_ORDER: Readonly<Record<OmniKind, number>> = { conversation: 0, action: 1, setting: 2, message: 3, file: 3 };

/**
 * Deterministic and local: what was typed decides first (a better match always wins), then a known conversation before an action before a
 * setting, then the host's own order. Nothing is learned, so a never-used exact match is as findable as a favourite (NV-03). The result is
 * computed in one synchronous pass, so a late arrival can never move a result under the person's finger (NV-06).
 */
export function rankEntries(entries: readonly OmniEntry[], query: string): readonly OmniEntry[] {
  if (query.trim() === "") return entries.filter((e) => e.suggested === true);
  return entries
    .map((e, i) => ({ e, i, s: scoreEntry(e, query) }))
    .filter((x): x is { e: OmniEntry; i: number; s: number } => x.s !== null)
    .sort((a, b) => a.s - b.s || KIND_ORDER[a.e.kind] - KIND_ORDER[b.e.kind] || a.i - b.i)
    .map((x) => x.e);
}
