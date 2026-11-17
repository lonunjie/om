import { findSameNames, type Collisions, type SameName } from "./sameName";
import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import type { GroupDetailView, GroupEvent, GroupId, GroupInviteView, GroupMessageView, GroupNoticeView, GroupSummaryView, HeldGroupInvitations } from "@om/platform-api";
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
 * Who is who, as Core's group view says it: each member's name is the one this device's own contact list gives that account (the same
 * as the 1:1 chat shows; ADR-0189: no new metadata, nothing is read from the Room), and Core marks the person using the device.
 * An account this device has no contact for stays unnamed, and the screen says "Someone" for it. A name is never a cryptographic
 * identity (GROUPS_AND_SPACES_UX section 10).
 */
export interface PeopleNames {
  readonly nameOf: (account: string) => string | null;
  readonly isYou: (account: string) => boolean;
  /** 1, 2, ... for each account with no name, the same in every list and line of this group (so "Someone 2" is always the same person). */
  readonly unnamedNumber: (account: string) => number;
  /** Who among the people now in the group shows the same name as someone else (section 10); null when nobody does. */
  readonly sameName: (account: string) => SameName | null;
  /** One entry per shared name, for the one-line note. */
  readonly sharedNames: Collisions["groups"];
}

export function usePeopleNames(detail: GroupDetailView | null): PeopleNames {
  return useMemo(() => {
    const known = new Map<string, { display: string | null; you: boolean }>();
    for (const p of detail?.people ?? []) known.set(p.account, p);
    for (const p of detail?.former ?? []) if (!known.has(p.account)) known.set(p.account, p);
    const numbers = new Map<string, number>();
    for (const [a, p] of known) if (p.display === null && !p.you) numbers.set(a, numbers.size + 1);
    const shared = findSameNames((detail?.people ?? []).flatMap((p) => (p.display === null || p.you ? [] : [{ id: p.account, name: p.display }])));
    return { nameOf: (a) => known.get(a)?.display ?? null, isYou: (a) => known.get(a)?.you === true, unnamedNumber: (a) => numbers.get(a) ?? numbers.size + 1, sameName: shared.of, sharedNames: shared.groups };
  }, [detail]);
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
      case "commit_prepared": case "message_sent": case "message_changed": case "dropped": case "commit_superseded":
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
