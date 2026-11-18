import type { ConversationId, ConversationSummary, IntroductionRequest } from "@om/platform-api";
import type { GroupsCopy } from "../groups/copy";
import { findSameNames, type Collisions, type Named, type SameName } from "../groups/sameName";
import { shownName } from "./shownName";

/**
 * The same-name and lookalike cues (GROUPS_AND_SPACES_UX section 10, ADR-0197) on the one-to-one surfaces. The logic and the words are the groups
 * ones, reused unchanged: this file only picks what is compared (the shown names, so a nickname wins, ADR-0196) and which line goes with it.
 */
export type SameNameCopy = GroupsCopy["sameName"];

/** Your one-to-one contacts that show the same name or names that look alike. Group rows (`isGroup`) are not people and are left out. */
export function contactCollisions(rows: readonly Pick<ConversationSummary, "id" | "displayName" | "nickname">[], isGroup?: (id: ConversationId) => boolean): Collisions {
  return findSameNames(rows.filter((c) => isGroup?.(c.id) !== true).map((c) => ({ id: c.id, name: shownName(c) })));
}

/** The one calm line for a list of your own contacts. */
export const contactsLine = (copy: SameNameCopy, s: SameName): string => (s.kind === "alike" ? copy.alike : copy.contacts)(s.name, s.of);

/**
 * Whether a person asking to message you shows a name that equals, or looks like, the shown name of a contact, with the place among those who share it.
 * The requester has no nickname (their name is their own claim), so it is compared as sent; contacts are compared by their shown names. Null when
 * nothing collides. Nothing here blocks or changes the request.
 */
export function requestCollision(requestId: string, requestName: string, contacts: readonly Named[]): SameName | null {
  return introductionCollisions(contacts, [{ id: requestId, displayName: requestName }])(requestId);
}

/** Compare the visible requests with each other and with the current profile's contacts. */
export function introductionCollisions(contacts: readonly Named[], requests: readonly Pick<IntroductionRequest, "id" | "displayName">[]): (id: string) => SameName | null {
  const key = (id: string): string => `request:${id}`;
  const matches = findSameNames([
    ...contacts.map(({ id, name }) => ({ id: `contact:${id}`, name })),
    ...requests.map(({ id, displayName }) => ({ id: key(id), name: displayName })),
  ]);
  return (id) => matches.of(key(id));
}

/** The one calm line for a request: "N people here are named X" or "... have names that look similar to X". */
export const requestLine = (copy: SameNameCopy, s: SameName): string => (s.kind === "alike" ? copy.alike : copy.here)(s.name, s.of);
