// DEV/TEST ONLY: a simulated `GroupApi` for UI tests (no MLS, no wire, nothing leaves the process). It mirrors the typed
// surface of the real Core (`GroupIntent`, events, views, refusal codes) so screens can be built and tested without the
// engine, which depends on the proposed ADR-0187. Every other person is simulated: `simulateInvite`, `simulateAnswer` and
// `simulateMessage` play what the other side would do. `out` is always empty: the harness has no transport.
import { asGroupId } from "@om/platform-api";
import type {
  ConversationId, GroupApi, GroupDetailView, GroupEvent, GroupId, GroupIntent, GroupInviteView, GroupMemberView, GroupMessageView, GroupNoticeView, GroupProtectionView,
  GroupRoomSetting, GroupSettingChange, GroupStep, GroupSummaryView, HeldGroupInvitation, HeldGroupInvitations, HeldGroupOverflow, ProfileExecutionContext, ProfileId,
} from "@om/platform-api";

const SELF = "acct-self";
const hex = (n: number): string => Array.from(globalThis.crypto.getRandomValues(new Uint8Array(n)), (b) => b.toString(16).padStart(2, "0")).join("");

interface Member { account: string; status: "member" | "invited"; invite?: string }
/** The Room fields of ADR-0189 section 3 (codes as Core's `GroupProtectionView`). */
interface Policy { profile: number; joinRule: number; newDevicePolicy: number; historyPolicy: number; membersMayInvite: boolean; timer: number | null }
interface Group { id: GroupId; name: string; members: Member[]; messages: GroupMessageView[]; epoch: number; policy: Policy; notices: GroupNoticeView[]; quorum: boolean; canManage: boolean }
const BUNDLES: Readonly<Record<number, Policy>> = {
  1: { profile: 1, joinRule: 1, newDevicePolicy: 1, historyPolicy: 1, membersMayInvite: true, timer: null },
  2: { profile: 2, joinRule: 2, newDevicePolicy: 1, historyPolicy: 1, membersMayInvite: false, timer: null },
  3: { profile: 3, joinRule: 2, newDevicePolicy: 2, historyPolicy: 1, membersMayInvite: false, timer: null },
};
const JOIN = (v: number): [string, number] => (v === 1 ? ["invite_only", 1] : v === 2 ? ["admin_approval", 2] : ["open", 0]);
const NEWDEV = (v: number): [string, number] => (v === 2 ? ["admin_approval", 1] : ["automatic", 0]);
const HIST = (v: number): [string, number] => (v === 1 ? ["since_join", 2] : v === 2 ? ["recent_window", 1] : ["full_shared_history", 0]);
const PROF = (v: number): [string, number] => (v === 1 ? ["recommended", 1] : v === 2 ? ["hardened", 2] : v === 3 ? ["maximum_privacy", 3] : ["reserved", 0]);
/** Every visible difference, each marked loosened when the Room became less strict (the same rule as Core's `room::diff`). */
function diffPolicy(b: Policy, a: Policy): GroupSettingChange[] {
  const out: GroupSettingChange[] = [];
  const add = (field: string, from: [string, number], to: [string, number]): void => { out.push({ field, from: from[0], to: to[0], loosened: to[1] < from[1] }); };
  if (b.joinRule !== a.joinRule) add("join_rule", JOIN(b.joinRule), JOIN(a.joinRule));
  if (b.newDevicePolicy !== a.newDevicePolicy) add("new_device_policy", NEWDEV(b.newDevicePolicy), NEWDEV(a.newDevicePolicy));
  if (b.historyPolicy !== a.historyPolicy) add("history_policy", HIST(b.historyPolicy), HIST(a.historyPolicy));
  if (b.profile !== a.profile) add("security_profile", PROF(b.profile), PROF(a.profile));
  if (b.membersMayInvite !== a.membersMayInvite) out.push({ field: "members_may_invite", from: b.membersMayInvite ? "anyone" : "admins_only", to: a.membersMayInvite ? "anyone" : "admins_only", loosened: a.membersMayInvite });
  if (b.timer !== a.timer) {
    const loosened = b.timer !== null && (a.timer === null || a.timer > b.timer);
    out.push({ field: "disappearing_timer", from: b.timer === null ? "off" : String(b.timer), to: a.timer === null ? "off" : String(a.timer), loosened });
  }
  return out;
}
function applySetting(p: Policy, s: GroupRoomSetting): Policy | null {
  if ("protection" in s) return BUNDLES[s.protection] ?? null;
  if ("join_rule" in s) return s.join_rule === 1 || s.join_rule === 2 ? { ...p, joinRule: s.join_rule } : null; // `open` is never reachable from here
  if ("new_device_policy" in s) return s.new_device_policy === 1 || s.new_device_policy === 2 ? { ...p, newDevicePolicy: s.new_device_policy } : null;
  if ("history_policy" in s) return s.history_policy === 1 || s.history_policy === 2 || s.history_policy === 3 ? { ...p, historyPolicy: s.history_policy } : null;
  if ("members_may_invite" in s) return { ...p, membersMayInvite: s.members_may_invite };
  return { ...p, timer: s.disappearing_timer };
}
interface Invite { id: string; groupId: GroupId; groupName: string; dir: "outgoing" | "incoming"; state: GroupInviteView["state"]; relationshipId: ConversationId; peerAccount: string; createdMs: number }
interface Store { groups: Map<GroupId, Group>; invites: Map<string, Invite>; held: Invite[]; overflow: HeldGroupOverflow }

