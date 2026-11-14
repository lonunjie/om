import { useCallback, useEffect, useMemo, useState } from "react";
import type { ConversationSummary, GroupDetailView, GroupId, GroupInviteView, GroupMessageView, GroupSummaryView } from "@om/platform-api";
import type { ConversationDigest } from "../conversations/hooks";
import { groupApiOf } from "./services";
import type { ConversationServices } from "../conversations/services";

/** Core's group path has no change feed yet, so what is on screen is re-read after every step and on a calm interval while it is shown. */
const POLL_MS = 2500;

export interface GroupsState {
  readonly available: boolean;
  readonly groups: readonly GroupSummaryView[];
  readonly invites: readonly GroupInviteView[];
  readonly refresh: () => void;
}

const NONE: readonly never[] = [];

export function useGroups(s: ConversationServices): GroupsState {
  const api = groupApiOf(s);
  const [tick, setTick] = useState(0);
  const refresh = useCallback(() => { setTick((n) => n + 1); }, []);
  useEffect(() => {
    if (api === null) return;
    const id = setInterval(refresh, POLL_MS);
    const onVisible = (): void => { if (document.visibilityState === "visible") refresh(); };
    document.addEventListener("visibilitychange", onVisible);
    return () => { clearInterval(id); document.removeEventListener("visibilitychange", onVisible); };
  }, [api, refresh]);
  const data = useMemo(() => {
    // `tick` is a re-read trigger, not an input.
    // eslint-disable-next-line @typescript-eslint/no-meaningless-void-operator
    void tick;
    if (api === null) return { groups: NONE, invites: NONE };
    try { return { groups: api.list(s.ctx), invites: api.invites(s.ctx) }; } catch { return { groups: NONE, invites: NONE }; } // a stale context after a profile switch
  }, [api, s.ctx, tick]);
  return { available: api !== null, groups: data.groups, invites: data.invites, refresh };
}

export interface GroupChatData {
  readonly detail: GroupDetailView | null;
  readonly messages: readonly GroupMessageView[];
}

export function useGroupChat(s: ConversationServices, groupId: GroupId, refreshKey: number): GroupChatData {
  const api = groupApiOf(s);
  const [tick, setTick] = useState(0);
  useEffect(() => {
    if (api === null) return;
    const id = setInterval(() => { setTick((n) => n + 1); }, POLL_MS);
    return () => { clearInterval(id); };
  }, [api]);
  return useMemo(() => {
    // `tick` and `refreshKey` are re-read triggers, not inputs.
    // eslint-disable-next-line @typescript-eslint/no-meaningless-void-operator
    void tick; void refreshKey;
    if (api === null) return { detail: null, messages: NONE };
    try { return { detail: api.get(s.ctx, groupId), messages: api.messages(s.ctx, groupId) }; } catch { return { detail: null, messages: NONE }; }
  }, [api, s.ctx, groupId, tick, refreshKey]);
}

/**
 * Who is who. Core's group views name people only by account, so a name is known only where something on this device already ties an
 * account to a chat: an invitation names the person on the chat it went over. Your own account is known once you have sent a message.
 * Anyone else is "Someone". (Open question: the group views should carry display names and a "this is you" flag.)
 */
export interface PeopleNames {
  readonly nameOf: (account: string) => string | null;
  readonly isYou: (account: string) => boolean;
}

export function usePeopleNames(invites: readonly GroupInviteView[], chats: readonly ConversationSummary[], messages: readonly GroupMessageView[]): PeopleNames {
  return useMemo(() => {
    const byRel = new Map(chats.map((c) => [c.id as string, c.displayName]));
    const known = new Map<string, string>();
    for (const i of invites) { const n = byRel.get(i.relationshipId); if (n !== undefined) known.set(i.peerAccount, n); }
    const me = messages.find((m) => m.outgoing)?.senderAccount ?? null;
    return { nameOf: (a) => known.get(a) ?? null, isYou: (a) => me !== null && a === me };
  }, [invites, chats, messages]);
}

/** The row of a group in the chat list, in the same shape as a chat's. */
export function digestOfGroup(messages: readonly GroupMessageView[]): ConversationDigest {
  const last = messages[messages.length - 1];
  return {
    lastText: last === undefined ? null : last.text.replace(/\s+/g, " ").trim(),
    lastFromMe: last?.outgoing === true,
    lastDelivery: null,
    lastSentAt: last?.atMs ?? null,
    incomingAt: messages.filter((m) => !m.outgoing).map((m) => m.atMs),
  };
}
