// SPDX-License-Identifier: MPL-2.0
// Two people, each on their own Home (two separate workerd instances), complete invite -> IntroGate -> MessageRequest -> chat
// with two real Core wasm profiles and the HTTP adapter (deviations D4 and D10; plan: docs/reports/2026-11-03-first-contact-across-homes-plan.md).
// Conformance cases: DS-01 (a signed descriptor via two providers), DS-05 (the discovery provider is unavailable), PC-A05
// (one capability, one relay) and the V-020 vendor-block drill as far as one route per relationship allows.
// Needs `make core-wasm` for dist/om_wasm.wasm (skipped, loudly, if absent).
import { existsSync, readFileSync } from "node:fs";
import { join } from "node:path";
import { afterAll, beforeAll, describe, expect, it } from "vitest";
import { asConversationId, asOperationId, asProfileId } from "@om/platform-api";
import type { ConversationTimelineItem, HomeSummary, OutcomeClass, ProfileExecutionContext, ProfileId } from "@om/platform-api";
import { createCoreRuntime, instantiateCore } from "@om/core-wasm";
import type { WasmCoreClient } from "@om/core-wasm";
import { createMemoryKv, type MemoryKv } from "@om/core-wasm/test-support";
import {
  admissionHash, concat, decodeDescriptorGet, decodeHomeError, encode, encodeDepositRequest, encodeKeyFetch, fromBase64Url, fromHex, map, toBase64Url, toHex,
} from "@om/home-wire";
import { createHttpProvider, createMemorySecretStore, parseProviderUrl, type HttpProvider, type SecretStore } from "@om/provider-http";
import { startHome, type Home } from "./harness";
import { asBody, rand, sealed } from "./kit";

const WASM = join(import.meta.dirname, "../../../packages/om-core-wasm/dist/om_wasm.wasm");
const present = existsSync(WASM);
if (!present) console.warn("om_wasm.wasm not built: run `make core-wasm`; skipping the two-Home e2e");

const HOME: HomeSummary = { tier: "T1", observers: ["host_sees_ciphertext"], migrationAvailable: true, clientOriginChoice: ["default_origin"] };
const op = asOperationId;

/** Alice's Home (a) and Bob's Home (b) ask for their owner's admission secret, as a freshly deployed personal Home does; c and d are open. */
let a: Home; let b: Home; let c: Home; let d: Home;
const adm = { a: crypto.getRandomValues(new Uint8Array(32)), b: crypto.getRandomValues(new Uint8Array(32)) };
beforeAll(async () => {
  if (!present) return;
  [a, b, c, d] = await Promise.all([
    startHome({ vars: { OM_ADMISSION: await admissionHash(adm.a) } }),
    startHome({ vars: { OM_ADMISSION: await admissionHash(adm.b) } }),
    startHome(),
    startHome(),
  ]);
}, 180_000);
afterAll(async () => { if (present) await Promise.all([a, b, c, d].map((h) => h.dispose())); });

interface Req { readonly by: string; readonly home: string; readonly method: string; readonly path: string; readonly admission: boolean }
/** Every request any adapter made, and a switch that makes a Home unreachable (a vendor block, a dead machine). */
class World {
  readonly log: Req[] = [];
  readonly down = new Set<string>();
  readonly homes: Home[];
  constructor(...homes: Home[]) { this.homes = homes; }
  fetchFor(by: string): typeof fetch {
    return async (input, init) => {
      const url = new URL(typeof input === "string" ? input : input instanceof URL ? input.href : input.url);
      this.log.push({ by, home: url.origin, method: init?.method ?? "GET", path: url.pathname, admission: new Headers(init?.headers).has("om-admission") });
      if (this.down.has(url.origin)) throw new TypeError("blocked");
      return fetch(input, init);
    };
  }
  by(who: string, home: Home): Req[] { return this.log.filter((r) => r.by === who && r.home === home.url); }
}

interface Party { rt: WasmCoreClient; kv: MemoryKv; ctx: ProfileExecutionContext; id: ProfileId; provider: HttpProvider; secrets: SecretStore }

function providerFor(world: World, who: string, home: Home, secrets: SecretStore, admission?: Uint8Array): HttpProvider {
  const config = parseProviderUrl(home.url, { allowLoopbackHttp: true });
  if (!config) throw new Error("bad url");
  return createHttpProvider({ config, secrets, fetch: world.fetchFor(who), timeoutMs: 10_000, allowLoopbackHttp: true, ...(admission ? { admission } : {}) });
}

