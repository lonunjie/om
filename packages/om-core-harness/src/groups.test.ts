import { describe, expect, it } from "vitest";
import { asConversationId, asPersonId, asProfileId } from "@om/platform-api";
import type { GroupStep } from "@om/platform-api";
import { createHarness, ProfileAccessError } from "./harness";

const A = asProfileId("profile-a");
const B = asProfileId("profile-b");
const CA = asConversationId("conv-a1");
const CB = asConversationId("conv-b1");

function setup() {
  const h = createHarness({ clock: () => 1000 });
  const a = h.seedProfile({ profileId: A, label: "A", conversations: [{ id: CA, personId: asPersonId("pa"), displayName: "Ann" }] });
  const b = h.seedProfile({ profileId: B, label: "B", conversations: [{ id: CB, personId: asPersonId("pb"), displayName: "Bo" }] });
  return { h, a, b };
}
const ok = (s: GroupStep): Extract<GroupStep, { ok: true }> => {
  if (!s.ok) throw new Error(s.refused);
  return s;
};

describe("simulated groups (same typed surface as the real Core)", () => {
  it("create, the invitee answers, text, leave", async () => {
    const { h, a } = setup();
    expect(h.groups.available()).toBe(true);
    const created = ok(await h.groups.do(a, { kind: "create", name: " Book club ", relationshipIds: [CA] }));
    expect(created.events.map((e) => e.kind)).toEqual(["created", "invite_sent"]);
    expect(created.out).toEqual([]);
    const [g] = h.groups.list(a);
    if (!g) throw new Error("no group");
    expect(g).toMatchObject({ name: "Book club", members: 1, invited: 1 });
    // an invited person is shown, not a member; nothing is sent to the group until they accept
    expect(h.groups.get(a, g.conversationId)?.people.map((p) => p.status)).toEqual(["member", "invited"]);
    const [inv] = h.groups.invites(a);
    expect(inv).toMatchObject({ dir: "outgoing", state: "offered", groupName: "Book club" });
    expect(h.groups.simulateAnswer(a, inv?.id ?? "", "accept")).toBe(true);
    expect(h.groups.list(a)[0]).toMatchObject({ members: 2, invited: 0 });
    ok(await h.groups.do(a, { kind: "send_text", conversationId: g.conversationId, text: "hello" }));
    expect(h.groups.simulateMessage(a, g.conversationId, "acct-conv-a1", "hi")).toBe(true);
    expect(h.groups.messages(a, g.conversationId).map((m) => [m.text, m.outgoing])).toEqual([["hello", true], ["hi", false]]);
    ok(await h.groups.do(a, { kind: "remove", conversationId: g.conversationId, account: "acct-conv-a1" }));
    expect(h.groups.list(a)[0]?.members).toBe(1);
    ok(await h.groups.do(a, { kind: "leave", conversationId: g.conversationId }));
    expect(h.groups.list(a)).toEqual([]);
    expect(h.groups.get(a, g.conversationId)).toBeNull();
  });

  it("an incoming invitation is accepted or declined; refusals are typed and change nothing", async () => {
    const { h, a } = setup();
    const id = h.groups.simulateInvite(a, CA, "Chess");
    expect(h.groups.invites(a)[0]).toMatchObject({ dir: "incoming", state: "offered", groupName: "Chess" });
    expect(await h.groups.do(a, { kind: "decline", inviteId: "nope" })).toEqual({ ok: false, refused: "unknown_invite" });
    expect(await h.groups.do(a, { kind: "create", name: "x", relationshipIds: [asConversationId("zz")] })).toEqual({ ok: false, refused: "unknown_relationship" });
    expect(h.groups.list(a)).toEqual([]);
    const r = ok(await h.groups.do(a, { kind: "accept", inviteId: id }));
    expect(r.events.map((e) => e.kind)).toEqual(["invite_accepted", "joined"]);
    expect(h.groups.list(a)[0]).toMatchObject({ name: "Chess", members: 2 });
    expect(await h.groups.do(a, { kind: "accept", inviteId: id })).toEqual({ ok: false, refused: "unknown_invite" });
    expect(await h.groups.do(a, { kind: "commit_accepted", intentId: "x" })).toEqual({ ok: false, refused: "unknown_intent" });
  });

  it("groups belong to their profile; a foreign or stale context fails closed", async () => {
    const { h, a, b } = setup();
    ok(await h.groups.do(a, { kind: "create", name: "Mine", relationshipIds: [] }));
    expect(h.groups.list(b)).toEqual([]);
    expect((await h.groups.do(b, { kind: "create", name: "x", relationshipIds: [CA] })).ok).toBe(false);
    expect(() => h.groups.list({ ...a, generation: "old" } as typeof a)).toThrow(ProfileAccessError);
  });
});
