// The host driver for groups: carrying `out`, receiving, the Commit reports, retry and restart. The first block scripts the
// group steps (no wasm); the second runs two real wasm Cores with OpenMLS through the driver over an in-process loopback carriage
// (needs `pnpm --filter @om/core-wasm build:wasm:groups`; skipped, loudly, only if the artifact is absent).
import { existsSync, readFileSync } from "node:fs";
import { join } from "node:path";
import { describe, expect, it } from "vitest";
import { asConversationId, asGroupId, asOperationId, asProfileId } from "@om/platform-api";
import type { ConversationId, GroupApi, GroupIntent, GroupOutbound, GroupStep, HomeSummary, ProfileExecutionContext, ProfileId } from "@om/platform-api";
import { captureProfileContext } from "@om/platform-api";
import { createCoreRuntime, instantiateCore } from "./index";
import type { CoreRuntimeOptions, WasmCoreClient } from "./index";
import { acceptAfterDelivery, createGroupDriver } from "./groups-driver";
import type { CarriageResult, GroupCarriage } from "./groups-driver";
import { createGroupLoopback } from "./groups-loopback";
import { createMemoryKv, createMemoryProvider } from "./test-support";
import type { MemoryKv } from "./test-support";

const P = asProfileId("p1");
const ctxOf = (id: ProfileId = P): ProfileExecutionContext => captureProfileContext(id, "g1" as never);
const wire = (t: string): GroupOutbound => ({ to: { relationship: asConversationId("r1") }, wire: { t } });

/** A scripted engine: each `do` answers from a queue and records the intents it saw. */
function scripted(steps: GroupStep[]): GroupApi & { seen: GroupIntent[] } {
  const seen: GroupIntent[] = [];
  return {
    seen,
    available: () => true,
    do: (_c, i) => { seen.push(i); return Promise.resolve(steps.shift() ?? { ok: true, events: [], out: [] }); },
    receive: () => Promise.resolve(steps.shift() ?? { ok: true, events: [], out: [] }),
    list: () => [], get: () => null, invites: () => [], messages: () => [],
  };
}
function carriage(script: CarriageResult[] = []): GroupCarriage & { sent: GroupOutbound[] } {
  const sent: GroupOutbound[] = [];
  return { sent, available: () => true, send: (_p, o) => { const r = script.shift() ?? "sent"; if (r === "sent") sent.push(o); return Promise.resolve(r); }, onReceive: () => () => undefined };
}

