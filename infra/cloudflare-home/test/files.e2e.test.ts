// SPDX-License-Identifier: AGPL-3.0-or-later
// Files end to end: two real Core profiles send and open an attachment, a voice note and a location card through the real
// reference Home (Worker bundle in workerd) with the HTTP adapter, using the Home's advertised spool limits. Needs
// `make core-wasm` for dist/om_wasm.wasm (skipped, loudly, if absent).
import { existsSync, readFileSync } from "node:fs";
import { join } from "node:path";
import { afterAll, beforeAll, describe, expect, it } from "vitest";
import { asConversationId, asOperationId, asProfileId } from "@om/platform-api";
import type { HomeSummary, ProfileExecutionContext, ProfileId } from "@om/platform-api";
import { createCoreRuntime, instantiateCore } from "@om/core-wasm";
import type { WasmCoreClient } from "@om/core-wasm";
import { createMemoryKv, type MemoryKv } from "@om/core-wasm/test-support";
import { admissionHash } from "@om/home-wire";
import { createHttpProvider, createMemorySecretStore, parseProviderUrl, type HttpProvider } from "@om/provider-http";
import { startHome, type Home } from "./harness";

const WASM = join(import.meta.dirname, "../../../packages/om-core-wasm/dist/om_wasm.wasm");
const present = existsSync(WASM);
if (!present) console.warn("om_wasm.wasm not built: run `make core-wasm`; skipping the files e2e");

const HOME: HomeSummary = { tier: "T1", observers: ["host_sees_ciphertext"], migrationAvailable: true, clientOriginChoice: ["default_origin"] };
const op = asOperationId;
const MIB = 1024 * 1024;
const secret = crypto.getRandomValues(new Uint8Array(32));

interface Party { rt: WasmCoreClient; ctx: ProfileExecutionContext; id: ProfileId; seen: string[] }

let home: Home;
beforeAll(async () => { if (present) home = await startHome({ vars: { OM_ADMISSION: await admissionHash(secret) } }); }, 120_000);
afterAll(async () => { if (present) await home.dispose(); });

async function boot(id: ProfileId): Promise<Party> {
  const config = parseProviderUrl(home.url, { allowLoopbackHttp: true });
  if (!config) throw new Error("bad url");
  const seen: string[] = [];
  const provider: HttpProvider = createHttpProvider({ config, secrets: createMemorySecretStore(), admission: secret, allowLoopbackHttp: true, timeoutMs: 20_000, fetch: (i, init) => { seen.push(`${init?.method ?? "GET"} ${typeof i === "string" ? i : i instanceof URL ? i.href : i.url}`); return fetch(i, init); } });
  const rt = await createCoreRuntime({ core: await instantiateCore(readFileSync(WASM)), provider, storage: () => createMemoryKvOnce(id), home: HOME, autoDrain: false, retryMs: 0 });
  return { rt, ctx: await rt.openProfile(id), id, seen };
}
const kvs = new Map<string, MemoryKv>();
const createMemoryKvOnce = (id: ProfileId): MemoryKv => { const k = kvs.get(id) ?? createMemoryKv(); kvs.set(id, k); return k; };

async function settle(...ps: Party[]): Promise<void> {
  for (let i = 0; i < 8; i++) for (const p of ps) await p.rt.flush(p.id);
  for (const p of ps) await p.rt.idle();
}
const outcome = async (p: Party, id: string) => (await p.rt.pendingOperations(p.ctx)).find((o) => o.operationId === id)?.outcome;
const rand = (n: number): Uint8Array => { const b = new Uint8Array(n); for (let i = 0; i < n; i += 65536) crypto.getRandomValues(b.subarray(i, Math.min(n, i + 65536))); return b; };

