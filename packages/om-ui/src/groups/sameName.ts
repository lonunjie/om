/**
 * Two people who show the same name (GROUPS_AND_SPACES_UX section 10). A name is never a cryptographic identity, so when two
 * collide the screen adds cues that do not depend on the name: a pattern, "1 of 2", and one calm line.
 *
 * Two names collide when they are equal after Unicode NFKC, case folding and trimming (`same`), or when they differ but have the same UTS #39
 * skeleton, which covers cross-script lookalikes such as Cyrillic "a" for Latin "a" and mixed-script names (`alike`, ADR-0197, `lookalike.ts`).
 * A skeleton of fewer than two characters never matches as a lookalike. Names are shown as typed; nothing is blocked or flagged as an attack.
 */
import { lookalikeKey, matchable } from "./lookalike";

export const nameKey = (name: string): string => name.normalize("NFKC").trim().toUpperCase().toLowerCase();

export interface SameName {
  /** The name as the first colliding person shows it. */
  readonly name: string;
  /** 1-based place among the people who share it, in a stable order that does not depend on who is listed first. */
  readonly index: number;
  readonly of: number;
  /** `same`: every name is equal after normalizing. `alike`: at least two differ but look similar. */
  readonly kind: SameKind;
}

export type SameKind = "same" | "alike";

export interface Collisions {
  readonly of: (id: string) => SameName | null;
  /** One entry per shared name, for the one-line note. */
  readonly groups: readonly { readonly name: string; readonly count: number; readonly kind: SameKind }[];
}

export interface Named { readonly id: string; readonly name: string }

/**
 * Finds the people who share a name or have names that look alike. Entries with no name never collide. `id` must be stable and unique per person.
 * Two entries are in one group when their normalized names are equal or their lookalike keys are equal (a chain of such matches is one group).
 */
export function findSameNames(people: readonly Named[]): Collisions {
  const named = people.filter((p) => nameKey(p.name) !== "");
  const parent = named.map((_, i) => i);
  const root = (i: number): number => { let r = i; while (parent[r] !== r) r = parent[r] ?? r; return r; };
  const seen = new Map<string, number>();
  named.forEach((p, i) => {
    const like = lookalikeKey(p.name);
    for (const k of [`e:${nameKey(p.name)}`, ...(matchable(like) ? [`l:${like}`] : [])]) {
      const j = seen.get(k);
      if (j === undefined) seen.set(k, i); else parent[root(i)] = root(j);
    }
  });
  const byRoot = new Map<number, Named[]>();
  named.forEach((p, i) => { const r = root(i); byRoot.set(r, [...(byRoot.get(r) ?? []), p]); });
  const found = new Map<string, SameName>();
  const groups: { name: string; count: number; kind: SameKind }[] = [];
  for (const list of byRoot.values()) {
    const ids = [...new Set(list.map((p) => p.id))].sort();
    if (ids.length < 2) continue;
    const first = list.find((p) => p.id === ids[0]);
    const name = first?.name.trim() ?? "";
    const kind: SameKind = new Set(list.map((p) => nameKey(p.name))).size === 1 ? "same" : "alike";
    groups.push({ name, count: ids.length, kind });
    ids.forEach((id, i) => { found.set(id, { name, index: i + 1, of: ids.length, kind }); });
  }
  return { of: (id) => found.get(id) ?? null, groups };
}
