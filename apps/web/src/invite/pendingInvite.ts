// An invitation link that was opened in this tab (a camera scan or a chat app opened `https://<host>/#invite/<token>`).
// The token lives in the page address fragment, so no server ever sees it. It is read once, removed from the address bar
// (a reload or a screenshot of the address does not repeat it) and kept for this tab only, so it survives a reload while
// the person sets up or unlocks, and is forgotten when the tab closes. Nothing is sent until the person taps Accept.
import { useSyncExternalStore } from "react";
import { inviteTokenFromFragment } from "@om/platform-api";

const KEY = "om.pending-invite";

let current: string | null | undefined; // undefined: not read from session storage yet
const listeners = new Set<() => void>();

function stored(): string | null {
  try { return globalThis.sessionStorage.getItem(KEY); } catch { return null; }
}

export function getPendingInvite(): string | null {
  if (current === undefined) current = stored();
  return current;
}

export function setPendingInvite(token: string | null): void {
  current = token;
  try { if (token === null) globalThis.sessionStorage.removeItem(KEY); else globalThis.sessionStorage.setItem(KEY, token); } catch { /* blocked storage: it still works until a reload */ }
  for (const l of listeners) l();
}

export const clearPendingInvite = (): void => { setPendingInvite(null); };

const subscribe = (l: () => void): (() => void) => { listeners.add(l); return () => { listeners.delete(l); }; };

/** The opened invitation's token, or null. Re-renders when one arrives or is dealt with. */
export function usePendingInvite(): string | null {
  return useSyncExternalStore(subscribe, getPendingInvite, () => null);
}

type Win = Pick<Window, "location" | "history">;

/** Takes an invitation out of the address fragment, if there is one. Returns whether it found one. */
export function captureInviteFromAddress(win: Win = window): boolean {
  const token = inviteTokenFromFragment(win.location.hash);
  if (token === null) return false;
  setPendingInvite(token);
  try { win.history.replaceState(null, "", win.location.pathname + win.location.search); } catch { /* address stays; the token is already held */ }
  return true;
}

/** Captures the invitation now and whenever the address fragment changes while the app is open (a link pasted into the same tab). */
export function watchInviteAddress(win: Window = window): void {
  captureInviteFromAddress(win);
  win.addEventListener("hashchange", () => { captureInviteFromAddress(win); });
}

// After an invitation is accepted, Core adds the chat straight away (it shows "waiting for them to accept"). The shell opens
// that chat so the person sees what happened; this remembers which chats existed at that moment, so the new one is recognised.
let expected: readonly string[] | null = null;

export function expectNewChat(knownIds: readonly string[]): void {
  expected = knownIds;
  for (const l of listeners) l();
}

export const clearExpectedNewChat = (): void => { expected = null; for (const l of listeners) l(); };

/** The chats that existed when an invitation was accepted, or null when no new chat is being waited for. */
export function useExpectedNewChat(): readonly string[] | null {
  return useSyncExternalStore(subscribe, () => expected, () => null);
}
