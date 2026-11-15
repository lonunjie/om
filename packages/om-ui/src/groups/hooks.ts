import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import type { ConversationSummary, GroupDetailView, GroupEvent, GroupId, GroupInviteView, GroupMessageView, GroupNoticeView, GroupSummaryView, HeldGroupInvitations } from "@om/platform-api";
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

export interface HeldGroupState {
  readonly held: HeldGroupInvitations;
  /** The person read the overflow notice. */
  readonly acknowledge: () => void;
}
const NO_HELD: HeldGroupInvitations = { invitations: [], overflow: { invites: 0, replies: 0 } };

/**
 * Invitations held before the group module is loaded, read through the base-module query (it works whether or not groups are
 * available; a client without one reports nothing). Re-read on the same calm interval as the group views.
 */
export function useHeldGroupInvites(s: ConversationServices): HeldGroupState {
  const api = s.client.groups;
  const has = api?.held !== undefined;
  const [tick, setTick] = useState(0);
  useEffect(() => {
    if (!has) return;
    const id = setInterval(() => { setTick((n) => n + 1); }, POLL_MS);
    return () => { clearInterval(id); };
  }, [has]);
  const held = useMemo(() => {
    // `tick` is a re-read trigger, not an input.
    // eslint-disable-next-line @typescript-eslint/no-meaningless-void-operator
    void tick;
    try { return api?.held?.(s.ctx) ?? NO_HELD; } catch { return NO_HELD; } // a stale context after a profile switch
  }, [api, s.ctx, tick]);
  const acknowledge = useCallback(() => {
    const done = (): void => { setTick((n) => n + 1); };
    const p = api?.ackHeldOverflow?.(s.ctx);
    if (p !== undefined) void p.then(done, done);
  }, [api, s.ctx]);
  return { held, acknowledge };
}

export interface GroupChatData {
  readonly detail: GroupDetailView | null;
  readonly messages: readonly GroupMessageView[];
  /** The announced changes of the Room's settings, oldest first. */
  readonly notices: readonly GroupNoticeView[];
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
    if (api === null) return { detail: null, messages: NONE, notices: NONE };
    try { return { detail: api.get(s.ctx, groupId), messages: api.messages(s.ctx, groupId), notices: api.notices(s.ctx, groupId) }; } catch { return { detail: null, messages: NONE, notices: NONE }; }
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

/**
 * What the group carriage says in the background, as states a person can see. Events are facts that happened; this keeps only the
 * ones that still need a line on screen and drops them when they stop being true or the person has seen them. Silent by design:
 * `commit_superseded` (the Commit is retried by Core), `dropped` (nothing changed) and the plain progress events.
 */
export interface GroupStates {
  /** A group update is kept until what it needs arrives. */
  readonly held: boolean;
  /** Frames a delivery service refused for good, not yet acknowledged by the person. */
  readonly undeliverable: number;
  /** A queued settings change that could not be carried out, by group id. */
  readonly settingBlocked: ReadonlyMap<string, string>;
  /** An accepted invitation whose add is not done, by invitation id. */
  readonly admitBlocked: ReadonlyMap<string, string>;
}

export const NO_STATES: GroupStates = { held: false, undeliverable: 0, settingBlocked: new Map(), admitBlocked: new Map() };

export function reduceGroupStates(prev: GroupStates, events: readonly GroupEvent[]): GroupStates {
  let next = prev;
  for (const e of events) {
    switch (e.kind) {
      case "held": next = { ...next, held: true }; break;
      // What a held update waited for has arrived, or the person is in.
      case "joined": case "member_added": case "message_received": next = { ...next, held: false }; break;
      case "undeliverable": next = { ...next, undeliverable: next.undeliverable + e.count }; break;
      case "room_setting_blocked": next = { ...next, settingBlocked: new Map(next.settingBlocked).set(e.conversationId, e.reason) }; break;
      case "admit_blocked": next = { ...next, admitBlocked: new Map(next.admitBlocked).set(e.inviteId, e.reason) }; break;
      case "policy_changed": {
        const blocked = new Map(next.settingBlocked);
        blocked.delete(e.conversationId);
        next = { ...next, settingBlocked: blocked }; // the notice line in the timeline (a live log) is the lasting, announced record
        break;
      }
      case "member_removed": case "no_longer_member": case "created": case "invite_sent": case "invite_received": case "invite_accepted": case "invite_declined":
      case "commit_prepared": case "message_sent": case "dropped": case "commit_superseded":
        break;
    }
  }
  return next;
}

export interface GroupEventsState {
  readonly states: GroupStates;
  /** Counts event batches seen, so a screen can re-read its views the moment something happened instead of on the next poll. */
  readonly version: number;
  /** Feed events a step returned (the same reducer). */
  readonly report: (events: readonly GroupEvent[]) => void;
  readonly dismissUndeliverable: () => void;
  readonly dismissSettingBlocked: (groupId: string) => void;
  readonly dismissAdmitBlocked: (inviteId: string) => void;
}

/** Subscribes to `groups.onEvents` when the host has a driver (a client without one reports nothing and the states stay empty). */
export function useGroupEvents(s: ConversationServices, onEvents?: (events: readonly GroupEvent[]) => void): GroupEventsState {
  const api = groupApiOf(s);
  const [states, setStates] = useState<GroupStates>(NO_STATES);
  const [version, setVersion] = useState(0);
  const cb = useRef(onEvents);
  cb.current = onEvents;
  const report = useCallback((events: readonly GroupEvent[]) => { setStates((p) => reduceGroupStates(p, events)); setVersion((n) => n + 1); cb.current?.(events); }, []);
  const profileId = s.ctx.profileId;
  useEffect(() => {
    setStates(NO_STATES);
    if (api?.onEvents === undefined) return;
    return api.onEvents((pid, events) => { if (pid === profileId) report(events); });
  }, [api, profileId, report]);
  return {
    states, version, report,
    dismissUndeliverable: useCallback(() => { setStates((p) => ({ ...p, undeliverable: 0 })); }, []),
    dismissSettingBlocked: useCallback((g: string) => { setStates((p) => { const m = new Map(p.settingBlocked); m.delete(g); return { ...p, settingBlocked: m }; }); }, []),
    dismissAdmitBlocked: useCallback((i: string) => { setStates((p) => { const m = new Map(p.admitBlocked); m.delete(i); return { ...p, admitBlocked: m }; }); }, []),
  };
}
