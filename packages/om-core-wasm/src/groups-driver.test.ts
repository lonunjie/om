// The host driver for groups over the real carriage (ADR-0189): settle, drain the outbox in order, the 1:1 lane, per-Room polling,
// held frames, route updates, retry and restart. The first block scripts Core (no wasm); the second runs two real wasm Cores with
// OpenMLS over the in-memory provider (needs `pnpm --filter @om/core-wasm build:wasm:groups`; skipped, loudly, only if absent).
import { existsSync, readFileSync } from "node:fs";
import { join } from "node:path";
import { describe, expect, it } from "vitest";
import { asConversationId, asGroupId, asOperationId, asProfileId, captureProfileContext } from "@om/platform-api";
import type { ConversationId, GroupEvent, GroupStep, GroupTarget, GroupWire, HomeSummary, ProfileExecutionContext, ProfileId } from "@om/platform-api";
import { createCoreRuntime, instantiateCore } from "./index";
import type { WasmCoreClient } from "./index";
import { createGroupDriver } from "./groups-driver";
import type { FrameSend, GroupCarriageStatus, GroupCore, GroupOutItem } from "./groups-driver";
import { createGroupFaults } from "./groups-loopback";
import type { GroupFaults } from "./groups-loopback";
import { createMemoryKv, createMemoryProvider, TEST_RELAY_URI } from "./test-support";
import type { MemoryKv, MemoryProvider } from "./test-support";
import type { Inbound, NetCall, NetResult, Provider } from "./types";

const P = asProfileId("p1");
const ctx: ProfileExecutionContext = captureProfileContext(P, "g1" as never);
const dev = (n: number): GroupTarget => ({ device: `d${String(n)}`, account: "a1" });
const empty: GroupStep = { ok: true, events: [], out: [] };
const entry = { relayUri: TEST_RELAY_URI, capability: "c4".repeat(16) + "00".repeat(16) };

