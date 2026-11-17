// Ordinary groups (ADR-0177) through the real Core: the typed `groups` API of the binding over two wasm Cores. Needs the
// module built WITH the OpenMLS engine (`pnpm --filter @om/core-wasm build:wasm:groups`, which depends on the proposed
// ADR-0187); the shipped module is built without it and must refuse every step. Skipped, loudly, only if an artifact is absent.
// The group wire is Core-internal JSON (plan Q2): the test only carries it between the two Cores, as a host driver would.
import { existsSync, readFileSync } from "node:fs";
import { join } from "node:path";
import { describe, expect, it } from "vitest";
import { asConversationId, asGroupId, asOperationId, asProfileId } from "@om/platform-api";
import type { ConversationId, GroupEvent, GroupId, GroupSender, GroupStep, HomeSummary, ProfileExecutionContext, ProfileId } from "@om/platform-api";
import { createCoreRuntime, instantiateCore } from "./index";
import type { WasmCoreClient } from "./index";
import { createMemoryKv, createMemoryProvider, type MemoryProvider } from "./test-support";

const SHIPPED = join(import.meta.dirname, "../dist/om_wasm.wasm");
const WITH_GROUPS = join(import.meta.dirname, "../dist/om_wasm_groups.wasm");
const HOME: HomeSummary = { tier: "T1", observers: ["host_sees_ciphertext"], migrationAvailable: true, clientOriginChoice: ["default_origin"] };
const op = asOperationId;
if (!existsSync(WITH_GROUPS)) console.warn("om_wasm_groups.wasm not built: run `pnpm --filter @om/core-wasm build:wasm:groups`; skipping the real group suite");

interface Party { rt: WasmCoreClient; ctx: ProfileExecutionContext; id: ProfileId; rel: ConversationId }

async function boot(file: string, provider: MemoryProvider, id: ProfileId): Promise<Omit<Party, "rel">> {
  const core = await instantiateCore(readFileSync(file));
  const rt = await createCoreRuntime({ core, provider, storage: () => createMemoryKv(), home: HOME, autoDrain: false, retryMs: 0 });
  return { rt, ctx: await rt.openProfile(id), id };
}
async function settle(...ps: Omit<Party, "rel">[]): Promise<void> {
  for (let i = 0; i < 20; i++) for (const p of ps) await p.rt.flush(p.id);
  for (const p of ps) await p.rt.idle();
}

/** Two people with an active 1:1 relationship (the first-contact flow), on the given module. */
async function connect(file: string, fileB: string = file): Promise<[Party, Party]> {
  const provider = createMemoryProvider();
  const a = await boot(file, provider, asProfileId("alice"));
  const b = await boot(fileB, provider, asProfileId("bob"));
  await a.rt.dispatch({ kind: "CreateIdentityProfile", ctx: a.ctx, operationId: op("a-create"), label: "Alice" });
  await b.rt.dispatch({ kind: "CreateIdentityProfile", ctx: b.ctx, operationId: op("b-create"), label: "Bob" });
  await settle(a, b);
  await a.rt.dispatch({ kind: "CreateInvite", ctx: a.ctx, operationId: op("a-invite") });
  await settle(a, b);
  const invite = await a.rt.query({ kind: "GetInvite", ctx: a.ctx, operationId: op("a-invite") });
  if (!invite) throw new Error("no invite");
  await b.rt.dispatch({ kind: "AcceptInvitation", ctx: b.ctx, operationId: op("b-accept"), link: invite.link });
  await settle(a, b);
  const [intro] = await a.rt.query({ kind: "GetIntroductions", ctx: a.ctx });
  if (!intro) throw new Error("no introduction");
  await a.rt.dispatch({ kind: "AcceptIntroduction", ctx: a.ctx, operationId: op("a-intro"), requestId: intro.id });
  await settle(a, b);
  const [ac] = await a.rt.query({ kind: "GetConversationList", ctx: a.ctx });
  const [bc] = await b.rt.query({ kind: "GetConversationList", ctx: b.ctx });
  if (!ac || !bc) throw new Error("no conversation");
  return [{ ...a, rel: asConversationId(ac.id) }, { ...b, rel: asConversationId(bc.id) }];
}

const must = (s: GroupStep): Extract<GroupStep, { ok: true }> => {
  if (!s.ok) throw new Error(`refused: ${s.refused}`);
  return s;
};
const has = (ev: readonly { who: number; e: GroupEvent }[], who: number, kind: GroupEvent["kind"]): boolean => ev.some((x) => x.who === who && x.e.kind === kind);