describe("group driver (scripted engine)", () => {
  it("is unavailable, and refuses every step, with no carriage", async () => {
    const engine = scripted([]);
    const d = createGroupDriver({ engine, storage: () => createMemoryKv(), contextFor: ctxOf, carriage: null });
    expect(d.available()).toBe(false);
    expect(await d.do(ctxOf(), { kind: "send_text", conversationId: asGroupId("aa"), text: "x" })).toEqual({ ok: false, refused: "groups_unavailable" });
    expect(engine.seen).toEqual([]);
  });

  it("writes outbounds durably before the step returns, keeps order across an outage and retries the same bytes", async () => {
    const kv = createMemoryKv();
    const c = carriage(["sent", "unavailable"]);
    const engine = scripted([{ ok: true, events: [], out: [wire("a"), wire("b"), wire("c")] }]);
    const d = createGroupDriver({ engine, storage: () => kv, contextFor: ctxOf, carriage: c, retryMs: 0 });
    const step = await d.do(ctxOf(), { kind: "send_text", conversationId: asGroupId("aa"), text: "x" });
    expect(step.ok).toBe(true);
    await d.idle();
    expect(c.sent.map((o) => o.wire.t)).toEqual(["a"]); // b was refused: c did not overtake it
    expect(await d.pending(ctxOf())).toBe(2);
    await d.flush(ctxOf());
    expect(c.sent.map((o) => o.wire.t)).toEqual(["a", "b", "c"]);
    expect(await d.pending(ctxOf())).toBe(0);
  });

  it("survives a restart: a new driver on the same store sends what was owed", async () => {
    const kv = createMemoryKv();
    const down = carriage(["unavailable"]);
    const first = createGroupDriver({ engine: scripted([{ ok: true, events: [], out: [wire("x")] }]), storage: () => kv, contextFor: ctxOf, carriage: down, retryMs: 0 });
    await first.do(ctxOf(), { kind: "send_text", conversationId: asGroupId("aa"), text: "x" });
    await first.idle();
    first.dispose();
    const up = carriage();
    const second = createGroupDriver({ engine: scripted([]), storage: () => kv, contextFor: ctxOf, carriage: up, retryMs: 0 });
    await second.flush(ctxOf());
    expect(up.sent.map((o) => o.wire.t)).toEqual(["x"]);
  });

  it("drops what the transport says can never be delivered", async () => {
    const c = carriage(["undeliverable"]);
    const kv = createMemoryKv();
    const d = createGroupDriver({ engine: scripted([{ ok: true, events: [], out: [wire("a")] }]), storage: () => kv, contextFor: ctxOf, carriage: c, retryMs: 0 });
    await d.do(ctxOf(), { kind: "send_text", conversationId: asGroupId("aa"), text: "x" });
    await d.idle();
    expect(await d.pending(ctxOf())).toBe(0);
  });

  it("reports a Commit published only after its wires are out, and never accepted without a policy", async () => {
    const engine = scripted([{ ok: true, events: [{ kind: "commit_prepared", intentId: "i1" }], out: [wire("commit")] }]);
    const c = carriage(["unavailable"]);
    const kv = createMemoryKv();
    const d = createGroupDriver({ engine, storage: () => kv, contextFor: ctxOf, carriage: c, retryMs: 0 });
    await d.do(ctxOf(), { kind: "accept", inviteId: "x" });
    await d.idle();
    expect(engine.seen.map((i) => i.kind)).toEqual(["accept"]); // the Commit is not out yet: nothing reported
    await d.flush(ctxOf());
    expect(engine.seen.map((i) => i.kind)).toEqual(["accept", "commit_published"]); // out, so published; no policy, so not accepted
    await d.flush(ctxOf());
    expect(engine.seen.map((i) => i.kind)).toEqual(["accept", "commit_published"]);
  });

  it("with a policy, reports accepted and sends the Welcome it releases", async () => {
    const engine = scripted([
      { ok: true, events: [{ kind: "commit_prepared", intentId: "i1" }], out: [wire("commit")] },
      { ok: true, events: [], out: [] }, // commit_published
      { ok: true, events: [], out: [wire("welcome")] }, // commit_accepted
    ]);
    const c = carriage();
    const kv = createMemoryKv();
    const d = createGroupDriver({ engine, storage: () => kv, contextFor: ctxOf, carriage: c, acceptance: acceptAfterDelivery, retryMs: 0 });
    await d.do(ctxOf(), { kind: "accept", inviteId: "x" });
    await d.idle();
    expect(engine.seen.map((i) => i.kind)).toEqual(["accept", "commit_published", "commit_accepted"]);
    expect(c.sent.map((o) => o.wire.t)).toEqual(["commit", "welcome"]);
  });

  it("resumes a half-reported Commit after a restart", async () => {
    const kv = createMemoryKv();
    const first = createGroupDriver({ engine: scripted([{ ok: true, events: [{ kind: "commit_prepared", intentId: "i9" }], out: [wire("commit")] }]), storage: () => kv, contextFor: ctxOf, carriage: carriage(["unavailable"]), retryMs: 0 });
    await first.do(ctxOf(), { kind: "accept", inviteId: "x" });
    await first.idle();
    first.dispose();
    const engine = scripted([]);
    const second = createGroupDriver({ engine, storage: () => kv, contextFor: ctxOf, carriage: carriage(), retryMs: 0 });
    await second.flush(ctxOf());
    expect(engine.seen).toEqual([{ kind: "commit_published", intentId: "i9" }]);
  });
});

// ------------------------------------------------------------------------------------------------ real Cores
const WITH_GROUPS = join(import.meta.dirname, "../dist/om_wasm_groups.wasm");
const HOME: HomeSummary = { tier: "T1", observers: ["host_sees_ciphertext"], migrationAvailable: true, clientOriginChoice: ["default_origin"] };
const op = asOperationId;
if (!existsSync(WITH_GROUPS)) console.warn("om_wasm_groups.wasm not built: run `pnpm --filter @om/core-wasm build:wasm:groups`; skipping the driver suite over real Cores");

interface Party { rt: WasmCoreClient; ctx: ProfileExecutionContext; id: ProfileId; rel: ConversationId; kv: MemoryKv; device: string }

