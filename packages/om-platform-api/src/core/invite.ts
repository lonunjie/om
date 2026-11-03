// Invitation-link grammar. Parsing never fetches or resolves anything.
//
// Core's canonical form is `om:invite/<token>`. People share a web form, `https://<host>/#invite/<token>`, because a phone
// camera or a chat app can open it: the token sits in the fragment, so it is never sent to any server. Both forms carry the
// same token, and a pasted message that merely contains a link works too (people paste whatever they were sent).
export const INVITE_LINK_MAX_LENGTH = 512;
/** Longest pasted text searched for a link; anything bigger is not a chat message about an invitation. */
export const INVITE_TEXT_MAX_LENGTH = 2000;

// invite id, intro key digest and the introduction services the invite names (`intro_services[]`): 480 characters is 360 bytes.
const TOKEN = "[A-Za-z0-9_-]{16,480}";
// The `om:` form must start the text or follow a space or an opening quote/bracket: not a path inside some other address.
const CANONICAL = new RegExp(`(?<=^|[\\s"'(<])om:invite/(${TOKEN})(?![A-Za-z0-9_-])`);
const WEB = new RegExp(`https?://[A-Za-z0-9.-]+(?::\\d{1,5})?/?#invite/(${TOKEN})(?![A-Za-z0-9_-])`);
const FRAGMENT = new RegExp(`^#/?invite/(${TOKEN})$`);

export type InviteLinkResult =
  | { readonly ok: true; readonly token: string }
  | { readonly ok: false; readonly reason: "empty" | "too_long" | "malformed" };

/** Finds one invitation (`om:invite/<token>` or `https://<host>/#invite/<token>`) in what was pasted, scanned or opened. */
export function parseInviteLink(input: string): InviteLinkResult {
  const s = input.trim();
  if (s === "") return { ok: false, reason: "empty" };
  if (s.length > INVITE_TEXT_MAX_LENGTH) return { ok: false, reason: "too_long" };
  const m = CANONICAL.exec(s) ?? WEB.exec(s);
  if (m?.[1] !== undefined) return { ok: true, token: m[1] };
  // Something link-sized that is not an invitation is "malformed"; a long run of link characters is "too long" to be one.
  return { ok: false, reason: s.length > INVITE_LINK_MAX_LENGTH ? "too_long" : "malformed" };
}

/** The canonical form Core accepts, for a token. */
export function canonicalInviteLink(token: string): string {
  return `om:invite/${token}`;
}

/** The link a person shares: `<origin>/#invite/<token>`. Anything that is not an invitation link comes back unchanged. */
export function webInviteLink(link: string, origin: string): string {
  const parsed = parseInviteLink(link);
  return parsed.ok ? `${origin.replace(/\/+$/, "")}/#invite/${parsed.token}` : link;
}

/** The token in a page address fragment (`#invite/<token>`), or null when the fragment is anything else. */
export function inviteTokenFromFragment(fragment: string): string | null {
  return FRAGMENT.exec(fragment)?.[1] ?? null;
}