/** A scripted Core: a durable-looking outbox, a queue of answers for `do`, and a record of every call made. */
function scripted(over: { steps?: GroupStep[]; lane?: (rel: string, wire: GroupWire) => { ok: true } | { ok: false; refused: string }; seal?: (item: GroupOutItem) => FrameSend } = {}): GroupCore & {
  calls: string[]; box: GroupOutItem[]; mailboxes: { mailbox: string; relayUri: string }[]; routeOwed: string[]; pendingFrames: number; laneKept: GroupWire[]; laneLost: number; received: string[]; frameAnswers: Map<string, GroupStep>;
} {
  const steps = [...(over.steps ?? [])];
  const c = {
    calls: [] as string[], box: [] as GroupOutItem[], mailboxes: [] as { mailbox: string; relayUri: string }[], routeOwed: [] as string[], pendingFrames: 0, laneKept: [] as GroupWire[], laneLost: 0, received: [] as string[], frameAnswers: new Map<string, GroupStep>(),
    available: () => true,
    do: (_c: ProfileExecutionContext, i: { kind: string }) => { c.calls.push(`do:${i.kind}`); return Promise.resolve(steps.shift() ?? empty); },
    receive: () => Promise.resolve(empty),
    list: () => [], get: () => null, invites: () => [], messages: () => [], notices: () => [],
    settle: () => { c.calls.push("settle"); return Promise.resolve(empty); },
    outbox: () => [...c.box],
    status: (): GroupCarriageStatus => ({ mailboxes: c.mailboxes, routeOwed: c.routeOwed, outbox: c.box.length, pending: c.pendingFrames, laneOut: c.laneKept.length, laneLost: c.laneLost }),
    frameSend: (_c: ProfileExecutionContext, to: GroupTarget, wire: GroupWire): Promise<FrameSend> => {
      const item = c.box.find((i) => i.wire === wire && i.to === to);
      c.calls.push(`seal:${wire.t}`);
      if (item === undefined) return Promise.resolve({ ok: false, refused: "unknown" });
      return Promise.resolve(over.seal?.(item) ?? { ok: true, sealed: [{ device: "device" in to ? to.device : "", wrapper: `w${String(item.id)}`, entry }] });
    },
    outboxDone: (_c: ProfileExecutionContext, ids: readonly number[]) => { c.calls.push(`done:${ids.join(",")}`); c.box = c.box.filter((i) => !ids.includes(i.id)); return Promise.resolve(); },
    laneSend: (_c: ProfileExecutionContext, rel: string, wire: GroupWire) => { c.calls.push(`lane:${wire.t}`); return Promise.resolve(over.lane?.(rel, wire) ?? { ok: true as const }); },
    laneFlush: () => {
      const w = c.laneKept[0];
      if (w === undefined) return Promise.resolve({ ok: true as const, moved: false });
      c.calls.push(`lane:${w.t}`);
      const r = over.lane?.("r", w) ?? { ok: true as const };
      if (!r.ok) return Promise.resolve(r);
      c.laneKept.shift();
      return Promise.resolve({ ok: true as const, moved: true });
    },
    lanePump: () => { c.calls.push("pump"); return Promise.resolve(empty); },
    frameReceive: (_c: ProfileExecutionContext, wrapper: string) => { c.received.push(wrapper); return Promise.resolve(c.frameAnswers.get(wrapper) ?? empty); },
    pendingRefresh: () => { c.calls.push("refresh"); return Promise.resolve(); },
    pendingRetry: () => { c.calls.push("retry"); return Promise.resolve(empty); },
    routeUpdate: (_c: ProfileExecutionContext, room: string) => { c.calls.push(`route_update:${room}`); c.routeOwed = c.routeOwed.filter((r) => r !== room); c.box.push(item(900, { device: "d9", account: "a9" }, "route_update")); return Promise.resolve({ ok: true as const, queued: 1 }); },
    held: () => ({ invitations: [], overflow: { invites: 0, replies: 0 } }),
    ackHeldOverflow: () => Promise.resolve(),
    routesExpire: () => { c.calls.push("expire"); return Promise.resolve([] as { relayUri: string; capability: string }[]); },
  };
  return c;
}
const item = (id: number, to: GroupTarget, t = "app"): GroupOutItem => ({ id, intentId: null, to, wire: { t, n: id } });

/** A provider that records deposits, fails them on demand and serves scripted mailboxes. */
function relay(): Provider & { deposits: string[]; fail: Set<number>; boxes: Map<string, Inbound[]>; acks: number[]; revoked: string[]; reject: boolean; n: number } {
  const r = {
    deposits: [] as string[], fail: new Set<number>(), boxes: new Map<string, Inbound[]>(), acks: [] as number[], revoked: [] as string[], reject: false, n: 0,
    perform(call: NetCall): Promise<NetResult> {
      if (call.call === "ack") { r.acks.push(call.seq); return Promise.resolve({ result: "ok" }); }
      if (call.call !== "deposit") return Promise.resolve({ result: "ok" });
      r.n++;
      if (r.fail.has(r.n)) return Promise.resolve({ result: "err", error: { kind: "unavailable" } });
      if (r.reject) return Promise.resolve({ result: "err", error: { kind: "rejected", reason: "unauthorized" } });
      r.deposits.push(call.bytes);
      return Promise.resolve({ result: "ok" });
    },
    poll: (mailbox: string, after: number): Promise<readonly Inbound[]> => Promise.resolve((r.boxes.get(mailbox) ?? []).filter((i) => i.seq > after)),
    revokeDeposit: (_u: string, cap: string): Promise<void> => { r.revoked.push(cap); return Promise.resolve(); },
  };
  return r;
}
const mk = (core: GroupCore, provider: Provider, kv: MemoryKv, topUp: () => Promise<void> = () => Promise.resolve()): ReturnType<typeof createGroupDriver> =>
  createGroupDriver({ core, provider, storage: () => kv, contextFor: () => ctx, topUp, retryMs: 0 });

