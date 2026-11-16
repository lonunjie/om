/**
 * Two people who show the same name (GROUPS_AND_SPACES_UX section 10). A name is never a cryptographic identity, so when two
 * collide the screen adds cues that do not depend on the name: a pattern, "1 of 2", and one calm line.
 *
 * The collision rule is exact equality after Unicode NFKC, case folding and trimming whitespace. Lookalike and mixed-script
 * matching (UTS #39) is not done here; it is a recorded open item.
 */
export const nameKey = (name: string): string => name.normalize("NFKC").trim().toUpperCase().toLowerCase();

export interface SameName {
  /** The name as the first colliding person shows it. */
  readonly name: string;
  /** 1-based place among the people who share it, in a stable order that does not depend on who is listed first. */
  readonly index: number;
  readonly of: number;
}

export interface Collisions {
  readonly of: (id: string) => SameName | null;
  /** One entry per shared name, for the one-line note. */
  readonly groups: readonly { readonly name: string; readonly count: number }[];
}

export interface Named { readonly id: string; readonly name: string }

/** Finds the people who share a name. Entries with no name never collide. `id` must be stable and unique per person. */
export function findSameNames(people: readonly Named[]): Collisions {
  const byKey = new Map<string, Named[]>();
  for (const p of people) {
    const k = nameKey(p.name);
    if (k === "") continue;
    byKey.set(k, [...(byKey.get(k) ?? []), p]);
  }
  const found = new Map<string, SameName>();
  const groups: { name: string; count: number }[] = [];
  for (const list of byKey.values()) {
    const ids = [...new Set(list.map((p) => p.id))].sort();
    if (ids.length < 2) continue;
    const first = list.find((p) => p.id === ids[0]);
    const name = first?.name.trim() ?? "";
    groups.push({ name, count: ids.length });
    ids.forEach((id, i) => { found.set(id, { name, index: i + 1, of: ids.length }); });
  }
  return { of: (id) => found.get(id) ?? null, groups };
}
