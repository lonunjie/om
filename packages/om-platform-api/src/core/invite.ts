// Strict `om:` invitation-link grammar. Parsing never fetches or resolves anything.
export const INVITE_LINK_MAX_LENGTH = 256;
const INVITE_LINK = /^om:invite\/([A-Za-z0-9_-]{16,200})$/;

export type InviteLinkResult =
  | { readonly ok: true; readonly token: string }
  | { readonly ok: false; readonly reason: "empty" | "too_long" | "malformed" };

/** Accepts exactly `om:invite/<token>`; surrounding whitespace is trimmed, everything else is rejected. */
export function parseInviteLink(input: string): InviteLinkResult {
  const s = input.trim();
  if (s === "") return { ok: false, reason: "empty" };
  if (s.length > INVITE_LINK_MAX_LENGTH) return { ok: false, reason: "too_long" };
  const m = INVITE_LINK.exec(s);
  return m?.[1] === undefined ? { ok: false, reason: "malformed" } : { ok: true, token: m[1] };
}