/** Carry a step's outbounds to the other party (and what they answer), like a host driver; `devs` are [alice device, bob device]. */
async function pump(p: [Party, Party], devs: readonly [string, string], from: number, step: Extract<GroupStep, { ok: true }>): Promise<{ who: number; e: GroupEvent }[]> {
  const ev = step.events.map((e) => ({ who: from, e }));
  const to = 1 - from;
  const target = p[to];
  if (!target) throw new Error("party index");
  for (const o of step.out) {
    const sender: GroupSender = "relationship" in o.to ? { relationship: target.rel } : { device: devs[from] ?? "" };
    ev.push(...(await pump(p, devs, to, must(await target.rt.groups.receive(target.ctx, sender, o.wire)))));
  }
  return ev;
}

describe.skipIf(!existsSync(SHIPPED))("the shipped module has no group engine", () => {
  it("reports groups unavailable and refuses every step, changing nothing", async () => {
    const [alice] = await connect(SHIPPED);
    expect(alice.rt.groups.available()).toBe(false);
    const r = await alice.rt.groups.do(alice.ctx, { kind: "create", name: "x", relationshipIds: [alice.rel] });
    expect(r).toEqual({ ok: false, refused: "groups_unavailable" });
    expect(alice.rt.groups.list(alice.ctx)).toEqual([]);
  });
});

describe.skipIf(!existsSync(WITH_GROUPS))("groups through two real wasm Cores with OpenMLS", () => {
  it("create, invite, accept, add, text both ways, leave", async () => {
    const [alice, bob] = await connect(WITH_GROUPS);
    const p: [Party, Party] = [alice, bob];
    expect(alice.rt.groups.available()).toBe(true);
    const none: readonly [string, string] = ["", ""];
    const created = must(await alice.rt.groups.do(alice.ctx, { kind: "create", name: "Book club", relationshipIds: [alice.rel] }));
    expect(created.out[0]?.wire.t).toBe("invite");
    const ev = await pump(p, none, 0, created);
    expect(has(ev, 1, "invite_received")).toBe(true);
    const conv: GroupId = asGroupId(alice.rt.groups.list(alice.ctx)[0]?.conversationId ?? "");
    const [inv] = bob.rt.groups.invites(bob.ctx);
    expect(inv).toMatchObject({ groupName: "Book club", dir: "incoming", state: "offered" });
    expect(inv && "releases" in inv).toBe(false);
    // the invitee is shown as invited, not as a member, and nobody can send yet
    expect(alice.rt.groups.get(alice.ctx, conv)?.people.map((x) => x.status).sort()).toEqual(["invited", "member"]);
    const acc = must(await bob.rt.groups.do(bob.ctx, { kind: "accept", inviteId: inv?.id ?? "" }));
    const bobDev = String((acc.out[0]?.wire["releases"] as { device: string }[])[0]?.device);
    const ev2 = await pump(p, none, 1, acc);
    const prepared = ev2.find((x) => x.who === 0 && x.e.kind === "commit_prepared")?.e;
    if (prepared?.kind !== "commit_prepared") throw new Error("no commit prepared");
    expect(alice.rt.groups.get(alice.ctx, conv)?.canSend).toBe(false);
    expect(prepared.intentId).not.toBe("");
    const welcome = must(await alice.rt.groups.settle(alice.ctx)); // Core accepts its own Commit (ADR-0189 section 2): there is no host report
    const dir = welcome.out[0]?.wire["directory"] as [string, string, string][];
    const aliceDev = dir.map((d) => d[0]).find((d) => d !== bobDev) ?? "";
    const devs: readonly [string, string] = [aliceDev, bobDev];
    expect(has(await pump(p, devs, 0, welcome), 1, "joined")).toBe(true);
    // text both ways: a wire per other device, never over the relationship
    const sent = must(await alice.rt.groups.do(alice.ctx, { kind: "send_text", conversationId: conv, text: "hello" }));
    expect(sent.out.every((o) => "device" in o.to)).toBe(true);
    expect(has(await pump(p, devs, 0, sent), 1, "message_received")).toBe(true);
    await pump(p, devs, 1, must(await bob.rt.groups.do(bob.ctx, { kind: "send_text", conversationId: conv, text: "hi A" })));
    for (const party of p) {
      expect(party.rt.groups.messages(party.ctx, conv).map((m) => m.text)).toEqual(["hello", "hi A"]);
      expect(party.rt.groups.get(party.ctx, conv)?.summary.members).toBe(2);
    }
    // reply, reaction, edit and delete cross the real engine as typed events (D143)
    const first = alice.rt.groups.messages(alice.ctx, conv)[0]?.id ?? "";
    await pump(p, devs, 1, must(await bob.rt.groups.do(bob.ctx, { kind: "send_reply", conversationId: conv, text: "yes", replyTo: first })));
    expect(has(await pump(p, devs, 1, must(await bob.rt.groups.do(bob.ctx, { kind: "set_reaction", conversationId: conv, messageId: first, key: "\u{1F44D}", present: true }))), 0, "message_changed")).toBe(true);
    expect(await bob.rt.groups.do(bob.ctx, { kind: "edit_message", conversationId: conv, messageId: first, text: "mine now" })).toEqual({ ok: false, refused: "not_author" });
    await pump(p, devs, 0, must(await alice.rt.groups.do(alice.ctx, { kind: "edit_message", conversationId: conv, messageId: first, text: "hello, all" })));
    for (const party of p) {
      const [m0, , m2] = party.rt.groups.messages(party.ctx, conv);
      expect(m0).toMatchObject({ id: first, text: "hello, all", edited: true, reactions: [{ key: "\u{1F44D}", count: 1, mine: party === bob }] });
      expect(m2).toMatchObject({ text: "yes", replyTo: first });
    }
    await pump(p, devs, 0, must(await alice.rt.groups.do(alice.ctx, { kind: "delete_message", conversationId: conv, messageId: first })));
    for (const party of p) expect(party.rt.groups.messages(party.ctx, conv).map((m) => m.text)).toEqual(["hi A", "yes"]);
    // typed refusals change nothing
    expect(await bob.rt.groups.do(bob.ctx, { kind: "decline", inviteId: "nope" })).toMatchObject({ ok: false });
    expect(bob.rt.groups.get(bob.ctx, asGroupId("00"))).toBeNull();
    // Bob leaves: his device stops sending; Alice's Core prepares the removal
    const leave = must(await bob.rt.groups.do(bob.ctx, { kind: "leave", conversationId: conv }));
    expect(has(await pump(p, devs, 1, leave), 0, "commit_prepared")).toBe(true);
    expect(await bob.rt.groups.do(bob.ctx, { kind: "send_text", conversationId: conv, text: "late" })).toMatchObject({ ok: false });
  });
});