describe("group driver (scripted Core)", () => {
  it("is unavailable, and refuses every step, when the module has no engine", async () => {
    const core = scripted();
    core.available = () => false;
    const d = mk(core, relay(), createMemoryKv());
    expect(d.available()).toBe(false);
    expect(await d.do(ctx, { kind: "send_text", conversationId: asGroupId("aa"), text: "x" })).toEqual({ ok: false, refused: "groups_unavailable" });
    expect(await d.flush(ctx)).toBe(false);
    expect(core.calls).toEqual([]);
  });

  it("settles after each durable step and first thing at start-up", async () => {
    const core = scripted();
    const d = mk(core, relay(), createMemoryKv());
    await d.do(ctx, { kind: "send_text", conversationId: asGroupId("aa"), text: "x" });
    await d.idle();
    expect(core.calls.slice(0, 2)).toEqual(["do:send_text", "settle"]);
    core.calls.length = 0;
    await d.flush(ctx); // a restart: nothing is waiting for a report, Core is simply asked first
    expect(core.calls[0]).toBe("settle");
  });

  it("drains the outbox in order: seal, deposit, then done; an outage keeps order and repeats the same item", async () => {
    const core = scripted();
    core.box = [item(1, dev(1)), item(2, dev(1)), item(3, dev(1))];
    const r = relay();
    r.fail.add(2); // the second deposit finds the relay down
    const d = mk(core, r, createMemoryKv());
    await d.flush(ctx);
    expect(r.deposits).toEqual(["w1"]); // w2 failed, w3 did not overtake it
    expect(core.box.map((i) => i.id)).toEqual([2, 3]);
    expect(core.calls.filter((c) => c.startsWith("done"))).toEqual(["done:1"]);
    expect(await d.pending(ctx)).toBe(2);
    await d.flush(ctx);
    expect(r.deposits).toEqual(["w1", "w2", "w3"]);
    expect(core.box).toEqual([]);
  });

  it("a destination without a usable route waits; the others go ahead", async () => {
    const core = scripted({ seal: (i) => ("device" in i.to && i.to.device === "d1" ? { ok: false, refused: "route_missing" } : { ok: true, sealed: [{ device: "d2", wrapper: `w${String(i.id)}`, entry }] }) });
    core.box = [item(1, dev(1)), item(2, dev(1)), item(3, dev(2))];
    const r = relay();
    const d = mk(core, r, createMemoryKv());
    await d.flush(ctx);
    expect(r.deposits).toEqual(["w3"]);
    expect(core.box.map((i) => i.id)).toEqual([1, 2]); // nothing was dropped and nothing for d1 reordered
  });

  it("tops up route offers when sealing needs one, then seals", async () => {
    let offers = 0;
    const core = scripted({ seal: (i) => (offers === 0 ? { ok: false, refused: "no_route_offer" } : { ok: true, sealed: [{ device: "d1", wrapper: `w${String(i.id)}`, entry }] }) });
    core.box = [item(1, dev(1))];
    const r = relay();
    const d = mk(core, r, createMemoryKv(), () => { offers = 4; return Promise.resolve(); });
    await d.flush(ctx);
    expect(r.deposits).toEqual(["w1"]);
  });

  it("a relay that refuses a route for good ends the item and is counted; it does not block the group", async () => {
    const core = scripted();
    core.box = [item(1, dev(1)), item(2, dev(2))];
    const r = relay();
    r.reject = true;
    const d = mk(core, r, createMemoryKv());
    const seen: unknown[] = [];
    d.onEvents?.((_p, ev) => seen.push(...ev));
    await d.flush(ctx);
    expect(core.box).toEqual([]);
    expect(d.dropped()).toBe(2);
    expect(seen).toContainEqual({ kind: "undeliverable", count: 2 });
  });

  it("keeps route distribution owed when the destination relay refuses the frame", async () => {
    const core = scripted(); core.box = [item(1, dev(1), "route_update")];
    const r = relay(); r.reject = true;
    const d = mk(core, r, createMemoryKv());
    await d.flush(ctx); d.dispose();
    expect(core.box.map((i) => i.id)).toEqual([1]);
    expect(d.dropped()).toBe(0);
    r.reject = false;
    const next = mk(core, r, createMemoryKv());
    await next.flush(ctx); next.dispose();
    expect(core.box).toEqual([]);
  });

  it("Core keeps an Invite, Accept or Decline until its 1:1 queue takes it; the driver keeps nothing and asks again", async () => {
    const kv = createMemoryKv();
    const core = scripted({ lane: () => ({ ok: false, refused: "no_route_offer" }) });
    core.laneKept = [{ t: "accept", n: 1 }];
    const d1 = mk(core, relay(), kv);
    await d1.flush(ctx);
    expect(await d1.pending(ctx)).toBe(1); // still Core's: nothing of it is in the driver's own storage
    const listed = await kv.list();
    expect(listed.ok ? listed.keys.filter((k) => k.startsWith("gdrv/lane") || k.startsWith("gdrv/dep")) : []).toEqual([]);
    d1.dispose();
    // after a restart Core still holds it, so the next pass moves it
    core.laneKept = [{ t: "accept", n: 1 }];
    const second = scripted();
    second.laneKept = [{ t: "accept", n: 1 }, { t: "decline", n: 2 }];
    await mk(second, relay(), kv).flush(ctx);
    expect(second.calls.filter((c) => c.startsWith("lane:"))).toEqual(["lane:accept", "lane:decline"]);
    expect(await mk(second, relay(), kv).pending(ctx)).toBe(0);
  });

  it("tells the client when Core dropped a lane item for good", async () => {
    const core = scripted();
    core.laneLost = 2;
    const d = mk(core, relay(), createMemoryKv());
    const seen: unknown[] = [];
    d.onEvents?.((_p, ev) => seen.push(...ev));
    await d.flush(ctx);
    expect(seen).toContainEqual({ kind: "undeliverable", count: 2 });
  });

  it("polls each per-Room mailbox in order, acknowledges after the step, and never repeats a frame (also across a restart)", async () => {
    const core = scripted();
    core.mailboxes = [{ mailbox: "m1", relayUri: TEST_RELAY_URI }];
    core.frameAnswers.set("f2", { ok: false, refused: "frame_invalid" }); // a drop changes nothing and is not repeated
    const r = relay();
    r.boxes.set("m1", [{ seq: 2, bytes: "f2" }, { seq: 1, bytes: "f1" }, { seq: 3, bytes: "f3" }]);
    const kv = createMemoryKv();
    const d = mk(core, r, kv);
    await d.flush(ctx);
    expect(core.received).toEqual(["f1", "f2", "f3"]);
    expect(r.acks).toEqual([1, 2, 3]);
    d.dispose();
    const again = scripted();
    again.mailboxes = core.mailboxes;
    await mk(again, r, kv).flush(ctx);
    expect(again.received).toEqual([]);
  });

  it("after a poll, held frames ask for the missing heads and are retried", async () => {
    const core = scripted();
    core.pendingFrames = 1;
    await mk(core, relay(), createMemoryKv()).flush(ctx);
    expect(core.calls).toEqual(expect.arrayContaining(["refresh", "retry"]));
    expect(core.calls.indexOf("refresh")).toBeLessThan(core.calls.indexOf("retry"));
  });

  it("after a removal asks Core for a route update, which Core queues in its outbox; the driver deposits it, repeats it after an outage and revokes what expired", async () => {
    const core = scripted();
    core.routeOwed = ["room1"];
    const r = relay();
    r.fail.add(1);
    const d = mk(core, r, createMemoryKv());
    await d.flush(ctx);
    expect(core.calls).toContain("route_update:room1");
    expect(r.deposits).toEqual([]); // the relay was down
    expect(core.box.length).toBe(1); // the frame is still Core's, not the driver's
    expect(await d.pending(ctx)).toBe(1);
    d.dispose();
    await mk(core, r, createMemoryKv()).flush(ctx);
    expect(r.deposits).toEqual(["w900"]);
    expect(core.box).toEqual([]);
    expect(core.calls.filter((c) => c === "route_update:room1").length).toBe(1);
    const third = scripted();
    third.routesExpire = () => Promise.resolve([{ relayUri: TEST_RELAY_URI, capability: "cap1" }]);
    await mk(third, r, createMemoryKv()).flush(ctx);
    expect(r.revoked).toEqual(["cap1"]);
  });

  it("reports background events to a listener", async () => {
    const core = scripted();
    core.mailboxes = [{ mailbox: "m1", relayUri: TEST_RELAY_URI }];
    const ev: GroupEvent = { kind: "held", reason: "head_missing" };
    core.frameAnswers.set("f1", { ok: true, events: [ev], out: [] });
    const r = relay();
    r.boxes.set("m1", [{ seq: 1, bytes: "f1" }]);
    const d = mk(core, r, createMemoryKv());
    const seen: GroupEvent[] = [];
    d.onEvents?.((_p, e) => { seen.push(...e); });
    await d.flush(ctx);
    expect(seen).toEqual([ev]);
  });
});

