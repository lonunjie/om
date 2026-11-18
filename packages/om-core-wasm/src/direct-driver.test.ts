// The direct-transfer host driver over the real Core (two wasm instances) and a loopback channel (D120: no product transport is
// built). Needs `pnpm --filter @om/core-wasm build:wasm`; skipped, loudly, only if the artifact is absent.
import { existsSync, readFileSync } from "node:fs";
import { join } from "node:path";
import { describe, expect, it } from "vitest";
import { asConversationId, asOperationId, asProfileId } from "@om/platform-api";
import type { ConversationTimelineItem, HomeSummary, MessageId, ProfileExecutionContext, ProfileId } from "@om/platform-api";
import { createCoreRuntime, instantiateCore } from "./index";
import type { DirectChannel, DirectCiphertextCache, DirectDriver, DirectFrame, DirectTransport, WasmCoreClient } from "./index";
import { createDirectDriver } from "./direct-driver";
import { createDirectFaults } from "./direct-loopback";
import type { DirectFaults } from "./direct-loopback";
import { createMemoryKv, createMemoryProvider } from "./test-support";
import type { MemoryKv, MemoryProvider } from "./test-support";

const WASM = join(import.meta.dirname, "../dist/om_wasm.wasm");
const present = existsSync(WASM);
if (!present) console.warn("om_wasm.wasm not built: run `make core-wasm`; skipping the direct-driver suite");

const HOME: HomeSummary = { tier: "T1", observers: ["host_sees_ciphertext"], migrationAvailable: true, clientOriginChoice: ["default_origin"] };
const op = asOperationId;
const BIG = 3000;
const DAY = 24 * 3600 * 1000;
const bytesOf = (n: number, seed = 1): Uint8Array => Uint8Array.from({ length: n }, (_, i) => (i * 31 + seed) & 255);

interface Party { rt: WasmCoreClient; kv: MemoryKv; ctx: ProfileExecutionContext; id: ProfileId }
const clock = { skew: 0 };