async function boot(provider: ReturnType<typeof createMemoryProvider>, id: ProfileId, driver?: CoreRuntimeOptions["groupDriver"], kv: MemoryKv = createMemoryKv()): Promise<Omit<Party, "rel" | "device">> {
  const core = await instantiateCore(readFileSync(WITH_GROUPS));
  const rt = await createCoreRuntime({ core, provider, storage: () => kv, home: HOME, autoDrain: false, retryMs: 0, ...(driver === undefined ? {} : { groupDriver: driver }) });
  return { rt, ctx: await rt.openProfile(id), id, kv };
}
async function settle(...ps: Omit<Party, "rel" | "device">[]): Promise<void> {
  for (let i = 0; i < 20; i++) for (const p of ps) await p.rt.flush(p.id);
  for (const p of ps) await p.rt.idle();
}
async function connect(net?: ReturnType<typeof createGroupLoopback>): Promise<[Party, Party]> {
  const provider = createMemoryProvider();
  const d = (id: ProfileId): CoreRuntimeOptions["groupDriver"] | undefined => (net === undefined ? undefined : { carriage: net.carriage(id), acceptance: acceptAfterDelivery });
  const a = await boot(provider, asProfileId("alice"), d(asProfileId("alice")));
  const b = await boot(provider, asProfileId("bob"), d(asProfileId("bob")));
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
  const dev = async (p: Omit<Party, "rel" | "device">): Promise<string> => {
    const v = await p.rt.query({ kind: "GetDevices", ctx: p.ctx });
    const me = v.devices.find((d) => d.isCurrent);
    if (!me) throw new Error("no current device");
    return String(me.id);
  };
  return [{ ...a, rel: asConversationId(ac.id), device: await dev(a) }, { ...b, rel: asConversationId(bc.id), device: await dev(b) }];
}

