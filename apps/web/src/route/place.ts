// Where in the app a person is, as a web address. Changing place pushes a history entry, so the phone's Back button, the
// browser's Back/Forward and a reload all do what a person expects; acting inside a place (a sheet, a dialog) does not.
//
//   /                  chats (the root)          /settings/devices   Settings > Devices
//   /chats#<id>        one chat                  /settings/recovery  Settings > Recovery
//   /settings          Settings                  /settings/delivery  Settings > Message delivery
//   /safety-code       the safety-code steps
//
// A chat's id goes in the fragment, not the path: a fragment never leaves the device, so no host log learns which chat is open.
// Invitation links use `/#invite/<token>` and are read and removed by `invite/pendingInvite.ts` before anything else.
import { useCallback, useSyncExternalStore } from "react";

export type SettingsPage = "menu" | "devices" | "recovery" | "delivery";
export type Place =
  | { readonly kind: "chats"; readonly chat: string | null }
  | { readonly kind: "settings"; readonly page: SettingsPage }
  | { readonly kind: "safety-code" };

export const CHATS: Place = { kind: "chats", chat: null };
const SETTINGS_PATH: Readonly<Record<SettingsPage, string>> = { menu: "/settings", devices: "/settings/devices", recovery: "/settings/recovery", delivery: "/settings/delivery" };
const trim = (p: string): string => (p.length > 1 ? p.replace(/\/+$/, "") : p);

export function parsePlace(pathname: string, hash: string): Place {
  const path = trim(pathname);
  for (const page of Object.keys(SETTINGS_PATH) as SettingsPage[]) if (SETTINGS_PATH[page] === path) return { kind: "settings", page };
  if (path === "/safety-code") return { kind: "safety-code" };
  if (path === "/chats") {
    let id = "";
    try { id = decodeURIComponent(hash.replace(/^#/, "")); } catch { /* a malformed fragment is no chat */ }
    return { kind: "chats", chat: id === "" || id.startsWith("invite/") ? null : id };
  }
  return CHATS; // "/", "/index.html" and anything unknown: the root, never an error page
}

export function placeUrl(place: Place): string {
  switch (place.kind) {
    case "settings": return SETTINGS_PATH[place.page];
    case "safety-code": return "/safety-code";
    case "chats": return place.chat === null ? "/" : `/chats#${encodeURIComponent(place.chat)}`;
  }
}

export const samePlace = (a: Place, b: Place): boolean => placeUrl(a) === placeUrl(b);

/** The tab title for a place. Never a chat name: titles show in task switchers and history lists. */
export const placeTitle = (place: Place, names: { readonly app: string; readonly chats: string; readonly settings: string; readonly safetyCode: string }): string => {
  const head = place.kind === "settings" ? names.settings : place.kind === "safety-code" ? names.safetyCode : names.chats;
  return `${head} · ${names.app}`;
};

const EVENT = "om:place";
interface HistoryMark { readonly om: number }
const depth = (): number => ((globalThis.history.state as Partial<HistoryMark> | null)?.om ?? 0);

const snapshot = (): string => globalThis.location.pathname + globalThis.location.hash;
const subscribe = (notify: () => void): (() => void) => {
  globalThis.addEventListener("popstate", notify);
  globalThis.addEventListener("hashchange", notify);
  globalThis.addEventListener(EVENT, notify);
  return () => {
    globalThis.removeEventListener("popstate", notify);
    globalThis.removeEventListener("hashchange", notify);
    globalThis.removeEventListener(EVENT, notify);
  };
};

/** Go to a place: a new history entry, or (replace) the current one when the old address should not stay reachable. */
export function navigate(place: Place, opts: { readonly replace?: boolean } = {}): void {
  const url = placeUrl(place);
  if (url === snapshot()) return;
  try {
    if (opts.replace === true) globalThis.history.replaceState({ om: depth() }, "", url);
    else globalThis.history.pushState({ om: depth() + 1 }, "", url);
  } catch {
    return; // a sandbox that forbids history leaves the person where they are
  }
  globalThis.dispatchEvent(new Event(EVENT));
}

/** Back to where the person came from inside the app; when this address was opened directly, to `fallback` instead of out of the app. */
export function goBack(fallback: Place): void {
  if (depth() > 0) globalThis.history.back();
  else navigate(fallback, { replace: true });
}

export function usePlace(): { readonly place: Place; /** The address as it is in the bar, before it is made canonical. */ readonly raw: string; readonly go: typeof navigate; readonly back: typeof goBack } {
  const addr = useSyncExternalStore(subscribe, snapshot, () => "/");
  const hashAt = addr.indexOf("#");
  const place = parsePlace(hashAt < 0 ? addr : addr.slice(0, hashAt), hashAt < 0 ? "" : addr.slice(hashAt));
  const go = useCallback(navigate, []);
  return { place, raw: addr, go, back: goBack };
}