let runs = 0;
async function connect(): Promise<{ alice: Party; bob: Party; aConv: string; bConv: string }> {
  const n = (runs += 1); // a fresh pair of installs per case: separate stores and ids
  const alice = await boot(asProfileId(`alice${String(n)}`));
  const bob = await boot(asProfileId(`bob${String(n)}`));
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

describe.skipIf(!present)("files through the reference Home", () => {
  it("reads the advertised limits, sends a file, a voice note and a location card, the recipient opens them, and the Home holds only ciphertext", async () => {
    const { alice, bob, aConv, bConv } = await connect();
    await alice.rt.attachments.refreshLimits();
    expect(alice.rt.attachments.limits()).toEqual({ maxBlobBytes: 4 * MIB, maxSpoolBytes: 32 * MIB, maxBlobs: 64 });

    const file = rand(20_000);
    const f = await alice.rt.attachments.prepare(alice.ctx, file, { mediaType: "application/pdf", filename: "plan.pdf" });
    const v = await alice.rt.attachments.prepare(alice.ctx, rand(6_000), { mediaType: "audio/webm", durationMs: 3100 });
    if (!f.ok || !v.ok) throw new Error("prepare");
    await alice.rt.dispatch({ kind: "SendAttachments", ctx: alice.ctx, operationId: op("file"), conversationId: asConversationId(aConv), draft: { text: "the plan" }, attachments: [{ blobId: f.blobId }] });
    await alice.rt.dispatch({ kind: "SendAttachments", ctx: alice.ctx, operationId: op("voice"), conversationId: asConversationId(aConv), draft: { text: "" }, attachments: [{ blobId: v.blobId, voiceNote: {} }] });
    await alice.rt.dispatch({ kind: "SendLocation", ctx: alice.ctx, operationId: op("loc"), conversationId: asConversationId(aConv), latE7: 143_583_000, lonE7: 1_210_000_000, accuracyM: 20 });
    await settle(alice, bob);
    for (const id of ["file", "voice", "loc"]) expect(await outcome(alice, id), id).toEqual({ kind: "Committed" });

    const view = await bob.rt.query({ kind: "GetConversationView", ctx: bob.ctx, conversationId: asConversationId(bConv) });
    const withFile = view.timeline.find((t) => t.attachments?.[0]?.filename === "plan.pdf");
    const withVoice = view.timeline.find((t) => t.presentation?.voiceNote !== undefined);
    expect(view.timeline.some((t) => t.presentation?.locationCard !== undefined)).toBe(true);
    if (!withFile || !withVoice) throw new Error("missing items");
    const before = bob.seen.length;
    expect(bob.seen.slice(before)).toEqual([]); // nothing is fetched until asked
    const opened = await bob.rt.attachments.open(bob.ctx, withFile.messageId, 0);
    if (!opened.ok) throw new Error(JSON.stringify(opened));
    expect(Array.from(opened.bytes)).toEqual(Array.from(file));
    expect(bob.seen.slice(before).filter((l) => l.startsWith("GET ") && l.includes("/v1/blobs/"))).toHaveLength(1);
    // The recipient reads with the read capability only: no admission, no capability in any URL.
    for (const l of [...alice.seen, ...bob.seen]) expect(l).not.toMatch(/[?#]/);
    const voice = await bob.rt.attachments.open(bob.ctx, withVoice.messageId, 0);
    expect(voice.ok).toBe(true);
  }, 180_000);

  it("a file over the Home's bound is refused in a typed way before anything is uploaded", async () => {
    const { alice, bob, aConv } = await connect();
    await alice.rt.attachments.refreshLimits();
    const before = alice.seen.length;
    const r = await alice.rt.attachments.prepare(alice.ctx, rand(5 * MIB), { mediaType: "video/mp4", filename: "big.mp4" });
    expect(r).toEqual({ ok: false, refusal: { error: "too_large", max: expect.any(Number) as number } });
    await settle(alice, bob);
    expect(alice.seen.slice(before).filter((l) => l.includes("/v1/blobs"))).toEqual([]);
    expect((await alice.rt.query({ kind: "GetConversationView", ctx: alice.ctx, conversationId: asConversationId(aConv) })).timeline).toHaveLength(0);
  }, 120_000);
});
