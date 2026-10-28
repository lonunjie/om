// Runs the real Core (om-wasm) through the platform-api CoreClient: onboarding, invite and introduction, two-party
// messaging with ratcheting across restarts, commit-before-release and unknown network outcomes. Needs
// `pnpm --filter @om/core-wasm build:wasm` (make core-wasm); the suite is skipped, loudly, only if the artifact is absent.
import { existsSync, readFileSync } from "node:fs";
import { join } from "node:path";
import { describe, expect, it } from "vitest";
import { asConversationId, asOperationId, asProfileId } from "@om/platform-api";
import type { ConfirmationToken, ConversationSummary, ConversationTimelineItem, HomeSummary, OutcomeClass, ProfileExecutionContext, ProfileId } from "@om/platform-api";
import { createCoreRuntime, CoreStorageError, instantiateCore } from "./index";
import type { WasmCoreClient } from "./index";
import { createMemoryKv, createMemoryProvider, type MemoryKv, type MemoryProvider } from "./test-support";

const WASM = join(import.meta.dirname, "../dist/om_wasm.wasm");
const present = existsSync(WASM);
if (!present) console.warn("om_wasm.wasm not built: run `make core-wasm`; skipping the real-Core suite");

const HOME: HomeSummary = { tier: "T1", observers: ["host_sees_ciphertext"], migrationAvailable: true, clientOriginChoice: ["default_origin"] };
const op = asOperationId;

interface Party { rt: WasmCoreClient; kv: MemoryKv; ctx: ProfileExecutionContext; id: ProfileId }

async function boot(kv: MemoryKv, provider: MemoryProvider, id: ProfileId, retryMs = 0): Promise<Party> {
  const core = await instantiateCore(readFileSync(WASM));
  const rt = await createCoreRuntime({ core, provider, storage: () => kv, home: HOME, autoDrain: false, retryMs });
  const ctx = await rt.openProfile(id);
  return { rt, kv, ctx, id };
}

async function settle(...ps: Party[]): Promise<void> {
  for (let i = 0; i < 20; i++) {
    for (const p of ps) await p.rt.flush(p.id);
  }
  for (const p of ps) await p.rt.idle();
}

async function outcome(p: Party, id: string): Promise<OutcomeClass | undefined> {
  return (await p.rt.pendingOperations(p.ctx)).find((o) => o.operationId === id)?.outcome;
}

async function timeline(p: Party, conv: string): Promise<string[]> {
  const v = await p.rt.query({ kind: "GetConversationView", ctx: p.ctx, conversationId: asConversationId(conv) });
  return v.timeline.map((t: ConversationTimelineItem) => `${t.direction === "outgoing" ? "out" : "in"}:${t.text}`);
}

async function connect(provider: MemoryProvider): Promise<{ alice: Party; bob: Party; aConv: string; bConv: string }> {
  const alice = await boot(createMemoryKv(), provider, asProfileId("alice"));
  const bob = await boot(createMemoryKv(), provider, asProfileId("bob"));
  await alice.rt.dispatch({ kind: "CreateIdentityProfile", ctx: alice.ctx, operationId: op("a-create"), label: "Alice" });
  await bob.rt.dispatch({ kind: "CreateIdentityProfile", ctx: bob.ctx, operationId: op("b-create"), label: "Bob" });
  await settle(alice, bob);
  await alice.rt.dispatch({ kind: "CreateInvite", ctx: alice.ctx, operationId: op("a-invite") });
  await settle(alice, bob);
  const invite = await alice.rt.query({ kind: "GetInvite", ctx: alice.ctx, operationId: op("a-invite") });
  if (!invite) throw new Error("no invite");
  await bob.rt.dispatch({ kind: "AcceptInvitation", ctx: bob.ctx, operationId: op("b-accept"), link: invite.link });
  await settle(alice, bob);
  const [intro] = await alice.rt.query({ kind: "GetIntroductions", ctx: alice.ctx });
  if (!intro) throw new Error("no introduction");
  await alice.rt.dispatch({ kind: "AcceptIntroduction", ctx: alice.ctx, operationId: op("a-intro"), requestId: intro.id });
  await settle(alice, bob);
  const [ac] = await alice.rt.query({ kind: "GetConversationList", ctx: alice.ctx });
  const [bc] = await bob.rt.query({ kind: "GetConversationList", ctx: bob.ctx });
  if (!ac || !bc) throw new Error("no conversation");
  return { alice, bob, aConv: ac.id, bConv: bc.id };
}