describe.skipIf(!existsSync(WITH_GROUPS))("the driver over two real wasm Cores with OpenMLS", () => {
  it("create, invite, accept, add, text both ways, an outage, a duplicate and a restart, all through the driver", async () => {
    const [alice, bob] = await connect();
    const net = createGroupLoopback();
    net.connect({ profileId: alice.id, relationship: alice.rel }, { profileId: bob.id, relationship: bob.rel });
    net.device(alice.id, alice.device);
    net.device(bob.id, bob.device);
    const mk = (p: Party, car = net.carriage(p.id)): ReturnType<typeof createGroupDriver> =>
      createGroupDriver({ engine: p.rt.groups, storage: () => p.kv, contextFor: (id) => p.rt.contextFor(id), carriage: car, acceptance: acceptAfterDelivery, retryMs: 0 });
    const da = mk(alice);
    const db = mk(bob);
    const quiet = async (): Promise<void> => { for (let i = 0; i < 10; i++) { await da.idle(); await db.idle(); } };

    expect(da.available()).toBe(true);
    const created = await da.do(alice.ctx, { kind: "create", name: "Book club", relationshipIds: [alice.rel] });
    expect(created.ok).toBe(true);
    await quiet();
    const [inv] = db.invites(bob.ctx);
    expect(inv).toMatchObject({ groupName: "Book club", dir: "incoming", state: "offered" });
    const conv = asGroupId(da.list(alice.ctx)[0]?.conversationId ?? "");
    expect(da.get(alice.ctx, conv)?.people.map((x) => x.status).sort()).toEqual(["invited", "member"]); // not yet added: Bob has not said yes

    expect((await db.do(bob.ctx, { kind: "accept", inviteId: inv?.id ?? "" })).ok).toBe(true);
    await quiet();
    expect(db.list(bob.ctx)[0]?.members).toBe(2); // Welcome arrived and was joined
    expect(da.get(alice.ctx, conv)?.canSend).toBe(true);

    await da.do(alice.ctx, { kind: "send_text", conversationId: conv, text: "hello" });
    await quiet();
    await db.do(bob.ctx, { kind: "send_text", conversationId: conv, text: "hi A" });
    await quiet();
    for (const [d, p] of [[da, alice], [db, bob]] as const) expect(d.messages(p.ctx, conv).map((m) => m.text)).toEqual(["hello", "hi A"]);
    // an application message never travels over a relationship (no pairwise group mode): only invitations and the Welcome do
    expect(net.log.filter((l) => l.via === "relationship").map((l) => l.wire.t).sort()).toEqual(["accept", "invite", "welcome"]);
    expect(net.log.filter((l) => l.wire.t === "app").every((l) => l.via === "device")).toBe(true);

    // an outage: the message stays owed, durable, and arrives once, in order, when the transport is back
    net.setDown(bob.id, true);
    await da.do(alice.ctx, { kind: "send_text", conversationId: conv, text: "while you were away" });
    await quiet();
    expect(await da.pending(alice.ctx)).toBe(1);
    expect(db.messages(bob.ctx, conv).map((m) => m.text)).toEqual(["hello", "hi A"]);
    net.setDown(bob.id, false);
    net.duplicateNext(bob.id); // and it is delivered twice: shown once
    await da.flush(alice.ctx);
    await quiet();
    expect(await da.pending(alice.ctx)).toBe(0);
    expect(db.messages(bob.ctx, conv).map((m) => m.text)).toEqual(["hello", "hi A", "while you were away"]);

    // a restart of the sender's driver loses nothing that was owed
    net.setDown(bob.id, true);
    await da.do(alice.ctx, { kind: "send_text", conversationId: conv, text: "after restart" });
    await quiet();
    da.dispose();
    const da2 = mk(alice, net.carriage(alice.id));
    net.setDown(bob.id, false);
    await da2.flush(alice.ctx);
    await da2.idle();
    await db.idle();
    expect(db.messages(bob.ctx, conv).map((m) => m.text)).toEqual(["hello", "hi A", "while you were away", "after restart"]);

    // Bob leaves: his device stops sending, and Alice's driver commits the removal through the same pipeline
    expect((await db.do(bob.ctx, { kind: "leave", conversationId: conv })).ok).toBe(true);
    await quiet();
    await da2.idle();
    expect(await db.do(bob.ctx, { kind: "send_text", conversationId: conv, text: "late" })).toMatchObject({ ok: false });
    expect(da2.get(alice.ctx, conv)?.summary.members).toBe(1);
  });

  it("through createCoreRuntime: the runtime owns the driver, and a restarted runtime sends what was owed", async () => {
    const net = createGroupLoopback();
    const [alice, bob] = await connect(net);
    net.connect({ profileId: alice.id, relationship: alice.rel }, { profileId: bob.id, relationship: bob.rel });
    net.device(alice.id, alice.device);
    net.device(bob.id, bob.device);
    const quiet = async (): Promise<void> => { for (let i = 0; i < 10; i++) { await alice.rt.idle(); await bob.rt.idle(); } };
    expect(alice.rt.groups.available()).toBe(true);
    await alice.rt.groups.do(alice.ctx, { kind: "create", name: "Runtime", relationshipIds: [alice.rel] });
    await quiet();
    await bob.rt.groups.do(bob.ctx, { kind: "accept", inviteId: bob.rt.groups.invites(bob.ctx)[0]?.id ?? "" });
    await quiet();
    const conv = asGroupId(alice.rt.groups.list(alice.ctx)[0]?.conversationId ?? "");
    expect(bob.rt.groups.list(bob.ctx)[0]?.members).toBe(2);
    // Bob's side is down; Alice sends, then her whole runtime restarts over the same store: the message is still owed
    net.setDown(bob.id, true);
    await alice.rt.groups.do(alice.ctx, { kind: "send_text", conversationId: conv, text: "owed across a restart" });
    await quiet();
    alice.rt.dispose();
    const again = await boot(createMemoryProvider(), alice.id, { carriage: net.carriage(alice.id), acceptance: acceptAfterDelivery }, alice.kv);
    net.setDown(bob.id, false);
    await again.rt.idle();
    await bob.rt.idle();
    expect(bob.rt.groups.messages(bob.ctx, conv).map((m) => m.text)).toEqual(["owed across a restart"]);
    expect(again.rt.groups.messages(again.ctx, conv).map((m) => m.text)).toEqual(["owed across a restart"]);
  });

  it("a host with no carriage cannot use groups even though the module has the engine", async () => {
    const [alice] = await connect(undefined);
    const bare = await boot(createMemoryProvider(), asProfileId("solo"), { carriage: null });
    expect(alice.rt.groups.available()).toBe(true); // the bare engine, as the hand-carried tests use it
    expect(bare.rt.groups.available()).toBe(false);
    expect(await bare.rt.groups.do(bare.ctx, { kind: "create", name: "x", relationshipIds: [] })).toEqual({ ok: false, refused: "groups_unavailable" });
  });

  it("without a policy a prepared Commit stays prepared: the group is paused and the Welcome is not sent", async () => {
    const [alice, bob] = await connect();
    const net = createGroupLoopback();
    net.connect({ profileId: alice.id, relationship: alice.rel }, { profileId: bob.id, relationship: bob.rel });
    net.device(alice.id, alice.device);
    net.device(bob.id, bob.device);
    const mk = (p: Party): ReturnType<typeof createGroupDriver> => createGroupDriver({ engine: p.rt.groups, storage: () => p.kv, contextFor: (id) => p.rt.contextFor(id), carriage: net.carriage(p.id), retryMs: 0 });
    const da = mk(alice);
    const db = mk(bob);
    await da.do(alice.ctx, { kind: "create", name: "Paused", relationshipIds: [alice.rel] });
    for (let i = 0; i < 6; i++) { await da.idle(); await db.idle(); }
    await db.do(bob.ctx, { kind: "accept", inviteId: db.invites(bob.ctx)[0]?.id ?? "" });
    for (let i = 0; i < 6; i++) { await da.idle(); await db.idle(); }
    const conv = asGroupId(da.list(alice.ctx)[0]?.conversationId ?? "");
    expect(da.get(alice.ctx, conv)?.canSend).toBe(false);
    expect(net.log.some((l) => l.wire.t === "welcome")).toBe(false);
    expect(db.list(bob.ctx)).toEqual([]);
  });
});

