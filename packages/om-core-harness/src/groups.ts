// DEV/TEST ONLY: a simulated `GroupApi` for UI tests (no MLS, no wire, nothing leaves the process). It mirrors the typed
// surface of the real Core (`GroupIntent`, events, views, refusal codes) so screens can be built and tested without the
// engine, which depends on the proposed ADR-0187. Every other person is simulated: `simulateInvite`, `simulateAnswer` and
// `simulateMessage` play what the other side would do. `out` is always empty: the harness has no transport.
import { asGroupId } from "@om/platform-api";
import type {
  ConversationId, GroupApi, GroupDetailView, GroupEvent, GroupId, GroupIntent, GroupInviteView, GroupMemberView, GroupMessageView, GroupStep,
  GroupSummaryView, ProfileExecutionContext,
} from "@om/platform-api";

const SELF = "acct-self";
const hex = (n: number): string => Array.from(globalThis.crypto.getRandomValues(new Uint8Array(n)), (b) => b.toString(16).padStart(2, "0")).join("");

interface Member { account: string; status: "member" | "invited"; invite?: string }
interface Group { id: GroupId; name: string; members: Member[]; messages: GroupMessageView[]; epoch: number }
interface Invite { id: string; groupId: GroupId; groupName: string; dir: "outgoing" | "incoming"; state: GroupInviteView["state"]; relationshipId: ConversationId; peerAccount: string; createdMs: number }
interface Store { groups: Map<GroupId, Group>; invites: Map<string, Invite> }

export interface GroupSim extends GroupApi {
  /** Someone with an active 1:1 invites this person to a group; returns the invitation id. */
  simulateInvite(ctx: ProfileExecutionContext, relationshipId: ConversationId, name: string): string;
  /** The other person answers an invitation this person sent. `false`: no such outgoing invitation. */
  simulateAnswer(ctx: ProfileExecutionContext, inviteId: string, answer: "accept" | "decline"): boolean;
  /** A member sends a text. `false`: not a group or not a member. */
  simulateMessage(ctx: ProfileExecutionContext, conversationId: GroupId, fromAccount: string, text: string): boolean;
}

/** `stateFor` validates the context (and throws for a stale or foreign one); `hasRelationship` is the 1:1 conversation check. */
export function createGroupSim(opts: { readonly stateFor: (ctx: ProfileExecutionContext) => unknown; readonly hasRelationship: (ctx: ProfileExecutionContext, id: ConversationId) => boolean; readonly clock: () => number }): GroupSim {
  const stores = new Map<string, Store>();
  const store = (ctx: ProfileExecutionContext): Store => {
    opts.stateFor(ctx);
    let s = stores.get(ctx.profileId);
    if (!s) { s = { groups: new Map(), invites: new Map() }; stores.set(ctx.profileId, s); }
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
  const selfIn = (g: Group): boolean => g.members.some((m) => m.account === SELF && m.status === "member");

  return {
    available: () => true,
    do(ctx, intent: GroupIntent) {
      const s = store(ctx);
      const g = "conversationId" in intent ? s.groups.get(intent.conversationId) : undefined;
      switch (intent.kind) {
        case "create": {
          for (const r of intent.relationshipIds) if (!opts.hasRelationship(ctx, r)) return Promise.resolve(refuse("unknown_relationship"));
          if (intent.name.trim() === "") return Promise.resolve(refuse("invalid_name"));
          const id = asGroupId(hex(16));
          const grp: Group = { id, name: intent.name.trim(), members: [{ account: SELF, status: "member" }], messages: [], epoch: 0 };
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
          s.groups.set(inv.groupId, { id: inv.groupId, name: inv.groupName, members: [{ account: inv.peerAccount, status: "member" }, { account: SELF, status: "member" }], messages: [], epoch: 1 });
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
        case "commit_published": case "commit_accepted":
          return Promise.resolve(refuse("unknown_intent")); // the simulation commits at once: there is never a prepared Commit
      }
    },
    receive: () => Promise.resolve(refuse("no_transport")),
    list: (ctx) => [...store(ctx).groups.values()].map(summary),
    get(ctx, id): GroupDetailView | null {
      const g = store(ctx).groups.get(id);
      if (!g) return null;
      const people: GroupMemberView[] = g.members.map((m) => ({ account: m.account, roles: m.account === SELF || m === g.members[0] ? ["admin"] : ["member"], status: m.status }));
      return { summary: summary(g), people, canSend: selfIn(g) };
    },
    invites: (ctx) => [...store(ctx).invites.values()].map((i): GroupInviteView => ({ id: i.id, conversationId: i.groupId, groupName: i.groupName, dir: i.dir, state: i.state, relationshipId: i.relationshipId, peerAccount: i.peerAccount, createdMs: i.createdMs })),
    messages: (ctx, id) => [...(store(ctx).groups.get(id)?.messages ?? [])],

    simulateInvite(ctx, relationshipId, name) {
      const s = store(ctx);
      if (!opts.hasRelationship(ctx, relationshipId)) throw new Error("unknown relationship");
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
    simulateMessage(ctx, conversationId, fromAccount, text) {
      const g = store(ctx).groups.get(conversationId);
      if (!g || !g.members.some((m) => m.account === fromAccount && m.status === "member")) return false;
      g.messages.push({ id: hex(16), senderAccount: fromAccount, senderDevice: `dev-${fromAccount}`, epoch: g.epoch, text, atMs: opts.clock(), outgoing: false });
      return true;
    },
  };
}
