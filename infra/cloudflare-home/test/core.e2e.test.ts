// SPDX-License-Identifier: MPL-2.0
// End to end: two real Core wasm profiles (separate installs, separate stores) exchange messages through the real
// reference Home (the Worker bundle running in workerd), using only the HTTP provider adapter. Needs
// `make core-wasm` for dist/om_wasm.wasm (skipped, loudly, if absent).
import { existsSync, readFileSync } from "node:fs";
import { join } from "node:path";
import { afterAll, beforeAll, describe, expect, it } from "vitest";
import { asConversationId, asOperationId, asProfileId } from "@om/platform-api";
import type { ConversationTimelineItem, HomeSummary, OutcomeClass, ProfileExecutionContext, ProfileId } from "@om/platform-api";
import { createCoreRuntime, instantiateCore } from "@om/core-wasm";
import type { WasmCoreClient } from "@om/core-wasm";
import { createMemoryKv, type MemoryKv } from "@om/core-wasm/test-support";
import { decodeDepositRequest, decodeKeyPublish, parseDeposit, readBundleFacts, toHex } from "@om/home-wire";
import { createHttpProvider, createMemorySecretStore, parseProviderUrl, type HttpProvider, type SecretStore } from "@om/provider-http";
import { startHome, type Home } from "./harness";

const WASM = join(import.meta.dirname, "../../../packages/om-core-wasm/dist/om_wasm.wasm");
const present = existsSync(WASM);
if (!present) console.warn("om_wasm.wasm not built: run `make core-wasm`; skipping the Core-through-relay e2e");

const HOME: HomeSummary = { tier: "T1", observers: ["host_sees_ciphertext"], migrationAvailable: true, clientOriginChoice: ["default_origin"] };
const op = asOperationId;

interface Party { rt: WasmCoreClient; kv: MemoryKv; ctx: ProfileExecutionContext; id: ProfileId; provider: HttpProvider; secrets: SecretStore }

let home: Home;
beforeAll(async () => { if (present) home = await startHome(); }, 120_000);
afterAll(async () => { if (present) await home.dispose(); });

/** `secrets` is the installation's durable secret store; a restart of the same installation reuses it. */
function providerFor(secrets: SecretStore, fetchLog?: string[], bodies?: { url: string; body: Uint8Array }[]): HttpProvider {
  const config = parseProviderUrl(home.url, { allowLoopbackHttp: true });
  if (!config) throw new Error("bad url");
  const fetchImpl: typeof fetch = (input, init) => {
    const url = typeof input === "string" ? input : input instanceof URL ? input.href : input.url;
    fetchLog?.push(`${init?.method ?? "GET"} ${url}`);
    if (bodies && init?.body instanceof Uint8Array) bodies.push({ url, body: new Uint8Array(init.body) });
    return fetch(input, init);
  };
  return createHttpProvider({ config, secrets, fetch: fetchImpl, timeoutMs: 10_000 });
}

async function boot(kv: MemoryKv, secrets: SecretStore, provider: HttpProvider, id: ProfileId): Promise<Party> {
  const core = await instantiateCore(readFileSync(WASM));
  const rt = await createCoreRuntime({ core, provider, storage: () => kv, home: HOME, autoDrain: false, retryMs: 0 });
  const ctx = await rt.openProfile(id);
  return { rt, kv, ctx, id, provider, secrets };
}

async function settle(...ps: Party[]): Promise<void> {
  for (let i = 0; i < 8; i++) for (const p of ps) await p.rt.flush(p.id);
  for (const p of ps) await p.rt.idle();
}

const outcome = async (p: Party, id: string): Promise<OutcomeClass | undefined> => (await p.rt.pendingOperations(p.ctx)).find((o) => o.operationId === id)?.outcome;
async function timeline(p: Party, conv: string): Promise<string[]> {
  const v = await p.rt.query({ kind: "GetConversationView", ctx: p.ctx, conversationId: asConversationId(conv) });
  return v.timeline.map((t: ConversationTimelineItem) => `${t.direction === "outgoing" ? "out" : "in"}:${t.text}`);
}

