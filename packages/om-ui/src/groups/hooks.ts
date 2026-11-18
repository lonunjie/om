import { findSameNames, type Collisions, type SameName } from "./sameName";
import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import type { GroupApi, GroupDetailView, GroupEvent, GroupId, GroupInviteView, GroupMessageView, GroupNoticeView, GroupRejoinView, GroupSummaryView, HeldGroupInvitations, ProfileId } from "@om/platform-api";
import type { ConversationDigest } from "../conversations/hooks";
import { groupApiOf } from "./services";
import type { ConversationServices } from "../conversations/services";

/**
 * What is on screen is re-read when Core's change feed says something became durable for this profile (ADR-0209), after every step, and
 * when the tab is shown again. Only a host with no feed (`onChange` absent) falls back to re-reading on this calm interval.
 */
const POLL_MS = 2500;

/**
 * Calls `reread` whenever the group state could read differently: on the feed's change for `profileId`, on the tab becoming visible, and,
 * only where the host has no feed, on the fallback interval. The reads are local (Core's state in this page): no network request is
 * made or caused, and nothing about the feed leaves the device.
 */
function useChangeFeed(api: { readonly onChange?: GroupApi["onChange"] } | undefined, profileId: ProfileId, enabled: boolean, reread: () => void, visibility = true): void {
  const latest = useRef(reread);
  latest.current = reread;
  useEffect(() => {
    if (!enabled || api === undefined) return;
    const fire = (): void => { latest.current(); };
    const feed = api.onChange;
    let off: (() => void) | undefined;
    let id: ReturnType<typeof setInterval> | undefined;
    if (feed === undefined) id = setInterval(fire, POLL_MS);
    else off = feed.call(api, (changed) => { if (changed === profileId) fire(); });
    const onVisible = (): void => { if (document.visibilityState === "visible") fire(); };
    if (visibility) document.addEventListener("visibilitychange", onVisible);
    return () => {
      if (id !== undefined) clearInterval(id);
      off?.();
      if (visibility) document.removeEventListener("visibilitychange", onVisible);
    };
  }, [api, profileId, enabled, visibility]);
}

export interface GroupsState {
  readonly available: boolean;
  readonly groups: readonly GroupSummaryView[];
  readonly invites: readonly GroupInviteView[];
  readonly rejoin: GroupRejoinView;
  readonly refresh: () => void;
}

const NONE: readonly never[] = [];
const NO_REJOIN: GroupRejoinView = { contacts: NONE, pending: NONE, reviews: NONE };

export function useGroups(s: ConversationServices): GroupsState {
  const api = groupApiOf(s);
  const base = s.client.groups;
  const [tick, setTick] = useState(0);
  const refresh = useCallback(() => { setTick((n) => n + 1); }, []);
  useChangeFeed(api ?? base, s.ctx.profileId, api !== null || base?.rejoin !== undefined, refresh);
  const data = useMemo(() => {
    // `tick` is a re-read trigger, not an input.
    // eslint-disable-next-line @typescript-eslint/no-meaningless-void-operator
    void tick;
    try {
      return { groups: api?.list(s.ctx) ?? NONE, invites: api?.invites(s.ctx) ?? NONE, rejoin: base?.rejoin?.(s.ctx) ?? NO_REJOIN };
    } catch {
      return { groups: NONE, invites: NONE, rejoin: NO_REJOIN }; // a stale context after a profile switch
    }
  }, [api, base, s.ctx, tick]);
  return { available: api !== null, ...data, refresh };
}

export interface HeldGroupState {
  readonly held: HeldGroupInvitations;
  /** The person read the overflow notice. */
  readonly acknowledge: () => void;
}
const NO_HELD: HeldGroupInvitations = { invitations: [], overflow: { invites: 0, replies: 0 } };

/**
 * Invitations held before the group module is loaded, read through the base-module query (it works whether or not groups are
 * available; a client without one reports nothing). Re-read on the same change feed as the group views.
 */
export function useHeldGroupInvites(s: ConversationServices): HeldGroupState {
  const api = s.client.groups;
  const has = api?.held !== undefined;
  const [tick, setTick] = useState(0);
  useChangeFeed(api, s.ctx.profileId, has, useCallback(() => { setTick((n) => n + 1); }, []), false);
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
  useChangeFeed(api ?? undefined, s.ctx.profileId, api !== null, useCallback(() => { setTick((n) => n + 1); }, []), false);
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
      case "rejoin_changed": case "device_removed": case "commit_prepared": case "message_sent": case "message_changed": case "dropped": case "commit_superseded":
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
