// SPDX-License-Identifier: MPL-2.0
// The sealed signalling envelope through the real engineering module: two parties connect through the loopback store, then
// one seals a WebRTC offer for the other and the other opens it. Needs `pnpm --filter @om/core-wasm build:wasm:scope`.
import { existsSync } from "node:fs";
import { describe, expect, it } from "vitest";
import { SCOPE_WASM, connectPair } from "./lan-signal-support";

describe.skipIf(!existsSync(SCOPE_WASM))("sealed LAN signalling through the real module", () => {
  it("an accepted pair signals in both directions; a replay is refused", async () => {
    const { alice, bob, aConv, bConv } = await connectPair();
    const sessionId = "0f".repeat(16);
    const sdp = "v=0\r\no=- 1 1 IN IP4 0.0.0.0\r\ns=-\r\n";
    const sealed = alice.engines.lanSignal.seal(alice.ctx, aConv, { kind: "offer", sessionId, sdp });
    if ("refused" in sealed) throw new Error(sealed.refused);
    expect(sealed.envelope).not.toContain(Buffer.from("IN IP4").toString("hex"));
    expect(bob.engines.lanSignal.open(bob.ctx, bConv, sealed.envelope)).toEqual({ kind: "offer", sessionId, sdp });
    expect(bob.engines.lanSignal.open(bob.ctx, bConv, sealed.envelope)).toEqual({ refused: "replay" });
    const answer = bob.engines.lanSignal.seal(bob.ctx, bConv, { kind: "answer", sessionId, sdp: "answer" });
    if ("refused" in answer) throw new Error(answer.refused);
    expect(alice.engines.lanSignal.open(alice.ctx, aConv, answer.envelope)).toMatchObject({ kind: "answer", sdp: "answer" });
    // Malformed input is a refusal, not a throw; a host-supplied seed has no field to travel in.
    expect(alice.engines.lanSignal.seal(alice.ctx, aConv, { kind: "offer", sessionId: "zz", sdp })).toEqual({ refused: "malformed" });
    expect(alice.engines.lanSignal.open(alice.ctx, aConv, "00")).toEqual({ refused: "malformed" });
  });
});