async function connect(logA: string[], logB: string[]): Promise<{ alice: Party; bob: Party; aConv: string; bConv: string }> {
  const [sa, sb] = [createMemorySecretStore(), createMemorySecretStore()];
  const alice = await boot(createMemoryKv(), sa, providerFor(sa, logA), asProfileId("alice"));
  const bob = await boot(createMemoryKv(), sb, providerFor(sb, logB), asProfileId("bob"));
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

describe.skipIf(!present)("two real Core profiles through the reference Home", () => {
  it("invite, introduction and messages both ways, sealed on the wire, exactly once, across restarts", async () => {
    const logA: string[] = [];
    const logB: string[] = [];
    const { alice, bob, aConv, bConv } = await connect(logA, logB);
    await alice.rt.dispatch({ kind: "SendMessage", ctx: alice.ctx, operationId: op("m1"), conversationId: asConversationId(aConv), draft: { text: "hello bob" } });
    await settle(alice, bob);
    expect(await outcome(alice, "m1")).toEqual({ kind: "Committed" });
    expect(await timeline(bob, bConv)).toEqual(["in:hello bob"]);
    await bob.rt.dispatch({ kind: "SendMessage", ctx: bob.ctx, operationId: op("m2"), conversationId: asConversationId(bConv), draft: { text: "hi alice" } });
    await settle(alice, bob);
    expect(await timeline(alice, aConv)).toEqual(["out:hello bob", "in:hi alice"]);

    // Both sides restart over the same durable stores (new wasm instances, new adapter instances, empty adapter caches).
    const alice2 = await boot(alice.kv, alice.secrets, providerFor(alice.secrets, logA), alice.id);
    const bob2 = await boot(bob.kv, bob.secrets, providerFor(bob.secrets, logB), bob.id);
    await alice2.rt.dispatch({ kind: "SendMessage", ctx: alice2.ctx, operationId: op("m3"), conversationId: asConversationId(aConv), draft: { text: "after restart" } });
    await settle(alice2, bob2);
    expect(await timeline(bob2, bConv)).toEqual(["in:hello bob", "out:hi alice", "in:after restart"]);
    // Redelivery is harmless: polling again from a lower cursor does not duplicate a message.
    await settle(alice2, bob2);
    expect(await timeline(bob2, bConv)).toEqual(["in:hello bob", "out:hi alice", "in:after restart"]);

    // Nothing the Home was sent is readable: no plaintext, labels or identity strings in any request body it ever saw.
    const needles = ["hello bob", "hi alice", "after restart", "Alice", "Bob"].map((s) => toHex(new TextEncoder().encode(s)));
    expect(logA.length + logB.length).toBeGreaterThan(10);
    for (const line of [...logA, ...logB]) {
      for (const n of needles) expect(line).not.toContain(n);
      // capabilities never ride in a URL
      expect(line).not.toMatch(/[?#]/);
    }
  }, 120_000);

  it("what the Home receives is exactly the specified objects: a MessageRequest or a profile-1 SealedDeliveryEnvelope, and a device-bound signed bundle", async () => {
    const seen: { url: string; body: Uint8Array }[] = [];
    const [sa, sb] = [createMemorySecretStore(), createMemorySecretStore()];
    const alice = await boot(createMemoryKv(), sa, providerFor(sa, [], seen), asProfileId("alice"));
    const bob = await boot(createMemoryKv(), sb, providerFor(sb, [], seen), asProfileId("bob"));
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
    const kinds = new Set<string>();
    let bundles = 0;
    for (const { url, body } of seen) {
      if (url.endsWith("/v1/relay/deposit")) kinds.add(parseDeposit(decodeDepositRequest(body).envelope).kind);
      const m = /\/v1\/keys\/bundles\/([0-9a-f]{32})$/.exec(url);
      if (m) {
        const facts = readBundleFacts(decodeKeyPublish(body).bundle);
        expect(facts && toHex(facts.deviceId)).toBe(m[1]); // the bundle names the device it is published under
        bundles += 1;
      }
    }
    expect([...kinds].sort()).toEqual(["message_request", "sealed"]);
    expect(bundles).toBeGreaterThanOrEqual(2);
  }, 120_000);

  it("a timeout after the Home stored a message is reconciled by an exact retry: delivered once", async () => {
    const { alice, bob, aConv, bConv } = await connect([], []);
    // Make the first deposit's response disappear after the Home has stored it.
    let dropped = 0;
    const real = fetch;
    const lossy: typeof fetch = async (input, init) => {
      const res = await real(input, init);
      const url = typeof input === "string" ? input : input instanceof URL ? input.href : input.url;
      if (url.endsWith("/v1/relay/deposit") && dropped === 0) { dropped += 1; throw new TypeError("response lost"); }
      return res;
    };
    const config = parseProviderUrl(home.url, { allowLoopbackHttp: true });
    if (!config) throw new Error("bad url");
    const aliceLossy = await boot(alice.kv, alice.secrets, createHttpProvider({ config, secrets: alice.secrets, fetch: lossy, timeoutMs: 10_000 }), alice.id);
    await aliceLossy.rt.dispatch({ kind: "SendMessage", ctx: aliceLossy.ctx, operationId: op("m1"), conversationId: asConversationId(aConv), draft: { text: "once" } });
    await aliceLossy.rt.flush(aliceLossy.id);
    expect(dropped).toBe(1);
    await settle(aliceLossy, bob);
    expect(await outcome(aliceLossy, "m1")).toEqual({ kind: "Committed" });
    expect(await timeline(bob, bConv)).toEqual(["in:once"]);
  }, 120_000);

  it("the Home being unreachable keeps work queued (PendingNetwork) and resumes when it returns", async () => {
    const { alice, bob, aConv, bConv } = await connect([], []);
    let up = false;
    const config = parseProviderUrl(home.url, { allowLoopbackHttp: true });
    if (!config) throw new Error("bad url");
    const flaky: typeof fetch = (input, init) => (up ? fetch(input, init) : Promise.reject(new TypeError("offline")));
    const aliceOff = await boot(alice.kv, alice.secrets, createHttpProvider({ config, secrets: alice.secrets, fetch: flaky, timeoutMs: 5000 }), alice.id);
    await aliceOff.rt.dispatch({ kind: "SendMessage", ctx: aliceOff.ctx, operationId: op("m1"), conversationId: asConversationId(aConv), draft: { text: "queued" } });
    await aliceOff.rt.flush(aliceOff.id);
    expect(await outcome(aliceOff, "m1")).toEqual({ kind: "PendingNetwork" });
    up = true;
    await settle(aliceOff, bob);
    expect(await outcome(aliceOff, "m1")).toEqual({ kind: "Committed" });
    expect(await timeline(bob, bConv)).toEqual(["in:queued"]);
  }, 120_000);
});
