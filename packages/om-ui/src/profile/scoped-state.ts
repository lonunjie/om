// Profile-namespaced UI-local state (drafts, view cache, adaptive navigation, scroll/selection).
// Every key is {profileId, namespace, key}; the store never reads a "current profile", so a switch cannot leak across profiles.
import type { ProfileId } from "@om/platform-api";

/** Async storage seam. UW-3's encrypted store plugs in here; `createMemoryKeyValue` is for tests/dev. */
export interface KeyValue {
  get(key: string): Promise<string | undefined>;
  set(key: string, value: string): Promise<void>;
  delete(key: string): Promise<void>;
  /** Keys that start with `prefix`. */
  keys(prefix: string): Promise<readonly string[]>;
}

export function createMemoryKeyValue(): KeyValue {
  const m = new Map<string, string>();
  return {
    get: (k) => Promise.resolve(m.get(k)),
    set: (k, v) => { m.set(k, v); return Promise.resolve(); },
    delete: (k) => { m.delete(k); return Promise.resolve(); },
    keys: (p) => Promise.resolve([...m.keys()].filter((k) => k.startsWith(p))),
  };
}

export type ScopedNamespace = "drafts" | "view-cache" | "adaptive-navigation" | "scroll" | "selection";
export interface ScopedRef { readonly profileId: ProfileId; readonly namespace: ScopedNamespace; readonly key: string }

export interface ProfileScopedState {
  get(ref: ScopedRef): Promise<string | undefined>;
  set(ref: ScopedRef, value: string): Promise<void>;
  delete(ref: ScopedRef): Promise<void>;
  /** Removes every key of one namespace of one profile (e.g. ClearAdaptiveNavigation); other profiles are untouched. */
  clearNamespace(profileId: ProfileId, namespace: ScopedNamespace): Promise<void>;
  /** Removes all of one profile's UI state. */
  clearProfile(profileId: ProfileId): Promise<void>;
}

// JSON array encoding is unambiguous: the closing quote of each segment stops "p1" from prefix-matching "p10".
const PREFIX = "om.ui.profile";
const encode = (r: ScopedRef): string => JSON.stringify([PREFIX, r.profileId, r.namespace, r.key]);
const prefixOf = (parts: readonly string[]): string => JSON.stringify([PREFIX, ...parts]).slice(0, -1) + ",";

export function createProfileScopedState(kv: KeyValue): ProfileScopedState {
  const clear = async (prefix: string): Promise<void> => {
    for (const k of await kv.keys(prefix)) await kv.delete(k);
  };
  return {
    get: (r) => kv.get(encode(r)),
    set: (r, v) => kv.set(encode(r), v),
    delete: (r) => kv.delete(encode(r)),
    clearNamespace: (p, ns) => clear(prefixOf([p, ns])),
    clearProfile: (p) => clear(prefixOf([p])),
  };
}