async function boot(kv: MemoryKv, provider: MemoryProvider, id: ProfileId): Promise<Party> {
  const core = await instantiateCore(readFileSync(WASM), { nowMs: () => Date.now() + clock.skew });
  const rt = await createCoreRuntime({ core, provider, storage: () => kv, home: HOME, autoDrain: false, retryMs: 0, recoveryTick: false });
  return { rt, kv, ctx: await rt.openProfile(id), id };
}
async function settle(...ps: Party[]): Promise<void> {
  for (let i = 0; i < 20; i++) for (const p of ps) await p.rt.flush(p.id);
  for (const p of ps) await p.rt.idle();
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
const lastItem = async (p: Party, conv: string): Promise<ConversationTimelineItem | undefined> => (await p.rt.query({ kind: "GetConversationView", ctx: p.ctx, conversationId: asConversationId(conv) })).timeline.at(-1);
const stateOf = async (p: Party, conv: string): Promise<string | undefined> => (await lastItem(p, conv))?.attachments?.[0]?.state;

interface Cache extends DirectCiphertextCache { readonly stored: Uint8Array[]; failNext: number; readonly at: number[] }
function makeCache(faults: DirectFaults): Cache {
  const c: Cache = {
    stored: [], failNext: 0, at: [],
    put(_p, _m, _i, ct) {
      if (c.failNext > 0) { c.failNext--; return Promise.reject(new Error("disk full")); }
      c.at.push(faults.log.length); // how many frames had crossed when the write became durable
      c.stored.push(ct.slice());
      return Promise.resolve();
    },
  };
  return c;
}

async function world(opts: { maxAttempts?: number; pick?: () => Promise<{ bytes: Uint8Array; meta: { mediaType: string; filename?: string } } | null> } = {}) {
  const provider = createMemoryProvider();
  provider.setBlobLimits({ maxBlobBytes: 1024 });
  const { alice, bob, aConv, bConv } = await connect(provider);
  const faults = createDirectFaults();
  const cache = makeCache(faults);
  const common = { retryMs: 0, idleMs: 60_000, maxAttempts: opts.maxAttempts ?? 5 };
  const aliceDriver = createDirectDriver({ core: alice.rt, transport: faults.transportFor("alice"), cache: makeCache(faults), ...(opts.pick === undefined ? {} : { pickFile: opts.pick }), ...common });
  const bobDriver = createDirectDriver({ core: bob.rt, transport: faults.transportFor("bob"), cache, ...common });
  aliceDriver.start(alice.ctx);
  const file = bytesOf(BIG, 5);
  const prep = await alice.rt.attachments.prepareDirect?.(alice.ctx, file, { mediaType: "video/mp4", filename: "clip.mp4" });
  if (!prep?.ok) throw new Error(JSON.stringify(prep));
  await alice.rt.dispatch({ kind: "SendAttachmentsDirect", ctx: alice.ctx, operationId: op("d1"), conversationId: asConversationId(aConv), draft: { text: "big one" }, attachments: [{ blobId: prep.blobId }] });
  await settle(alice, bob);
  const mine = (await lastItem(alice, aConv)) as { messageId: MessageId };
  const theirs = (await lastItem(bob, bConv)) as { messageId: MessageId };
  return { alice, bob, aConv, bConv, faults, cache, aliceDriver, bobDriver, file, blobId: prep.blobId, messageId: mine.messageId, bobMessageId: theirs.messageId, provider };
}
/** Time passes; Core notices the lapse on its next command (as in the Rust engine test), so give it one. */
async function lapse(w: { alice: Party; aConv: string }): Promise<void> {
  clock.skew = 8 * DAY;
  await w.alice.rt.dispatch({ kind: "SendMessage", ctx: w.alice.ctx, operationId: op(`lapse-${String(Math.random())}`), conversationId: asConversationId(w.aConv), draft: { text: "hi" } });
  await settle(w.alice);
}
const finish = (...d: DirectDriver[]): void => { for (const x of d) x.dispose(); };

describe.skipIf(!present)("direct-transfer host driver, real Core over a loopback channel", () => {
  it("pumps each transfer only on the channel that opened it", async () => {
    const w = await world();
    w.aliceDriver.dispose();
    const second = await w.alice.rt.attachments.prepareDirect?.(w.alice.ctx, bytesOf(BIG, 6), { mediaType: "video/mp4" });
    if (!second?.ok) throw new Error("second attachment unavailable");
    await w.alice.rt.dispatch({ kind: "SendAttachmentsDirect", ctx: w.alice.ctx, operationId: op("d2"), conversationId: asConversationId(w.aConv), draft: { text: "second file" }, attachments: [{ blobId: second.blobId }] });
    await settle(w.alice, w.bob);
    const secondMessage = (await lastItem(w.bob, w.bConv))?.messageId;
    if (secondMessage === undefined) throw new Error("second attachment absent");
    let accept: ((channel: DirectChannel) => void) | undefined;
    const driver = createDirectDriver({
      core: w.alice.rt, cache: w.cache,
      transport: { connect: () => Promise.resolve(null), listen: (_ctx, onChannel) => { accept = onChannel; return () => {}; } },
    });
    const open = (buffered: number) => {
      let onFrame: (frame: DirectFrame) => void = () => {};
      let onDrain: () => void = () => {};
      const frames: DirectFrame[] = [];
      accept?.({
        get buffered() { return buffered; },
        send(frame) { frames.push(frame); return Promise.resolve(); },
        onFrame(listener) { onFrame = listener; return () => {}; },
        onDrain(listener) { onDrain = listener; return () => {}; },
        onClose() { return () => {}; },
        close() {},
      });
      return { frames, receive: (frame: DirectFrame) => { onFrame(frame); }, drain() { buffered = 0; onDrain(); } };
    };
    const tick = () => new Promise((resolve) => setTimeout(resolve, 0));
    const request = (message: MessageId) => {
      const frame = w.bob.rt.direct.fetchStart(w.bob.ctx, message, 0);
      if (frame === null) throw new Error("no request");
      return frame;
    };
    try {
      driver.start(w.alice.ctx);
      const a = open(300_000);
      a.receive(request(w.bobMessageId));
      await tick();
      const first = a.frames.find((frame) => frame.kind === "begin");
      if (first === undefined) throw new Error("first transfer did not start");
      a.receive({ kind: "ack", transferId: first.transferId, offset: 0 });
      await tick();
      expect(a.frames.map((frame) => frame.kind)).toEqual(["begin"]);

      const b = open(0);
      b.receive(request(secondMessage));
      await tick();
      const other = b.frames.find((frame) => frame.kind === "begin");
      if (other === undefined) throw new Error("second transfer did not start");
      b.receive({ kind: "ack", transferId: other.transferId, offset: 0 });
      a.drain();
      await tick();
      expect(b.frames.every((frame) => "transferId" in frame && frame.transferId === other.transferId)).toBe(true);
      expect(a.frames.every((frame) => "transferId" in frame && frame.transferId === first.transferId)).toBe(true);
      expect(a.frames.some((frame) => frame.kind === "end")).toBe(true);
      expect(b.frames.some((frame) => frame.kind === "end")).toBe(true);
    } finally { finish(driver, w.bobDriver); }
  }, 60_000);

  it("full transfer: commit before the receipt, receipt before the file opens, equal bytes, only ciphertext cached, sender confirmed", async () => {
    const w = await world();
    expect(await stateOf(w.alice, w.aConv)).toBe("waiting_for_recipient");
    const r = await w.bobDriver.fetch(w.bob.ctx, w.bobMessageId, 0);
    if (!r.ok) throw new Error(r.reason);
    expect(Array.from(r.bytes)).toEqual(Array.from(w.file));
    expect(r.filename).toBe("clip.mp4");
    expect(r.receiptSent).toBe(true);
    // D109: the durable write happened before any receipt frame crossed.
    const receiptAt = w.faults.log.findIndex((l) => l.frame.kind === "receipt");
    expect(receiptAt).toBeGreaterThan(-1);
    expect(w.cache.at).toEqual([expect.any(Number)]);
    expect(w.cache.at[0]).toBeLessThanOrEqual(receiptAt);
    expect(w.cache.stored[0]?.length).toBe(BIG + 16);
    expect(Buffer.from(w.cache.stored[0] ?? []).includes(Buffer.from(w.file.slice(0, 64)))).toBe(false); // ciphertext, not the file
    await settle(w.alice, w.bob);
    expect(await stateOf(w.alice, w.aConv)).toBe("delivered_locally");
    finish(w.aliceDriver, w.bobDriver);
  }, 60_000);

  it("a failed durable write sends no receipt and does not open; fetching again commits, then confirms", async () => {
    const w = await world();
    w.cache.failNext = 1;
    expect(await w.bobDriver.fetch(w.bob.ctx, w.bobMessageId, 0)).toEqual({ ok: false, reason: "commit_failed" });
    expect(w.faults.log.some((l) => l.frame.kind === "receipt")).toBe(false);
    expect(await stateOf(w.alice, w.aConv)).toBe("peer_says_received"); // their ack is only a claim
    const again = await w.bobDriver.fetch(w.bob.ctx, w.bobMessageId, 0);
    if (!again.ok) throw new Error(again.reason);
    expect(Array.from(again.bytes)).toEqual(Array.from(w.file));
    await settle(w.alice, w.bob);
    expect(await stateOf(w.alice, w.aConv)).toBe("delivered_locally");
    finish(w.aliceDriver, w.bobDriver);
  }, 60_000);

  it("a channel cut mid-transfer resumes from the bytes held, not from zero", async () => {
    const w = await world();
    w.faults.cutAfter(2); // the begin and the one chunk reach the receiver, the end does not
    const r = await w.bobDriver.fetch(w.bob.ctx, w.bobMessageId, 0);
    if (!r.ok) throw new Error(r.reason);
    expect(Array.from(r.bytes)).toEqual(Array.from(w.file));
    expect(w.faults.opened()).toBeGreaterThanOrEqual(2);
    const begins = w.faults.log.map((l, i) => (l.frame.kind === "begin" ? i : -1)).filter((i) => i >= 0);
    expect(begins.length).toBeGreaterThanOrEqual(2);
    // the second begin is answered by an ack above zero: the held prefix was kept
    const answer = w.faults.log.slice((begins[1] ?? 0) + 1).find((l) => l.dir === "toSender" && l.frame.kind === "ack");
    expect((answer?.frame as { offset: number }).offset).toBeGreaterThan(0);
    const ackOffsets = w.faults.log.filter((l) => l.dir === "toSender" && l.frame.kind === "ack").map((l) => (l.frame as { offset: number }).offset);
    expect(Math.max(...ackOffsets)).toBe(BIG + 16);
    await settle(w.alice, w.bob);
    expect(await stateOf(w.alice, w.aConv)).toBe("delivered_locally");
    finish(w.aliceDriver, w.bobDriver);
  }, 60_000);

  it("retries are bounded: no route gives up after the attempts, a refusing sender is not retried", async () => {
    const w = await world({ maxAttempts: 3 });
    w.faults.setDown(true);
    expect(await w.bobDriver.fetch(w.bob.ctx, w.bobMessageId, 0)).toEqual({ ok: false, reason: "no_channel" });
    expect(w.faults.opened()).toBe(0);
    w.faults.setDown(false);
    await w.alice.rt.dispatch({ kind: "WithdrawDirectOffer", ctx: w.alice.ctx, operationId: op("w1"), messageId: w.messageId });
    await settle(w.alice, w.bob);
    expect(await w.bobDriver.fetch(w.bob.ctx, w.bobMessageId, 0)).toEqual({ ok: false, reason: "refused" });
    expect(w.faults.opened()).toBe(1); // one try, no retry of a refusal
    finish(w.aliceDriver, w.bobDriver);
  }, 60_000);

  it("a corrupted chunk fails closed: nothing cached, no receipt, no file", async () => {
    const w = await world();
    w.faults.corruptNextChunk();
    expect(await w.bobDriver.fetch(w.bob.ctx, w.bobMessageId, 0)).toEqual({ ok: false, reason: "failed" });
    expect(w.cache.stored).toEqual([]);
    expect(w.faults.log.some((l) => l.frame.kind === "receipt")).toBe(false);
    expect(await stateOf(w.alice, w.aConv)).not.toBe("delivered_locally");
    finish(w.aliceDriver, w.bobDriver);
  }, 60_000);

  it("a forged ack or receipt from a channel stays a claim; a lost receipt is recovered by fetching again", async () => {
    const w = await world();
    w.faults.loseNext("receipt", "toSender");
    const first = await w.bobDriver.fetch(w.bob.ctx, w.bobMessageId, 0);
    expect(first.ok).toBe(true);
    await settle(w.alice, w.bob);
    expect(await stateOf(w.alice, w.aConv)).toBe("peer_says_received"); // the receipt never arrived
    const tid = (w.faults.log.find((l) => l.frame.kind === "begin")?.frame as { transferId: string }).transferId;
    await w.faults.forge("bob", [
      { kind: "ack", transferId: tid, offset: BIG + 16 },
      { kind: "receipt", transferId: tid, tag: "00".repeat(32) },
      { kind: "receipt", transferId: "11".repeat(16), tag: "22".repeat(32) },
    ]);
    await settle(w.alice, w.bob);
    expect(await stateOf(w.alice, w.aConv)).toBe("peer_says_received");
    const second = await w.bobDriver.fetch(w.bob.ctx, w.bobMessageId, 0);
    if (!second.ok) throw new Error(second.reason);
    expect(Array.from(second.bytes)).toEqual(Array.from(w.file));
    await settle(w.alice, w.bob);
    expect(await stateOf(w.alice, w.aConv)).toBe("delivered_locally");
    finish(w.aliceDriver, w.bobDriver);
  }, 60_000);

  it("a sender restart serves nothing until the same file is handed back; then the fetch completes", async () => {
    const w = await world({ maxAttempts: 1 });
    w.faults.cutAfter(0);
    expect((await w.bobDriver.fetch(w.bob.ctx, w.bobMessageId, 0)).ok).toBe(false);
    finish(w.aliceDriver);
    const alice2 = await boot(w.alice.kv, createMemoryProvider(), w.alice.id);
    const d2 = createDirectDriver({ core: alice2.rt, transport: w.faults.transportFor("alice"), cache: makeCache(w.faults), retryMs: 0, maxAttempts: 1 });
    d2.start(alice2.ctx);
    expect(await w.bobDriver.fetch(w.bob.ctx, w.bobMessageId, 0)).toEqual({ ok: false, reason: "refused" }); // not armed: the uniform refusal
    expect(await d2.rearm(alice2.ctx, w.blobId, bytesOf(BIG, 6))).toBe(false); // a different file
    expect(await d2.rearm(alice2.ctx, w.blobId, w.file)).toBe(true);
    const r = await w.bobDriver.fetch(w.bob.ctx, w.bobMessageId, 0);
    if (!r.ok) throw new Error(r.reason);
    expect(Array.from(r.bytes)).toEqual(Array.from(w.file));
    await settle(alice2, w.bob);
    expect((await alice2.rt.query({ kind: "GetDirectOffers", ctx: alice2.ctx }))[0]?.state).toBe("delivered_locally");
    finish(d2, w.bobDriver);
  }, 60_000);

  it("a receiver restart starts again from zero and still confirms", async () => {
    const w = await world({ maxAttempts: 1 });
    w.faults.cutAfter(1);
    expect((await w.bobDriver.fetch(w.bob.ctx, w.bobMessageId, 0)).ok).toBe(false);
    finish(w.bobDriver);
    const bob2 = await boot(w.bob.kv, createMemoryProvider(), w.bob.id);
    const d2 = createDirectDriver({ core: bob2.rt, transport: w.faults.transportFor("bob"), cache: w.cache, retryMs: 0 });
    const [theirs] = (await bob2.rt.query({ kind: "GetConversationView", ctx: bob2.ctx, conversationId: asConversationId(w.bConv) })).timeline.slice(-1);
    const r = await d2.fetch(bob2.ctx, (theirs as { messageId: MessageId }).messageId, 0);
    if (!r.ok) throw new Error(r.reason);
    expect(Array.from(r.bytes)).toEqual(Array.from(w.file));
    await settle(w.alice, bob2);
    expect(await stateOf(w.alice, w.aConv)).toBe("delivered_locally");
    finish(w.aliceDriver, d2);
  }, 60_000);

  it("Send again: only for a lapsed unconfirmed offer; the file is chosen again and goes as a new direct message that then arrives", async () => {
    let picks = 0;
    const w = await world({ pick: () => { picks++; return Promise.resolve({ bytes: bytesOf(BIG, 5), meta: { mediaType: "video/mp4", filename: "clip.mp4" } }); } });
    // still live: nothing to send again
    expect(await w.aliceDriver.sendAgain(w.alice.ctx, w.messageId)).toEqual({ ok: false, reason: "not_resendable" });
    w.faults.loseNext("receipt", "toSender");
    expect((await w.bobDriver.fetch(w.bob.ctx, w.bobMessageId, 0)).ok).toBe(true);
    await settle(w.alice, w.bob);
    expect(await stateOf(w.alice, w.aConv)).toBe("peer_says_received");
    try {
      await lapse(w); // the offer lapses without a confirmation
      expect((await w.alice.rt.query({ kind: "GetDirectOffers", ctx: w.alice.ctx }))[0]?.state).toBe("not_confirmed");
      // the UI hook: ConversationServices.sendAgain
      w.aliceDriver.sendAgainFor(w.alice.ctx)(w.messageId, 0);
      for (let i = 0; i < 50 && picks === 0; i++) await new Promise((r) => { setTimeout(r, 10); });
      expect(picks).toBe(1);
      await settle(w.alice, w.bob);
      const [mine, theirs] = [await lastItem(w.alice, w.aConv), await lastItem(w.bob, w.bConv)];
      expect(mine?.messageId).not.toBe(w.messageId);
      expect(mine?.attachments?.[0]?.state).toBe("waiting_for_recipient");
      const r = await w.bobDriver.fetch(w.bob.ctx, (theirs as { messageId: MessageId }).messageId, 0);
      if (!r.ok) throw new Error(r.reason);
      expect(Array.from(r.bytes)).toEqual(Array.from(bytesOf(BIG, 5)));
      await settle(w.alice, w.bob);
      expect(await stateOf(w.alice, w.aConv)).toBe("delivered_locally");
    } finally { clock.skew = 0; }
    finish(w.aliceDriver, w.bobDriver);
  }, 60_000);

  it("Send again without a picker, or with the person cancelling, does nothing", async () => {
    const w = await world();
    expect(await w.aliceDriver.sendAgain(w.alice.ctx, w.messageId)).toEqual({ ok: false, reason: "no_picker" });
    finish(w.aliceDriver, w.bobDriver);
    const w2 = await world({ pick: () => Promise.resolve(null) });
    w2.faults.loseNext("receipt", "toSender");
    await w2.bobDriver.fetch(w2.bob.ctx, w2.bobMessageId, 0);
    try { await lapse(w2); expect(await w2.aliceDriver.sendAgain(w2.alice.ctx, w2.messageId)).toEqual({ ok: false, reason: "cancelled" }); } finally { clock.skew = 0; }
    finish(w2.aliceDriver, w2.bobDriver);
  }, 60_000);

  it("ticks: resume() pumps at once and a stopped profile holds no timer (D88 pattern)", async () => {
    const pending: { fn: () => void; ms: number }[] = [];
    const w = await world();
    finish(w.aliceDriver);
    const d = createDirectDriver({ core: w.alice.rt, transport: w.faults.transportFor("alice"), cache: makeCache(w.faults), retryMs: 0, tickMs: 5000, setTimer: (fn, ms) => { const h = { fn, ms }; pending.push(h); return h; }, clearTimer: (h) => { const i = pending.indexOf(h as never); if (i >= 0) pending.splice(i, 1); } });
    d.start(w.alice.ctx);
    const r = await w.bobDriver.fetch(w.bob.ctx, w.bobMessageId, 0);
    expect(r.ok).toBe(true);
    expect(pending.filter((p) => p.ms === 5000).length).toBe(1); // one tick timer while a channel is open
    d.resume(); // cancels the held-back timer and takes a reading now
    await new Promise((x) => { setTimeout(x, 0); });
    d.stop(w.alice.id);
    expect(pending.length).toBe(0);
    finish(d, w.bobDriver);
  }, 60_000);

  it("the runtime wires the driver: listening while a profile is open, stopped when it closes, and `sendAgainFor` fits ConversationServices", async () => {
    const faults = createDirectFaults();
    const base = faults.transportFor("solo");
    const events: string[] = [];
    const transport: DirectTransport = { connect: (c, t) => base.connect(c, t), listen: (c, h) => { events.push("listen"); const off = base.listen(c, h); return () => { events.push("stop"); off(); }; } };
    const core = await instantiateCore(readFileSync(WASM));
    const rt = await createCoreRuntime({ core, provider: createMemoryProvider(), storage: () => createMemoryKv(), home: HOME, autoDrain: false, retryMs: 0, recoveryTick: false, directDriver: { transport, cache: makeCache(faults) } });
    const ctx = await rt.openProfile(asProfileId("solo"));
    expect(events).toEqual(["listen"]);
    const services: { sendAgain?: ((m: MessageId, i: number) => void) | undefined } = { sendAgain: rt.directDriver?.sendAgainFor(ctx) };
    expect(typeof services.sendAgain).toBe("function");
    rt.closeProfile(asProfileId("solo"));
    expect(events).toEqual(["listen", "stop"]);
    rt.dispose();
  }, 60_000);
});