export interface GroupSim extends GroupApi {
  held(ctx: ProfileExecutionContext): HeldGroupInvitations;
  ackHeldOverflow(ctx: ProfileExecutionContext): Promise<void>;
  /** Someone with an active 1:1 invites this person to a group; returns the invitation id. */
  simulateInvite(ctx: ProfileExecutionContext, relationshipId: ConversationId, name: string): string;
  /**
   * The device has (`true`, the default) or has not (`false`) loaded the group module. Without it every group view is empty and every
   * step is refused `groups_unavailable`, and an invitation that arrives is held (see `held`); loading the module hands the held
   * invitations to the group flow as ordinary pending invitations, never accepted.
   */
  setModuleLoaded(loaded: boolean): void;
  /** Invitations that were lost because the held queue was full (what the real Core counts). */
  simulateHeldOverflow(ctx: ProfileExecutionContext, invites: number, replies?: number): void;
  /** The other person answers an invitation this person sent. `false`: no such outgoing invitation. */
  simulateAnswer(ctx: ProfileExecutionContext, inviteId: string, answer: "accept" | "decline"): boolean;
  /** A member sends a text. `false`: not a group or not a member. */
  simulateMessage(ctx: ProfileExecutionContext, conversationId: GroupId, fromAccount: string, text: string): boolean;
  /** Another member changes a Room setting (everyone gets the notice). `false`: not a group or the setting is not reachable. */
  simulateSettingChange(ctx: ProfileExecutionContext, conversationId: GroupId, fromAccount: string, setting: GroupRoomSetting): boolean;
  /** The Room needs more than one holder's agreement to loosen: a loosening is refused `quorum_not_supported` (Core deviation D39). */
  simulateQuorum(ctx: ProfileExecutionContext, conversationId: GroupId): boolean;
  /** This person stops holding the capability to change Room settings: `room_setting` is refused `not_authorized`. */
  simulateNoAuthority(ctx: ProfileExecutionContext, conversationId: GroupId): boolean;
  /** Background events as a host driver reports them (`held`, `undeliverable`, `room_setting_blocked`, ...), delivered to `onEvents` listeners. */
  simulateEvent(ctx: ProfileExecutionContext, event: GroupEvent): void;
}

/** `stateFor` validates the context (and throws for a stale or foreign one); `hasRelationship` is the 1:1 conversation check. */
function protectionOf(p: Policy): GroupProtectionView {
  const b = BUNDLES[p.profile];
  const custom = b === undefined || b.joinRule !== p.joinRule || b.newDevicePolicy !== p.newDevicePolicy || b.historyPolicy !== p.historyPolicy || b.membersMayInvite !== p.membersMayInvite;
  return { profile: p.profile, custom, joinRule: p.joinRule, newDevicePolicy: p.newDevicePolicy, historyPolicy: p.historyPolicy, membersMayInvite: p.membersMayInvite, disappearingTimer: p.timer };
}

