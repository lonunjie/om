import type { ConversationSummary } from "@om/platform-api";

/**
 * The name to show for a contact: the person's own nickname when they set one (ADR-0196), else the name the contact chose.
 * Everything that shows a contact's name goes through this, so the nickname wins everywhere and the same-name cues compare what is shown.
 */
export const shownName = (c: Pick<ConversationSummary, "displayName" | "nickname">): string => (c.nickname === undefined || c.nickname === "" ? c.displayName : c.nickname);
