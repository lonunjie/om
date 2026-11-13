// Runs the real Core (om-wasm) through the platform-api CoreClient: onboarding, invite and introduction, two-party
// messaging with ratcheting across restarts, commit-before-release and unknown network outcomes. Needs
// `pnpm --filter @om/core-wasm build:wasm` (make core-wasm); the suite is skipped, loudly, only if the artifact is absent.
import { existsSync, readFileSync } from "node:fs";
import { join } from "node:path";
import { describe, expect, it } from "vitest";
import { asConversationId, asOperationId, asProfileId } from "@om/platform-api";
import type { ConversationTimelineItem, HomeSummary, MessageId, OutcomeClass, ProfileExecutionContext, ProfileId } from "@om/platform-api";
import { createCoreRuntime, instantiateCore } from "./index";
import type { DirectFrame } from "./index";
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


const bytesOf = (n: number, seed = 1): Uint8Array => Uint8Array.from({ length: n }, (_, i) => (i * 31 + seed) & 255);
const lastItem = async (p: Party, conv: string): Promise<ConversationTimelineItem | undefined> => (await p.rt.query({ kind: "GetConversationView", ctx: p.ctx, conversationId: asConversationId(conv) })).timeline.at(-1);

describe.skipIf(!present)("attachments through the real Core and the loopback store", () => {
  it("a small file is sealed, uploaded, sent, fetched and opened; the store holds only ciphertext", async () => {
    const provider = createMemoryProvider();
    const { alice, bob, aConv, bConv } = await connect(provider);
    const file = bytesOf(5000);
    const prep = await alice.rt.attachments.prepare(alice.ctx, file, { mediaType: "application/pdf", filename: "plan.pdf" });
    if (!prep.ok) throw new Error(JSON.stringify(prep));
    await alice.rt.dispatch({ kind: "SendAttachments", ctx: alice.ctx, operationId: op("att1"), conversationId: asConversationId(aConv), draft: { text: "here" }, attachments: [{ blobId: prep.blobId }] });
    await settle(alice, bob);
    expect(await outcome(alice, "att1")).toEqual({ kind: "Committed" });
    const got = await lastItem(bob, bConv);
    expect(got?.direction).toBe("incoming");
    expect(got?.attachments?.[0]).toMatchObject({ index: 0, mediaType: "application/pdf", filename: "plan.pdf", size: 5000, state: "remote" });
    // Nothing is fetched until the person asks (ADR-0150, metadata chapter 7).
    expect(provider.blobs().size).toBe(1);
    const opened = await bob.rt.attachments.open(bob.ctx, (got as { messageId: MessageId }).messageId, 0);
    if (!opened.ok) throw new Error(JSON.stringify(opened));
    expect(Array.from(opened.bytes)).toEqual(Array.from(file));
    expect(opened.filename).toBe("plan.pdf");
    // The store saw ciphertext only: neither the bytes, the name nor the type.
    const stored = [...provider.blobs().values()][0] ?? new Uint8Array();
    expect(Buffer.from(stored).includes(Buffer.from(file.slice(0, 64)))).toBe(false);
    for (const needle of ["plan.pdf", "application/pdf"]) expect(Buffer.from(stored).includes(Buffer.from(needle))).toBe(false);
    // The sender sees its own copy as stored for later, separately from delivered.
    const mine = await lastItem(alice, aConv);
    expect(mine?.attachments?.[0]?.state).toBe("stored_for_later");
  });

  it("a file over the advertised limit is refused before sealing, in a typed way, and nothing is sent", async () => {
    const provider = createMemoryProvider();
    provider.setBlobLimits({ maxBlobBytes: 1024 });
    const { alice, bob, aConv } = await connect(provider);
    const r = await alice.rt.attachments.prepare(alice.ctx, bytesOf(4096), { mediaType: "application/octet-stream" });
    expect(r).toEqual({ ok: false, refusal: { error: "too_large", max: expect.any(Number) as number } });
    expect(provider.blobs().size).toBe(0);
    await settle(alice, bob);
    expect((await alice.rt.query({ kind: "GetConversationView", ctx: alice.ctx, conversationId: asConversationId(aConv) })).timeline).toHaveLength(0);
  });

  it("with no file store advertised, files are refused (never sent inline); limits are read from the provider", async () => {
    const provider = createMemoryProvider();
    provider.setBlobLimits(null);
    const { alice } = await connect(provider);
    expect(alice.rt.attachments.limits()).toBeNull();
    expect(await alice.rt.attachments.prepare(alice.ctx, bytesOf(10), { mediaType: "text/plain" })).toEqual({ ok: false, refusal: { error: "no_blob_store" } });
    provider.setBlobLimits({ maxBlobBytes: 2048 });
    await alice.rt.attachments.refreshLimits();
    expect(alice.rt.attachments.limits()).toEqual({ maxBlobBytes: 2048 });
  });

  it("a store that answers too_large after the fact rejects the message with that reason (not retryable)", async () => {
    const provider = createMemoryProvider();
    const { alice, bob, aConv } = await connect(provider);
    const prep = await alice.rt.attachments.prepare(alice.ctx, bytesOf(3000), { mediaType: "text/plain" });
    if (!prep.ok) throw new Error("prepare");
    provider.setBlobLimits({ maxBlobBytes: 100 }); // the store shrank its bound; Core still holds the old limits
    await alice.rt.dispatch({ kind: "SendAttachments", ctx: alice.ctx, operationId: op("att2"), conversationId: asConversationId(aConv), draft: { text: "" }, attachments: [{ blobId: prep.blobId }] });
    await settle(alice, bob);
    expect(await outcome(alice, "att2")).toEqual({ kind: "Rejected", reason: "too_large" });
  });

  it("a timeout after the store stored it is reconciled by an exact retry: one object, the message arrives once; staged bytes survive a restart", async () => {
    const provider = createMemoryProvider();
    const { alice, bob, aConv, bConv } = await connect(provider);
    const prep = await alice.rt.attachments.prepare(alice.ctx, bytesOf(2000, 9), { mediaType: "image/png", filename: "a.png" });
    if (!prep.ok) throw new Error("prepare");
    expect([...alice.kv.snapshot().keys()].some((k) => k.startsWith("stage/"))).toBe(true);
    await alice.rt.dispatch({ kind: "SendAttachments", ctx: alice.ctx, operationId: op("att3"), conversationId: asConversationId(aConv), draft: { text: "pic" }, attachments: [{ blobId: prep.blobId }] });
    provider.inject("timeout_after_effect");
    // Alice restarts before the retry: a new instance over the same durable store.
    const alice2 = await boot(alice.kv, provider, alice.id);
    await settle(alice2, bob);
    expect(await outcome(alice2, "att3")).toEqual({ kind: "Committed" });
    expect(provider.blobs().size).toBe(1);
    expect((await timeline(bob, bConv)).filter((t) => t === "in:pic")).toHaveLength(1);
    // Once stored, the staged copy is gone.
    expect([...alice2.kv.snapshot().keys()].some((k) => k.startsWith("stage/"))).toBe(false);
  });

  it("a flipped bit in the stored object never yields plaintext; an expired object is not found", async () => {
    const provider = createMemoryProvider();
    const { alice, bob, aConv, bConv } = await connect(provider);
    const prep = await alice.rt.attachments.prepare(alice.ctx, bytesOf(900), { mediaType: "text/plain", filename: "n.txt" });
    if (!prep.ok) throw new Error("prepare");
    await alice.rt.dispatch({ kind: "SendAttachments", ctx: alice.ctx, operationId: op("att4"), conversationId: asConversationId(aConv), draft: { text: "x" }, attachments: [{ blobId: prep.blobId }] });
    await settle(alice, bob);
    const item = await lastItem(bob, bConv);
    const id = (item as { messageId: MessageId }).messageId;
    provider.corruptBlobs();
    expect(await bob.rt.attachments.open(bob.ctx, id, 0)).toEqual({ ok: false, refusal: { error: "integrity_failure" } });
    provider.expireBlobs();
    expect(await bob.rt.attachments.open(bob.ctx, id, 0)).toEqual({ ok: false, refusal: { error: "not_found" } });
  });

  it("voice note and location card: Core re-derives the presentation, and refuses a voice note on a non-audio file", async () => {
    const provider = createMemoryProvider();
    const { alice, bob, aConv, bConv } = await connect(provider);
    const audio = await alice.rt.attachments.prepare(alice.ctx, bytesOf(3000, 3), { mediaType: "audio/webm", durationMs: 4200 });
    if (!audio.ok) throw new Error("prepare");
    await alice.rt.dispatch({ kind: "SendAttachments", ctx: alice.ctx, operationId: op("vn"), conversationId: asConversationId(aConv), draft: { text: "" }, attachments: [{ blobId: audio.blobId, voiceNote: {} }] });
    await alice.rt.dispatch({ kind: "SendLocation", ctx: alice.ctx, operationId: op("loc"), conversationId: asConversationId(aConv), latE7: 143_583_000, lonE7: 1_210_000_000, accuracyM: 25, label: "Home" });
    await settle(alice, bob);
    const view = await bob.rt.query({ kind: "GetConversationView", ctx: bob.ctx, conversationId: asConversationId(bConv) });
    const vn = view.timeline.find((t) => t.presentation?.voiceNote !== undefined);
    expect(vn?.attachments?.[0]).toMatchObject({ mediaType: "audio/webm", durationMs: 4200 });
    const loc = view.timeline.find((t) => t.presentation?.locationCard !== undefined);
    expect(loc?.presentation?.locationCard).toMatchObject({ latE7: 143_583_000, lonE7: 1_210_000_000, geo: expect.stringContaining("geo:") as string });
    expect(loc?.text).toContain("geo:14.3583000,121.0000000");
  });

  describe("direct transfer of a file too big for the delivery service", () => {
    const BIG = 3000; // over a 1 KiB spool bound: offered directly (the plan says so before anything is sealed)

    async function offered(): Promise<{ alice: Party; bob: Party; aConv: string; bConv: string; file: Uint8Array; blobId: string; messageId: MessageId; bobMessageId: MessageId; provider: MemoryProvider }> {
      const provider = createMemoryProvider();
      provider.setBlobLimits({ maxBlobBytes: 1024 });
      const { alice, bob, aConv, bConv } = await connect(provider);
      const file = bytesOf(BIG, 5);
      expect(await alice.rt.query({ kind: "PlanAttachment", ctx: alice.ctx, size: BIG })).toEqual({ kind: "offer_direct", limit: expect.any(Number) as number });
      expect(await alice.rt.attachments.prepare(alice.ctx, file, { mediaType: "video/mp4" })).toEqual({ ok: false, refusal: { error: "too_large", max: expect.any(Number) as number } });
      const prep = await alice.rt.attachments.prepareDirect?.(alice.ctx, file, { mediaType: "video/mp4", filename: "clip.mp4" });
      if (!prep?.ok) throw new Error(JSON.stringify(prep));
      await alice.rt.dispatch({ kind: "SendAttachmentsDirect", ctx: alice.ctx, operationId: op("d1"), conversationId: asConversationId(aConv), draft: { text: "big one" }, attachments: [{ blobId: prep.blobId }] });
      await settle(alice, bob);
      expect(await outcome(alice, "d1")).toEqual({ kind: "Committed" });
      expect(provider.blobs().size).toBe(0); // nothing was stored anywhere
      const mine = (await lastItem(alice, aConv)) as { messageId: MessageId };
      const theirs = (await lastItem(bob, bConv)) as { messageId: MessageId };
      return { alice, bob, aConv, bConv, file, blobId: prep.blobId, messageId: mine.messageId, bobMessageId: theirs.messageId, provider };
    }

    /** A loopback channel: frames move between the two Cores until neither has anything more to say. */
    async function transfer(alice: Party, bob: Party, messageId: MessageId, drop?: number): Promise<void> {
      const first = bob.rt.direct.fetchStart(bob.ctx, messageId, 0);
      if (first === null) throw new Error("no fetch");
      let toAlice: DirectFrame[] = [first];
      let chunks = 0;
      for (let round = 0; round < 400; round++) {
        const toBob: DirectFrame[] = [];
        for (const f of toAlice) toBob.push(...(await alice.rt.direct.serve(alice.ctx, f)));
        toBob.push(...alice.rt.direct.pump(alice.ctx, 0));
        toAlice = [];
        for (const f of toBob) {
          if (f.kind === "chunk" && drop !== undefined && ++chunks > drop) return;
          toAlice.push(...bob.rt.direct.fetchHandle(bob.ctx, messageId, 0, f));
        }
        if (toAlice.length === 0 && toBob.length === 0) return;
      }
    }

    it("the message goes at once and says so: waiting for them, then delivered; the file arrives equal and nothing is stored", async () => {
      const { alice, bob, aConv, bConv, file, bobMessageId, messageId, blobId } = await offered();
      expect((await lastItem(alice, aConv))?.attachments?.[0]?.state).toBe("waiting_for_recipient");
      expect((await lastItem(bob, bConv))?.attachments?.[0]?.state).toBe("remote_direct");
      expect(await alice.rt.query({ kind: "GetDirectOffers", ctx: alice.ctx })).toEqual([expect.objectContaining({ messageId, blobId, size: BIG, state: "waiting_for_recipient", needsFile: false })]);
      await transfer(alice, bob, bobMessageId);
      expect(bob.rt.direct.fetchProgress(bob.ctx, bobMessageId, 0)?.state).toBe("complete");
      const opened = bob.rt.direct.fetchOpen(bob.ctx, bobMessageId, 0);
      if (!opened.ok) throw new Error(JSON.stringify(opened));
      expect(Array.from(opened.bytes)).toEqual(Array.from(file));
      expect(opened.filename).toBe("clip.mp4");
      expect((await lastItem(alice, aConv))?.attachments?.[0]?.state).toBe("delivered_locally");
    });

    it("a dropped channel resumes from what the receiver holds; a restart shows the offer as needing its file, and only the same file re-arms it", async () => {
      const { alice, bob, file, blobId, bobMessageId } = await offered();
      await transfer(alice, bob, bobMessageId, 0); // everything after the handshake is lost
      const held = bob.rt.direct.fetchProgress(bob.ctx, bobMessageId, 0);
      expect(held?.state).not.toBe("complete");
      const alice2 = await boot(alice.kv, createMemoryProvider(), alice.id);
      const [offer] = await alice2.rt.query({ kind: "GetDirectOffers", ctx: alice2.ctx });
      expect(offer).toMatchObject({ needsFile: true, state: "waiting_for_recipient" });
      expect(await alice2.rt.attachments.arm(alice2.ctx, blobId, bytesOf(BIG, 6))).toBe(false);
      expect(await alice2.rt.attachments.arm(alice2.ctx, blobId, file)).toBe(true);
      expect((await alice2.rt.query({ kind: "GetDirectOffers", ctx: alice2.ctx }))[0]?.needsFile).toBe(false);
    });

    it("taking the offer back serves nothing and says so; the message stays", async () => {
      const { alice, bob, aConv, bConv, messageId, bobMessageId } = await offered();
      await alice.rt.dispatch({ kind: "WithdrawDirectOffer", ctx: alice.ctx, operationId: op("w1"), messageId });
      await settle(alice, bob);
      expect(await outcome(alice, "w1")).toEqual({ kind: "Committed" });
      expect((await lastItem(alice, aConv))?.attachments?.[0]?.state).toBe("not_offered");
      await transfer(alice, bob, bobMessageId);
      expect(bob.rt.direct.fetchProgress(bob.ctx, bobMessageId, 0)?.state).not.toBe("complete");
      expect((await lastItem(bob, bConv))?.text).toBe("big one");
    });
  });
});
