// SPDX-License-Identifier: MPL-2.0
// The sealed signalling session over two real Cores and fake peers: the SDP only ever crosses the signaller sealed, a third
// party's envelope or a wrong session ends the attempt, and a failed attempt closes its peer. Needs `build:wasm:scope`.
import { existsSync } from "node:fs";
import { describe, expect, it } from "vitest";
import { LanSessionError, lanInitiate, lanRespond } from "./lan-session";
import type { SessionPeer, Signaller } from "./lan-session";
import { SCOPE_WASM, connectPair } from "./test-support-lan-signal";

function signallers(): { a: Signaller; b: Signaller; wire: string[] } {
  const wire: string[] = [];
  const box: Record<"a" | "b", { q: string[]; w: ((s: string) => void) | undefined }> = { a: { q: [], w: undefined }, b: { q: [], w: undefined } };
  const mk = (me: "a" | "b", other: "a" | "b"): Signaller => ({
    send: (s) => { wire.push(s); const t = box[other]; if (t.w) { const w = t.w; t.w = undefined; w(s); } else t.q.push(s); return Promise.resolve(); },
    receive: () => new Promise((res) => { const m = box[me]; const next = m.q.shift(); if (next !== undefined) res(next); else m.w = res; }),
  });
  return { a: mk("a", "b"), b: mk("b", "a"), wire };
}

class FakePeer implements SessionPeer<string> {
  closed = false; remote = "";
  constructor(readonly name: string) {}
  offer() { return Promise.resolve(`v=0 offer from ${this.name}`); }
  accept(o: string) { this.remote = o; return Promise.resolve(`v=0 answer from ${this.name}`); }
  finish(a: string) { this.remote = a; return Promise.resolve(); }
  opened() { return Promise.resolve(`channel:${this.name}:${this.remote}`); }
  close() { this.closed = true; }
}

describe.skipIf(!existsSync(SCOPE_WASM))("sealed LAN signalling session", () => {
  it("connects two accepted people and the SDP never crosses the signaller in clear", async () => {
    const { alice, bob, aConv, bConv } = await connectPair();
    const s = signallers();
    const [pa, pb] = [new FakePeer("alice"), new FakePeer("bob")];
    const [a, b] = await Promise.all([
      lanInitiate({ lanSignal: alice.engines.lanSignal, ctx: alice.ctx, conversationId: aConv, peer: pa, signaller: s.a, sessionId: "11".repeat(16) }),
      lanRespond({ lanSignal: bob.engines.lanSignal, ctx: bob.ctx, conversationId: bConv, peer: pb, signaller: s.b }),
    ]);
    expect(a).toBe("channel:alice:v=0 answer from bob");
    expect(b).toBe("channel:bob:v=0 offer from alice");
    expect(s.wire).toHaveLength(2);
    for (const w of s.wire) expect(Buffer.from(w, "hex").toString("latin1")).not.toContain("v=0");
  });

  it("a profile with no relationship with the sender cannot open the envelope: its peer is closed and no channel opens", async () => {
    const { alice, carol, aConv } = await connectPair();
    const s = signallers();
    const pc = new FakePeer("carol");
    const cConv = (await carol.rt.query({ kind: "GetConversationList", ctx: carol.ctx }))[0]?.id ?? "none";
    const sealed = alice.engines.lanSignal.seal(alice.ctx, aConv, { kind: "offer", sessionId: "22".repeat(16), sdp: "x" });
    if ("refused" in sealed) throw new Error(sealed.refused);
    void s.a.send(sealed.envelope);
    // Carol has no relationship with Alice: whatever conversation she names, the envelope does not open.
    await expect(lanRespond({ lanSignal: carol.engines.lanSignal, ctx: carol.ctx, conversationId: cConv, peer: pc, signaller: s.b })).rejects.toBeInstanceOf(Error);
    expect(pc.closed).toBe(true);
  });

  it("refuses an answer to a different session", async () => {
    const { alice, bob, aConv, bConv } = await connectPair();
    const s = signallers();
    const [pa, pb] = [new FakePeer("alice"), new FakePeer("bob")];
    const initiator = lanInitiate({ lanSignal: alice.engines.lanSignal, ctx: alice.ctx, conversationId: aConv, peer: pa, signaller: s.a, sessionId: "33".repeat(16) });
    // Bob answers under another session id than the offer he received.
    const offer = bob.engines.lanSignal.open(bob.ctx, bConv, await s.b.receive());
    if ("refused" in offer) throw new Error(offer.refused);
    const wrong = bob.engines.lanSignal.seal(bob.ctx, bConv, { kind: "answer", sessionId: "44".repeat(16), sdp: "a" });
    if ("refused" in wrong) throw new Error(wrong.refused);
    await s.b.send(wrong.envelope);
    await expect(initiator).rejects.toMatchObject({ reason: "wrong_session" } satisfies Partial<LanSessionError>);
    expect(pa.closed).toBe(true);
    void pb;
  });
});