describe.skipIf(!present)("real Core through the CoreClient interface", () => {
  it("onboarding: identity, display-once recovery key, profile list, subscriptions", async () => {
    const provider = createMemoryProvider();
    const p = await boot(createMemoryKv(), provider, asProfileId("me"));
    const seen: string[] = [];
    const un = p.rt.subscribe({ kind: "GetProfileList" }, (l) => { seen.push(l.map((x) => x.label).join(",")); });
    const outcomes: OutcomeClass[] = [];
    await p.rt.dispatch({ kind: "CreateIdentityProfile", ctx: p.ctx, operationId: op("c1"), label: "Me" });
    p.rt.observe(p.ctx, op("c1"), (o) => outcomes.push(o));
    expect(outcomes).toEqual([{ kind: "Committed" }]);
    expect(seen).toEqual(["Me"]);
    const key = await p.rt.query({ kind: "GetRecoveryKey", ctx: p.ctx, keyOperationId: op("c1") });
    expect(key?.groups).toHaveLength(14);
    await p.rt.dispatch({ kind: "ConfirmRecoveryKeyStored", ctx: p.ctx, operationId: op("c2"), keyOperationId: op("c1") });
    expect(await p.rt.query({ kind: "GetRecoveryKey", ctx: p.ctx, keyOperationId: op("c1") })).toBeNull();
    const devices = await p.rt.query({ kind: "GetDevices", ctx: p.ctx });
    expect(devices.devices).toHaveLength(1);
    expect(devices.devices[0]?.isCurrent).toBe(true);
    un();
  });

  it("without a provider that allocates routes, an invite still works and a first contact is refused as unavailable (no stand-in route)", async () => {
    const full = createMemoryProvider();
    const alice = await boot(createMemoryKv(), full, asProfileId("alice"));
    await alice.rt.dispatch({ kind: "CreateIdentityProfile", ctx: alice.ctx, operationId: op("a-create"), label: "Alice" });
    await alice.rt.dispatch({ kind: "CreateInvite", ctx: alice.ctx, operationId: op("a-invite") });
    await settle(alice);
    const invite = await alice.rt.query({ kind: "GetInvite", ctx: alice.ctx, operationId: op("a-invite") });
    if (!invite) throw new Error("no invite");
    // Bob's host has a provider that can carry calls but allocates no routes (the shape of "no Home configured").
    const bare: MemoryProvider = { ...createMemoryProvider(), perform: full.perform.bind(full), poll: full.poll.bind(full) };
    delete (bare as { allocateRoutes?: unknown }).allocateRoutes;
    const bob = await boot(createMemoryKv(), bare, asProfileId("bob"));
    await bob.rt.dispatch({ kind: "CreateIdentityProfile", ctx: bob.ctx, operationId: op("b-create"), label: "Bob" });
    await bob.rt.dispatch({ kind: "AcceptInvitation", ctx: bob.ctx, operationId: op("b-accept"), link: invite.link });
    await settle(alice, bob);
    expect(await outcome(bob, "b-accept")).toEqual({ kind: "Rejected", reason: "unavailable" });
    expect(await alice.rt.query({ kind: "GetIntroductions", ctx: alice.ctx })).toEqual([]);
  });

  it("two parties message through invite, introduction and a ratcheting session, across restarts", async () => {
    const provider = createMemoryProvider();
    const { alice, bob, aConv, bConv } = await connect(provider);
    expect((await alice.rt.query({ kind: "GetConversationList", ctx: alice.ctx })).map((c: ConversationSummary) => c.displayName)).toEqual(["Bob"]);
    await alice.rt.dispatch({ kind: "SendMessage", ctx: alice.ctx, operationId: op("m1"), conversationId: asConversationId(aConv), draft: { text: "hello bob" } });
    await settle(alice, bob);
    expect(await outcome(alice, "m1")).toEqual({ kind: "Committed" });
    expect(await timeline(bob, bConv)).toEqual(["in:hello bob"]);
    // Both sides restart: new wasm instances over the same durable stores.
    const alice2 = await boot(alice.kv, provider, alice.id);
    const bob2 = await boot(bob.kv, provider, bob.id);
    expect(alice2.ctx.generation).not.toBe(alice.ctx.generation);
    await bob2.rt.dispatch({ kind: "SendMessage", ctx: bob2.ctx, operationId: op("m2"), conversationId: asConversationId(bConv), draft: { text: "hi alice" } });
    await alice2.rt.dispatch({ kind: "SendMessage", ctx: alice2.ctx, operationId: op("m3"), conversationId: asConversationId(aConv), draft: { text: "second" } });
    await settle(alice2, bob2);
    // m2 and m3 were sent concurrently, so their relative order is arrival order; the first message is causally first.
    const at = await timeline(alice2, aConv);
    const bt = await timeline(bob2, bConv);
    expect(at[0]).toBe("out:hello bob");
    expect(bt[0]).toBe("in:hello bob");
    expect([...at].sort()).toEqual(["in:hi alice", "out:hello bob", "out:second"]);
    expect([...bt].sort()).toEqual(["in:hello bob", "in:second", "out:hi alice"]);
    // The relay saw only ciphertext.
    const needles = ["hello bob", "hi alice", "second", "Alice", "Bob"].map((s) => Buffer.from(s).toString("hex"));
    for (const bytes of provider.deposited()) for (const n of needles) expect(bytes).not.toContain(n);
  });

  it("first contact on the real Core: the request is unverified, the relationship is unbound until both bindings verify", async () => {
    const provider = createMemoryProvider();
    const alice = await boot(createMemoryKv(), provider, asProfileId("alice"));
    const bob = await boot(createMemoryKv(), provider, asProfileId("bob"));
    await alice.rt.dispatch({ kind: "CreateIdentityProfile", ctx: alice.ctx, operationId: op("a-create"), label: "Alice" });
    await bob.rt.dispatch({ kind: "CreateIdentityProfile", ctx: bob.ctx, operationId: op("b-create"), label: "Bob" });
    await settle(alice, bob);
    await alice.rt.dispatch({ kind: "CreateInvite", ctx: alice.ctx, operationId: op("a-invite") });
    await settle(alice, bob);
    const invite = await alice.rt.query({ kind: "GetInvite", ctx: alice.ctx, operationId: op("a-invite") });
    if (!invite) throw new Error("no invite");
    await bob.rt.dispatch({ kind: "AcceptInvitation", ctx: bob.ctx, operationId: op("b-accept"), link: invite.link });
    await settle(alice, bob);
    const intros = await alice.rt.query({ kind: "GetIntroductions", ctx: alice.ctx });
    expect(intros.map((i) => [i.displayName, i.verification])).toEqual([["Bob", "unverified"]]);
    expect(await alice.rt.query({ kind: "GetIntroductionLoad", ctx: alice.ctx })).toEqual({ throttled: false, dropped: 0 });
    await alice.rt.dispatch({ kind: "AcceptIntroduction", ctx: alice.ctx, operationId: op("a-intro"), requestId: intros[0]?.id ?? "" });
    // Before the symmetric bindings verify (no flush yet), Alice's contact is unbound and unverified.
    const [pending] = await alice.rt.query({ kind: "GetConversationList", ctx: alice.ctx });
    expect(pending?.relationship).toBe("accepted_unbound");
    expect(pending?.notices.map((n) => n.code)).toContain("unverified_contact");
    await settle(alice, bob);
    for (const p of [alice, bob]) {
      const [c] = await p.rt.query({ kind: "GetConversationList", ctx: p.ctx });
      expect(c?.relationship).toBe("active");
      expect(c?.notices).toEqual([]);
    }
  });

  it("destructive intents need a Core-issued nonce for exactly that kind and target (M2)", async () => {
    const provider = createMemoryProvider();
    const { alice, aConv } = await connect(provider);
    const [conv] = await alice.rt.query({ kind: "GetConversationList", ctx: alice.ctx });
    if (!conv) throw new Error("no conversation");
    const person = conv.personId;
    const nonce = async (id: string, target: string): Promise<ConfirmationToken> => {
      await alice.rt.dispatch({ kind: "IssueConfirmation", ctx: alice.ctx, operationId: op(id), intentKind: "BlockPerson", target });
      const v = await alice.rt.query({ kind: "GetConfirmation", ctx: alice.ctx, operationId: op(id) });
      return { nonce: v?.nonce ?? "" } as ConfirmationToken;
    };
    const block = async (id: string, confirmation: unknown): Promise<OutcomeClass | undefined> => {
      await alice.rt.dispatch({ kind: "BlockPerson", ctx: alice.ctx, operationId: op(id), personId: person, scope: "local", confirmation: confirmation as ConfirmationToken });
      return outcome(alice, id);
    };
    expect(await block("b0", Object.freeze({}))).toEqual({ kind: "Rejected", reason: "unauthorized" });
    expect(await block("b1", { nonce: "ab".repeat(16) })).toEqual({ kind: "Rejected", reason: "unauthorized" });
    const other = await nonce("n-other", "someone-else");
    expect(await block("b2", other)).toEqual({ kind: "Rejected", reason: "unauthorized" });
    const good = await nonce("n-good", person);
    expect(await block("b3", good)).toEqual({ kind: "Committed" });
    expect(await block("b4", good)).toEqual({ kind: "Rejected", reason: "unauthorized" });
    expect(aConv).not.toBe("");
  });

  it("a refused store write releases nothing, and the same operation id can be dispatched again", async () => {
    const provider = createMemoryProvider();
    const { alice, bob, aConv, bConv } = await connect(provider);
    alice.kv.failNextPut();
    await expect(alice.rt.dispatch({ kind: "SendMessage", ctx: alice.ctx, operationId: op("m1"), conversationId: asConversationId(aConv), draft: { text: "lost" } })).rejects.toBeInstanceOf(CoreStorageError);
    expect(await timeline(alice, aConv)).toEqual([]);
    expect(await outcome(alice, "m1")).toBeUndefined();
    await alice.rt.dispatch({ kind: "SendMessage", ctx: alice.ctx, operationId: op("m1"), conversationId: asConversationId(aConv), draft: { text: "kept" } });
    await settle(alice, bob);
    expect(await timeline(bob, bConv)).toEqual(["in:kept"]);
  });

  it("a timeout after send stays PendingReconciliation, is retried with the same identity and delivered once", async () => {
    const provider = createMemoryProvider();
    const { alice, bob, aConv, bConv } = await connect(provider);
    await alice.rt.dispatch({ kind: "SendMessage", ctx: alice.ctx, operationId: op("m1"), conversationId: asConversationId(aConv), draft: { text: "once" } });
    provider.inject("timeout_after_effect");
    await alice.rt.flush(alice.id);
    expect(await outcome(alice, "m1")).toEqual({ kind: "PendingReconciliation" });
    const [first] = (await alice.rt.query({ kind: "GetConversationView", ctx: alice.ctx, conversationId: asConversationId(aConv) })).timeline;
    expect(first).toMatchObject({ direction: "outgoing", delivery: { state: "pending_reconciliation" } });
    await settle(alice, bob);
    expect(await outcome(alice, "m1")).toEqual({ kind: "Committed" });
    expect(await timeline(bob, bConv)).toEqual(["in:once"]);
  });

  it("contexts are profile-bound: a forged or stale context is refused and nothing crosses profiles", async () => {
    const provider = createMemoryProvider();
    const p = await boot(createMemoryKv(), provider, asProfileId("one"));
    const forged = { profileId: asProfileId("one"), generation: "gen-0" as ProfileExecutionContext["generation"] };
    await expect(p.rt.query({ kind: "GetDevices", ctx: forged })).rejects.toMatchObject({ code: "profile_access" });
    await expect(p.rt.dispatch({ kind: "CreateInvite", ctx: forged, operationId: op("x") })).rejects.toMatchObject({ code: "profile_access" });
    const other = await boot(createMemoryKv(), provider, asProfileId("two"));
    await p.rt.dispatch({ kind: "CreateIdentityProfile", ctx: p.ctx, operationId: op("c"), label: "One" });
    expect(await other.rt.pendingOperations(other.ctx)).toEqual([]);
    expect([...p.kv.snapshot().keys()]).toEqual(["core/state"]);
  });
});