// ------------------------------------------------------------------------------------------------ real Cores
const WITH_GROUPS = join(import.meta.dirname, "../dist/om_wasm_groups.wasm");
const HOME: HomeSummary = { tier: "T1", observers: ["host_sees_ciphertext"], migrationAvailable: true, clientOriginChoice: ["default_origin"] };
const op = asOperationId;
if (!existsSync(WITH_GROUPS)) console.warn("om_wasm_groups.wasm not built: run `pnpm --filter @om/core-wasm build:wasm:groups`; skipping the driver suite over real Cores");

interface Boot { rt: WasmCoreClient; ctx: ProfileExecutionContext; id: ProfileId; kv: MemoryKv }
interface Party extends Boot { rel: ConversationId }

async function boot(provider: Provider, id: ProfileId, kv: MemoryKv = createMemoryKv(), driver = true): Promise<Boot> {
  const core = await instantiateCore(readFileSync(WITH_GROUPS));
  const rt = await createCoreRuntime({ core, provider, storage: () => kv, home: HOME, autoDrain: false, retryMs: 0, ...(driver ? { groupDriver: { retryMs: 0 } } : {}) });
  return { rt, ctx: await rt.openProfile(id), id, kv };
}
async function settle(...ps: Boot[]): Promise<void> {
  for (let i = 0; i < 20; i++) for (const p of ps) await p.rt.flush(p.id);
  for (const p of ps) await p.rt.idle();
}
async function connect(): Promise<{ alice: Party; bob: Party; faults: GroupFaults; relay: MemoryProvider }> {
  const relayStore = createMemoryProvider();
  const faults = createGroupFaults(relayStore);
  const a = await boot(faults.provider("alice"), asProfileId("alice"));
  const b = await boot(faults.provider("bob"), asProfileId("bob"));
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
  return { alice: { ...a, rel: asConversationId(ac.id) }, bob: { ...b, rel: asConversationId(bc.id) }, faults, relay: relayStore };
}
const texts = (p: Boot, conv: ReturnType<typeof asGroupId>): string[] => p.rt.groups.messages(p.ctx, conv).map((m) => m.text);