async function boot(kv: MemoryKv, secrets: SecretStore, provider: HttpProvider, id: ProfileId): Promise<Party> {
  const core = await instantiateCore(readFileSync(WASM));
  const rt = await createCoreRuntime({ core, provider, storage: () => kv, home: HOME, autoDrain: false, retryMs: 0 });
  const ctx = await rt.openProfile(id);
  return { rt, kv, ctx, id, provider, secrets };
}

const newParty = async (world: World, who: string, home: Home, admission?: Uint8Array): Promise<Party> => {
  const secrets = createMemorySecretStore();
  return boot(createMemoryKv(), secrets, providerFor(world, who, home, secrets, admission), asProfileId(who));
};

async function settle(...ps: Party[]): Promise<void> {
  for (let i = 0; i < 8; i++) for (const p of ps) await p.rt.flush(p.id);
  for (const p of ps) await p.rt.idle();
}

const outcome = async (p: Party, id: string): Promise<OutcomeClass | undefined> => (await p.rt.pendingOperations(p.ctx)).find((o) => o.operationId === id)?.outcome;
async function timeline(p: Party, conv: string): Promise<string[]> {
  const v = await p.rt.query({ kind: "GetConversationView", ctx: p.ctx, conversationId: asConversationId(conv) });
  return v.timeline.map((t: ConversationTimelineItem) => `${t.direction === "outgoing" ? "out" : "in"}:${t.text}`);
}
const conversations = (p: Party) => p.rt.query({ kind: "GetConversationList", ctx: p.ctx });

async function create(...ps: readonly [Party, string][]): Promise<void> {
  for (const [p, label] of ps) await p.rt.dispatch({ kind: "CreateIdentityProfile", ctx: p.ctx, operationId: op(`${p.id}-create`), label });
  await settle(...ps.map(([p]) => p));
}

async function createInvite(alice: Party, opId: string): Promise<string> {
  await alice.rt.dispatch({ kind: "CreateInvite", ctx: alice.ctx, operationId: op(opId) });
  await settle(alice);
  const invite = await alice.rt.query({ kind: "GetInvite", ctx: alice.ctx, operationId: op(opId) });
  if (!invite) throw new Error("no invite");
  return invite.link;
}

const tokenOf = (link: string): Uint8Array => fromBase64Url(link.slice("om:invite/".length));
const inviteIdOf = (link: string): string => toHex(tokenOf(link).slice(0, 16));
/** The same invite naming other introduction services (a link's services are locators only; the descriptor they lead to is signed). */
const linkNaming = (link: string, uris: readonly string[]): string => `om:invite/${toBase64Url(concat(tokenOf(link).slice(0, 48), encode(uris.map((u) => map([[0, u]])))))}`;
const servicesOf = (link: string): string => new TextDecoder().decode(tokenOf(link).slice(48));

async function introduce(alice: Party, bob: Party, link: string): Promise<{ aConv: string; bConv: string }> {
  await bob.rt.dispatch({ kind: "AcceptInvitation", ctx: bob.ctx, operationId: op("b-accept"), link });
  await settle(alice, bob);
  const [intro] = await alice.rt.query({ kind: "GetIntroductions", ctx: alice.ctx });
  if (!intro) throw new Error("no introduction");
  await alice.rt.dispatch({ kind: "AcceptIntroduction", ctx: alice.ctx, operationId: op("a-intro"), requestId: intro.id });
  await settle(alice, bob);
  const [ac] = await conversations(alice);
  const [bc] = await conversations(bob);
  if (!ac || !bc) throw new Error("no conversation");
  return { aConv: ac.id, bConv: bc.id };
}

const say = async (p: Party, conv: string, id: string, text: string): Promise<void> => {
  await p.rt.dispatch({ kind: "SendMessage", ctx: p.ctx, operationId: op(id), conversationId: asConversationId(conv), draft: { text } });
};