describe.skipIf(!existsSync(SHIPPED) || !existsSync(WITH_GROUPS))("an invitation reaches a device that has not loaded the groups module (D70)", () => {
  it("is listed by the base module without the engine, with no secret, and is never accepted", async () => {
    const [alice, bob] = await connect(WITH_GROUPS, SHIPPED);
    expect(bob.rt.groups.available()).toBe(false);
    const created = must(await alice.rt.groups.do(alice.ctx, { kind: "create", name: "Book club", relationshipIds: [alice.rel] }));
    const invite = created.out[0];
    if (invite === undefined || !("relationship" in invite.to)) throw new Error("no invite on the lane");
    const lane = alice.rt.groups as unknown as { laneSend: (ctx: ProfileExecutionContext, rel: string, wire: unknown) => Promise<{ ok: boolean }> };
    expect(await lane.laneSend(alice.ctx, alice.rel, invite.wire)).toEqual({ ok: true });
    await settle(alice, bob);
    const held = bob.rt.groups.held?.(bob.ctx);
    expect(held?.invitations).toHaveLength(1);
    expect(held?.invitations[0]).toMatchObject({ relationshipId: bob.rel, groupName: "Book club" });
    expect(held?.invitations[0]?.receivedMs).toBeGreaterThan(0);
    expect(Object.keys(held?.invitations[0] ?? {}).sort()).toEqual(["groupName", "inviteId", "receivedMs", "relationshipId"]);
    expect(held?.overflow).toEqual({ invites: 0, replies: 0 });
    // Nothing joined, nothing listed as a group invitation, and the engine-less module still refuses every step.
    expect(bob.rt.groups.invites(bob.ctx)).toEqual([]);
    expect(await bob.rt.groups.do(bob.ctx, { kind: "accept", inviteId: held?.invitations[0]?.inviteId ?? "" })).toEqual({ ok: false, refused: "groups_unavailable" });
    // Acknowledging the (empty) overflow is durable and leaves the held invitation alone.
    await bob.rt.groups.ackHeldOverflow?.(bob.ctx);
    expect(bob.rt.groups.held?.(bob.ctx).invitations).toHaveLength(1);
  });
});