export function createGroupSim(opts: { readonly stateFor: (ctx: ProfileExecutionContext) => unknown; readonly hasRelationship: (ctx: ProfileExecutionContext, id: ConversationId) => boolean; readonly clock: () => number }): GroupSim {
  const stores = new Map<string, Store>();
  const listeners = new Set<(profileId: ProfileId, events: readonly GroupEvent[]) => void>();
  const emit = (ctx: ProfileExecutionContext, events: readonly GroupEvent[]): void => { for (const l of [...listeners]) l(ctx.profileId, events); };
  const store = (ctx: ProfileExecutionContext): Store => {
    opts.stateFor(ctx);
    let s = stores.get(ctx.profileId);
    if (!s) { s = { groups: new Map(), invites: new Map(), held: [], overflow: { invites: 0, replies: 0 } }; stores.set(ctx.profileId, s); }
    return s;
  };
  const acct = (rel: ConversationId): string => `acct-${rel}`;
  const refuse = (refused: string): GroupStep => ({ ok: false, refused });
  const done = (events: GroupEvent[]): GroupStep => ({ ok: true, events, out: [] });
  const summary = (g: Group): GroupSummaryView => ({ conversationId: g.id, name: g.name, members: g.members.filter((m) => m.status === "member").length, invited: g.members.filter((m) => m.status === "invited").length, epoch: g.epoch });
  const newInvite = (s: Store, i: Omit<Invite, "id" | "createdMs">): Invite => {
    const inv: Invite = { ...i, id: hex(16), createdMs: opts.clock() };
    s.invites.set(inv.id, inv);
    return inv;
  };
  let loaded = true;
  const none = (): Promise<GroupStep> => Promise.resolve(refuse("groups_unavailable"));
  const selfIn = (g: Group): boolean => g.members.some((m) => m.account === SELF && m.status === "member");

  return {
    available: () => loaded,
    do(ctx, intent: GroupIntent) {
      if (!loaded) return none();
      const s = store(ctx);
      const g = "conversationId" in intent ? s.groups.get(intent.conversationId) : undefined;
      switch (intent.kind) {
        case "create": case "create_with_protection": {
          for (const r of intent.relationshipIds) if (!opts.hasRelationship(ctx, r)) return Promise.resolve(refuse("unknown_relationship"));
          if (intent.name.trim() === "") return Promise.resolve(refuse("invalid_name"));
          const id = asGroupId(hex(16));
          const level = intent.kind === "create_with_protection" ? intent.protection : 1;
          const policy = BUNDLES[level];
          if (policy === undefined) return Promise.resolve(refuse("security_profile_reserved"));
          const grp: Group = { id, name: intent.name.trim(), members: [{ account: SELF, status: "member" }], messages: [], epoch: 0, policy, notices: [], quorum: false, canManage: true };
          s.groups.set(id, grp);
          const events: GroupEvent[] = [{ kind: "created", conversationId: id }];
          for (const r of intent.relationshipIds) {
            const inv = newInvite(s, { groupId: id, groupName: grp.name, dir: "outgoing", state: "offered", relationshipId: r, peerAccount: acct(r) });
            grp.members.push({ account: acct(r), status: "invited", invite: inv.id });
            events.push({ kind: "invite_sent", inviteId: inv.id });
          }
          return Promise.resolve(done(events));
        }
        case "invite": {
          if (!g || !selfIn(g)) return Promise.resolve(refuse("unknown_group"));
          if (!opts.hasRelationship(ctx, intent.relationshipId)) return Promise.resolve(refuse("unknown_relationship"));
          if (g.members.some((m) => m.account === acct(intent.relationshipId))) return Promise.resolve(refuse("already_member"));
          const inv = newInvite(s, { groupId: g.id, groupName: g.name, dir: "outgoing", state: "offered", relationshipId: intent.relationshipId, peerAccount: acct(intent.relationshipId) });
          g.members.push({ account: acct(intent.relationshipId), status: "invited", invite: inv.id });
          return Promise.resolve(done([{ kind: "invite_sent", inviteId: inv.id }]));
        }
        case "accept": case "decline": {
          const inv = s.invites.get(intent.inviteId);
          if (inv?.dir !== "incoming" || inv.state !== "offered") return Promise.resolve(refuse("unknown_invite"));
          if (intent.kind === "decline") { inv.state = "declined"; return Promise.resolve(done([{ kind: "invite_declined", inviteId: inv.id }])); }
          inv.state = "done";
          s.groups.set(inv.groupId, { id: inv.groupId, name: inv.groupName, members: [{ account: inv.peerAccount, status: "member" }, { account: SELF, status: "member" }], messages: [], epoch: 1, policy: BUNDLES[1] as Policy, notices: [], quorum: false, canManage: true });
          return Promise.resolve(done([{ kind: "invite_accepted", inviteId: inv.id }, { kind: "joined", conversationId: inv.groupId }]));
        }
        case "leave": {
          if (!g || !selfIn(g)) return Promise.resolve(refuse("unknown_group"));
          s.groups.delete(g.id);
          return Promise.resolve(done([{ kind: "no_longer_member", conversationId: g.id }]));
        }
        case "remove": {
          if (!g || !selfIn(g)) return Promise.resolve(refuse("unknown_group"));
          if (intent.account === SELF || !g.members.some((m) => m.account === intent.account && m.status === "member")) return Promise.resolve(refuse("not_a_member"));
          g.members = g.members.filter((m) => m.account !== intent.account);
          g.epoch += 1;
          return Promise.resolve(done([{ kind: "member_removed", conversationId: g.id, account: intent.account }]));
        }
        case "send_text": {
          if (!g || !selfIn(g)) return Promise.resolve(refuse("unknown_group"));
          if (intent.text.trim() === "") return Promise.resolve(refuse("empty_message"));
          const id = hex(16);
          g.messages.push({ id, senderAccount: SELF, senderDevice: "dev-self", epoch: g.epoch, text: intent.text, atMs: opts.clock(), outgoing: true });
          return Promise.resolve(done([{ kind: "message_sent", conversationId: g.id, id }]));
        }
        case "room_setting": {
          if (!g || !selfIn(g)) return Promise.resolve(refuse("not_a_member"));
          if (!g.canManage) return Promise.resolve(refuse("not_authorized"));
          const next = applySetting(g.policy, intent.setting);
          if (next === null) return Promise.resolve(refuse("setting_not_reachable"));
          const changes = diffPolicy(g.policy, next);
          const loosens = changes.some((c) => c.loosened);
          if (loosens && intent.confirmed !== true) return Promise.resolve(refuse("loosening_needs_confirmation"));
          if (loosens && g.quorum) return Promise.resolve(refuse("quorum_not_supported"));
          if (changes.length === 0) return Promise.resolve(done([]));
          g.policy = next;
          g.epoch += 1;
          g.notices.push({ atMs: opts.clock(), byAccount: SELF, changes });
          return Promise.resolve(done([{ kind: "policy_changed", conversationId: g.id, byAccount: SELF, changes }]));
        }
        case "commit_published":
          return Promise.resolve(refuse("unknown_intent")); // the simulation commits at once: there is never a prepared Commit
      }
    },
    receive: () => Promise.resolve(refuse("no_transport")),
    list: (ctx) => (loaded ? [...store(ctx).groups.values()].map(summary) : []),
    get(ctx, id): GroupDetailView | null {
      const g = loaded ? store(ctx).groups.get(id) : undefined;
      if (!g) return null;
      const people: GroupMemberView[] = g.members.map((m) => ({ account: m.account, roles: m.account === SELF || m === g.members[0] ? ["admin"] : ["member"], status: m.status }));
      return { summary: summary(g), people, canSend: selfIn(g), protection: protectionOf(g.policy) };
    },
    invites: (ctx) => (loaded ? [...store(ctx).invites.values()] : []).map((i): GroupInviteView => ({ id: i.id, conversationId: i.groupId, groupName: i.groupName, dir: i.dir, state: i.state, relationshipId: i.relationshipId, peerAccount: i.peerAccount, createdMs: i.createdMs })),
    messages: (ctx, id) => [...(store(ctx).groups.get(id)?.messages ?? [])],
    notices: (ctx, id) => [...(store(ctx).groups.get(id)?.notices ?? [])],
    onEvents: (listener) => { listeners.add(listener); return () => { listeners.delete(listener); }; },
    settle: (ctx) => { store(ctx); return loaded ? Promise.resolve(done([])) : none(); },
    held(ctx): HeldGroupInvitations {
      const s = store(ctx);
      const invitations: HeldGroupInvitation[] = loaded ? [] : s.held.map((i) => ({ relationshipId: i.relationshipId, inviteId: i.id, groupName: i.groupName, receivedMs: i.createdMs }));
      return { invitations, overflow: { ...s.overflow } };
    },
    ackHeldOverflow(ctx) { store(ctx).overflow = { invites: 0, replies: 0 }; return Promise.resolve(); },
    setModuleLoaded(next) {
      loaded = next;
      if (!next) return;
      for (const s of stores.values()) { for (const i of s.held) s.invites.set(i.id, i); s.held = []; } // the lane is pumped: pending, not accepted
    },
    simulateHeldOverflow(ctx, invites, replies = 0) { store(ctx).overflow = { invites, replies }; },

    simulateInvite(ctx, relationshipId, name) {
      const s = store(ctx);
      if (!opts.hasRelationship(ctx, relationshipId)) throw new Error("unknown relationship");
      if (!loaded) {
        const inv: Invite = { id: hex(16), groupId: asGroupId(hex(16)), groupName: name, dir: "incoming", state: "offered", relationshipId, peerAccount: acct(relationshipId), createdMs: opts.clock() };
        s.held.push(inv);
        return inv.id;
      }
      return newInvite(s, { groupId: asGroupId(hex(16)), groupName: name, dir: "incoming", state: "offered", relationshipId, peerAccount: acct(relationshipId) }).id;
    },
    simulateAnswer(ctx, inviteId, answer) {
      const s = store(ctx);
      const inv = s.invites.get(inviteId);
      const g = inv ? s.groups.get(inv.groupId) : undefined;
      if (inv?.dir !== "outgoing" || inv.state !== "offered" || !g) return false;
      const m = g.members.find((x) => x.invite === inviteId);
      if (!m) return false;
      if (answer === "decline") { inv.state = "declined"; g.members = g.members.filter((x) => x !== m); return true; }
      inv.state = "done";
      m.status = "member";
      g.epoch += 1;
      return true;
    },
    simulateSettingChange(ctx, conversationId, fromAccount, setting) {
      const g = store(ctx).groups.get(conversationId);
      if (!g) return false;
      const next = applySetting(g.policy, setting);
      if (next === null) return false;
      const changes = diffPolicy(g.policy, next);
      if (changes.length === 0) return false;
      g.policy = next;
      g.epoch += 1;
      g.notices.push({ atMs: opts.clock(), byAccount: fromAccount, changes });
      emit(ctx, [{ kind: "policy_changed", conversationId, byAccount: fromAccount, changes }]);
      return true;
    },
    simulateQuorum(ctx, conversationId) {
      const g = store(ctx).groups.get(conversationId);
      if (!g) return false;
      g.quorum = true;
      return true;
    },
    simulateNoAuthority(ctx, conversationId) {
      const g = store(ctx).groups.get(conversationId);
      if (!g) return false;
      g.canManage = false;
      return true;
    },
    simulateEvent(ctx, event) { store(ctx); emit(ctx, [event]); },
    simulateMessage(ctx, conversationId, fromAccount, text) {
      const g = store(ctx).groups.get(conversationId);
      if (!g || !g.members.some((m) => m.account === fromAccount && m.status === "member")) return false;
      g.messages.push({ id: hex(16), senderAccount: fromAccount, senderDevice: `dev-${fromAccount}`, epoch: g.epoch, text, atMs: opts.clock(), outgoing: false });
      return true;
    },
  };
}