describe.skipIf(!present)("first contact and chat across two Homes", () => {
  it("invite, IntroGate, MessageRequest and messages both ways, each person on their own Home, across restarts", async () => {
    const world = new World(a, b);
    const alice = await newParty(world, "alice", a, adm.a);
    const bob = await newParty(world, "bob", b, adm.b);
    await create([alice, "Alice"], [bob, "Bob"]);

    const link = await createInvite(alice, "a-invite");
    // The invite carries intro_services[]: it names Alice's own Home, in the link and (signed) in the descriptor.
    expect(servicesOf(link)).toContain(`${a.url}/`);
    const { aConv, bConv } = await introduce(alice, bob, link);

    await say(alice, aConv, "m1", "hello bob");
    await settle(alice, bob);
    expect(await outcome(alice, "m1")).toEqual({ kind: "Committed" });
    expect(await timeline(bob, bConv)).toEqual(["in:hello bob"]);
    await say(bob, bConv, "m2", "hi alice");
    await settle(alice, bob);
    expect(await timeline(alice, aConv)).toEqual(["out:hello bob", "in:hi alice"]);

    // Both sides restart over the same durable stores: fresh wasm instances and adapters with empty caches, polling their own Home.
    const alice2 = await boot(alice.kv, alice.secrets, providerFor(world, "alice", a, alice.secrets, adm.a), alice.id);
    const bob2 = await boot(bob.kv, bob.secrets, providerFor(world, "bob", b, bob.secrets, adm.b), bob.id);
    await say(alice2, aConv, "m3", "after restart");
    await say(bob2, bConv, "m4", "and back");
    await settle(alice2, bob2);
    await settle(alice2, bob2); // redelivery is harmless
    // The two restart-era messages were sent concurrently, so only their order relative to each other is open.
    expect((await timeline(bob2, bConv)).sort()).toEqual(["in:after restart", "in:hello bob", "out:and back", "out:hi alice"]);
    expect((await timeline(alice2, aConv)).sort()).toEqual(["in:and back", "in:hi alice", "out:after restart", "out:hello bob"]);

    // Nothing sensitive at the wrong Home. Bob's requests to Alice's Home are what a stranger may make (a capabilities read, the
    // descriptor read, the MessageRequest, deposits to the routes Alice gave); Alice's to Bob's Home are deposits and the key fetch.
    const kinds = (rs: readonly Req[]): string[] => [...new Set(rs.map((r) => `${r.method} ${r.path.replace(/[0-9a-f]{32}/g, "<id>")}`))].sort();
    expect(kinds(world.by("bob", a))).toEqual(["GET /.well-known/om-provider-capabilities", "GET /v1/discovery/descriptors/<id>", "POST /v1/intro/requests", "POST /v1/relay/deposit"]);
    expect(kinds(world.by("alice", b))).toEqual(["GET /.well-known/om-provider-capabilities", "POST /v1/keys/fetch", "POST /v1/relay/deposit"]);
    // Nobody's admission secret ever left their own Home, and it was used where it belongs.
    expect(world.log.filter((r) => r.admission && ((r.by === "alice" && r.home !== a.url) || (r.by === "bob" && r.home !== b.url)))).toEqual([]);
    expect(world.by("alice", a).some((r) => r.admission)).toBe(true);
    expect(world.by("bob", b).some((r) => r.admission)).toBe(true);
    // No query or fragment in any URL. Alice's invite exists at her Home and not at Bob's.
    for (const r of world.log) expect(r.path).not.toMatch(/[?#]/);
    expect((await a.fetch(`/v1/discovery/descriptors/${inviteIdOf(link)}`)).status).toBe(200);
    expect((await b.fetch(`/v1/discovery/descriptors/${inviteIdOf(link)}`)).status).toBe(404);
  }, 180_000);

  it("DS-05: the inviter's Home is unavailable when the invitee accepts: pending, no relationship, nothing downgraded; it completes when the Home returns", async () => {
    const world = new World(c, d);
    const alice = await newParty(world, "alice", c);
    const bob = await newParty(world, "bob", d);
    await create([alice, "Alice"], [bob, "Bob"]);
    const link = await createInvite(alice, "a-invite");

    world.down.add(c.url); // Alice's Home is blocked for everyone, and Alice cannot poll it either
    await bob.rt.dispatch({ kind: "AcceptInvitation", ctx: bob.ctx, operationId: op("b-accept"), link });
    await settle(bob);
    expect(await outcome(bob, "b-accept")).toEqual({ kind: "PendingNetwork" });
    expect(await conversations(bob)).toEqual([]); // no relationship, no placeholder identity
    expect(await alice.rt.query({ kind: "GetIntroductions", ctx: alice.ctx })).toEqual([]);

    world.down.delete(c.url);
    await settle(alice, bob);
    const [intro] = await alice.rt.query({ kind: "GetIntroductions", ctx: alice.ctx });
    expect(intro?.verification).toBe("unverified");
    await alice.rt.dispatch({ kind: "AcceptIntroduction", ctx: alice.ctx, operationId: op("a-intro"), requestId: intro?.id ?? "" });
    await settle(alice, bob);
    expect((await conversations(alice)).length).toBe(1);
    expect((await conversations(bob)).length).toBe(1);
  }, 120_000);

  it("DS-05: a link naming a host the client will not contact fails clearly, and sends it nothing", async () => {
    const world = new World(c, d);
    const alice = await newParty(world, "alice", c);
    const bob = await newParty(world, "bob", d);
    await create([alice, "Alice"], [bob, "Bob"]);
    const link = await createInvite(alice, "a-invite");
    // Anything but https (or the opted-in loopback) is refused outright, whatever the invite's signer says.
    const bad = ["http://example.invalid/", "https://user:pw@example.invalid/", `${c.url}/?cap=1`, "ftp://example.invalid/"];
    for (const [i, uri] of bad.entries()) {
      await bob.rt.dispatch({ kind: "AcceptInvitation", ctx: bob.ctx, operationId: op(`b-bad-${String(i)}`), link: linkNaming(link, [uri]) });
      await settle(bob);
      expect(await outcome(bob, `b-bad-${String(i)}`), uri).toEqual({ kind: "Rejected", reason: "unsupported" });
    }
    expect(world.by("bob", c)).toEqual([]);
    expect(world.log.filter((r) => r.by === "bob" && r.home.includes("example.invalid"))).toEqual([]);
    expect(await conversations(bob)).toEqual([]);
  }, 120_000);

  it("DS-01: the same signed descriptor via two providers has the same verified semantics; a descriptor altered by a provider is refused", async () => {
    const world = new World(c, d);
    const alice = await newParty(world, "alice", c);
    const bob = await newParty(world, "bob", d);
    await create([alice, "Alice"], [bob, "Bob"]);
    const link = await createInvite(alice, "a-invite");
    const read = async (id: string) => decodeDescriptorGet(new Uint8Array(await (await c.fetch(`/v1/discovery/descriptors/${id}`)).arrayBuffer())).body;

    // A second provider (Bob's Home) carries the identical bytes; Alice publishes them there with her own install.
    const second = providerFor(world, "alice", d, alice.secrets);
    const put = (id: string, body: Uint8Array) => second.perform({ call: "put_invite", inviteId: id, descriptor: toHex(body), expiresAt: Date.now() + 3_600_000, payloadLimit: 32_768 });
    expect(await put(inviteIdOf(link), await read(inviteIdOf(link)))).toEqual({ result: "ok" });
    const { aConv, bConv } = await introduce(alice, bob, linkNaming(link, [`${d.url}/`]));
    // Same relationship as through Alice's own Home: the request went to the service her signed descriptor names, not to the one that served it.
    expect(world.by("bob", c).map((r) => r.path)).toContain("/v1/intro/requests");
    expect(world.by("bob", d).map((r) => r.path)).not.toContain("/v1/intro/requests");
    await say(alice, aConv, "m1", "same either way");
    await settle(alice, bob);
    expect(await timeline(bob, bConv)).toEqual(["in:same either way"]);

    // A provider that alters the descriptor cannot make anyone trust it: the signature and the link's key digest decide.
    const link2 = await createInvite(alice, "a-invite-2");
    const rec2 = (await read(inviteIdOf(link2))).slice();
    const mid = Math.floor(rec2.length / 2);
    rec2[mid] = (rec2[mid] ?? 0) ^ 0x01;
    expect(await put(inviteIdOf(link2), rec2)).toEqual({ result: "ok" });
    const carol = await newParty(world, "carol", d);
    await create([carol, "Carol"]);
    await carol.rt.dispatch({ kind: "AcceptInvitation", ctx: carol.ctx, operationId: op("c-accept"), link: linkNaming(link2, [`${d.url}/`]) });
    await settle(alice, carol);
    expect(await outcome(carol, "c-accept")).toEqual({ kind: "Rejected", reason: "integrity_failure" });
    expect(await conversations(carol)).toEqual([]);
    expect(await alice.rt.query({ kind: "GetIntroductions", ctx: alice.ctx })).toEqual([]);
  }, 120_000);

  it("a revoked invite stops working at the inviter's Home for someone who already holds the link", async () => {
    const world = new World(c, d);
    const alice = await newParty(world, "alice", c);
    const bob = await newParty(world, "bob", d);
    await create([alice, "Alice"], [bob, "Bob"]);
    const link = await createInvite(alice, "a-invite");
    await alice.rt.dispatch({ kind: "RevokeInvite", ctx: alice.ctx, operationId: op("a-revoke"), inviteId: "a-invite" });
    await settle(alice);
    expect(await outcome(alice, "a-revoke")).toEqual({ kind: "Committed" });
    expect((await c.fetch(`/v1/discovery/descriptors/${inviteIdOf(link)}`)).status).toBe(404);
    expect(world.by("alice", c).map((r) => `${r.method} ${r.path.replace(/[0-9a-f]{32}/g, "<id>")}`)).toContain("DELETE /v1/intro/tickets/<id>");
    await bob.rt.dispatch({ kind: "AcceptInvitation", ctx: bob.ctx, operationId: op("b-accept"), link });
    await settle(alice, bob);
    expect(await conversations(bob)).toEqual([]);
    expect(await alice.rt.query({ kind: "GetIntroductions", ctx: alice.ctx })).toEqual([]);
  }, 120_000);

  it("PC-A05: a capability is good at the Home that issued it and nowhere else", async () => {
    const world = new World(c, d);
    const alice = await newParty(world, "alice", c);
    const [route] = await alice.provider.allocateRoutes(1);
    if (!route) throw new Error("no route");
    await alice.provider.poll(route.mailbox, 0); // the owner's first poll claims the queue at her Home
    const deposit = (h: Home, cap: Uint8Array) => h.fetch("/v1/relay/deposit", { method: "POST", headers: { "content-type": "application/cbor" }, body: asBody(encodeDepositRequest({ requestId: rand(16), cap, envelope: sealed() })) });
    const cap = fromHex(route.capability);
    expect((await deposit(c, cap)).status).toBeLessThan(300);
    const elsewhere = await deposit(d, cap);
    expect(elsewhere.status).toBeGreaterThanOrEqual(400);
    expect(decodeHomeError(new Uint8Array(await elsewhere.arrayBuffer())).name).toBe("quota"); // exactly what a full or unknown queue answers
    expect(await alice.provider.poll(route.mailbox, 0)).toHaveLength(1); // only the deposit at her own Home arrived
    // Key fetch capabilities are per Home as well.
    const fetched = await d.fetch("/v1/keys/fetch", { method: "POST", headers: { "content-type": "application/cbor" }, body: asBody(encodeKeyFetch({ requestId: rand(16), cap, devices: [rand(16)] })) });
    expect(decodeHomeError(new Uint8Array(await fetched.arrayBuffer())).name).toBe("unauthorized");
  }, 60_000);

  it("V-020 drill: with one route per relationship, a blocked Home stalls only its owner's incoming mail", async () => {
    const world = new World(c, d);
    const alice = await newParty(world, "alice", c);
    const bob = await newParty(world, "bob", d);
    await create([alice, "Alice"], [bob, "Bob"]);
    const { aConv, bConv } = await introduce(alice, bob, await createInvite(alice, "a-invite"));

    world.down.add(d.url); // Bob's Home is blocked (a vendor-wide action, an outage)
    await say(bob, bConv, "m-bob", "bob, while his Home is blocked");
    await say(alice, aConv, "m-alice", "alice, to a blocked Home");
    await settle(alice, bob);
    // Alice still receives: Bob's deposit went to her route at her Home. Her own mail to Bob waits, typed, never rejected, never lost.
    expect(await timeline(alice, aConv)).toEqual(["out:alice, to a blocked Home", "in:bob, while his Home is blocked"]);
    expect(await outcome(alice, "m-alice")).toEqual({ kind: "PendingNetwork" });
    expect(await timeline(bob, bConv)).toEqual(["out:bob, while his Home is blocked"]); // Bob cannot reach his own mailbox either

    world.down.delete(d.url);
    await settle(alice, bob);
    expect(await outcome(alice, "m-alice")).toEqual({ kind: "Committed" });
    expect(await timeline(bob, bConv)).toEqual(["out:bob, while his Home is blocked", "in:alice, to a blocked Home"]); // exactly once
  }, 120_000);
});