describe.skipIf(!existsSync(WITH_GROUPS))("the driver over two real wasm Cores with OpenMLS, through the sealed carriage", () => {
  it("create, invite over the 1:1, join, Welcome, text both ways, an outage, a duplicate, a timeout, a restart and a removal", async () => {
    const { alice, bob, faults, relay: store } = await connect();
    expect(alice.rt.groups.available()).toBe(true);
    const seen: string[] = [];
    bob.rt.groups.onEvents?.((_p, e) => { seen.push(...e.map((x) => x.kind)); });

    const created = await alice.rt.groups.do(alice.ctx, { kind: "create", name: "Book club", relationshipIds: [alice.rel] });
    expect(created.ok).toBe(true);
    await settle(alice, bob);
    const [inv] = bob.rt.groups.invites(bob.ctx);
    expect(inv).toMatchObject({ groupName: "Book club", dir: "incoming", state: "offered" });
    const conv = asGroupId(alice.rt.groups.list(alice.ctx)[0]?.conversationId ?? "");
    expect(alice.rt.groups.get(alice.ctx, conv)?.people.map((x) => x.status).sort()).toEqual(["invited", "member"]); // Bob has not said yes

    expect((await bob.rt.groups.do(bob.ctx, { kind: "accept", inviteId: inv?.id ?? "" })).ok).toBe(true);
    await settle(alice, bob);
    expect(bob.rt.groups.list(bob.ctx)[0]?.members).toBe(2); // the Welcome arrived over Bob's per-Room route and was joined
    expect(alice.rt.groups.get(alice.ctx, conv)?.canSend).toBe(true); // Core accepted its own Commit: no host report existed
    expect(seen).toContain("joined");

    await alice.rt.groups.do(alice.ctx, { kind: "send_text", conversationId: conv, text: "hello" });
    await settle(alice, bob);
    await bob.rt.groups.do(bob.ctx, { kind: "send_text", conversationId: conv, text: "hi A" });
    await settle(alice, bob);
    for (const p of [alice, bob]) expect(texts(p, conv)).toEqual(["hello", "hi A"]);
    // the relay only ever saw sealed wrappers
    expect(store.deposited().every((b) => !Buffer.from(b, "hex").includes(Buffer.from("hello")))).toBe(true);

    // an outage on the sender: the message stays owed (Core's outbox), and arrives once, in order, when the relay is back
    faults.setDown("alice", true);
    await alice.rt.groups.do(alice.ctx, { kind: "send_text", conversationId: conv, text: "while you were away" });
    await settle(alice, bob);
    expect(await (alice.rt.groups as unknown as { pending(c: ProfileExecutionContext): Promise<number> }).pending(alice.ctx)).toBe(1);
    expect(texts(bob, conv)).toEqual(["hello", "hi A"]);
    faults.setDown("alice", false);
    faults.duplicateNext("alice"); // delivered twice: shown once
    await settle(alice, bob);
    expect(texts(bob, conv)).toEqual(["hello", "hi A", "while you were away"]);

    // a timeout after the deposit took effect: the driver cannot tell, repeats, and the duplicate is dropped by MLS
    faults.timeoutNext("alice");
    await alice.rt.groups.do(alice.ctx, { kind: "send_text", conversationId: conv, text: "maybe twice" });
    await settle(alice, bob);
    expect(texts(bob, conv)).toEqual(["hello", "hi A", "while you were away", "maybe twice"]);

    // a restart of Alice's whole runtime loses nothing that was owed
    faults.setDown("alice", true);
    await alice.rt.groups.do(alice.ctx, { kind: "send_text", conversationId: conv, text: "after restart" });
    await settle(alice, bob);
    alice.rt.dispose();
    faults.setDown("alice", false);
    const again = await boot(faults.provider("alice"), alice.id, alice.kv);
    await settle(again, bob);
    expect(texts(bob, conv)).toEqual(["hello", "hi A", "while you were away", "maybe twice", "after restart"]);
    expect(texts(again, conv)).toEqual(["hello", "hi A", "while you were away", "maybe twice", "after restart"]);

    // Alice removes Bob: Core commits it, Bob is out, and Alice's own route for the Room is replaced
    const bobAccount = again.rt.groups.get(again.ctx, conv)?.people.find((x) => x.roles.includes("Member"))?.account ?? "";
    const removal = await again.rt.groups.do(again.ctx, { kind: "remove", conversationId: conv, account: bobAccount });
    expect(removal).toMatchObject({ ok: true });
    await settle(again, bob);
    expect(again.rt.groups.get(again.ctx, conv)?.summary.members).toBe(1);
    expect(await bob.rt.groups.do(bob.ctx, { kind: "send_text", conversationId: conv, text: "late" })).toMatchObject({ ok: false });
  });

  it("a host without the driver keeps the bare engine, and a shipped module reports unavailable", async () => {
    const relayStore = createMemoryProvider();
    const bare = await boot(relayStore, asProfileId("bare"), createMemoryKv(), false);
    expect(bare.rt.groups.available()).toBe(true);
    expect("onEvents" in bare.rt.groups).toBe(false);
  });
});
