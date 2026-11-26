// SPDX-License-Identifier: MPL-2.0
import { describe, expect, it, vi } from "vitest";
import { HIGH_WATER, LAN_CHANNEL_LABEL, LOW_WATER, LanChannelError, createLanPeer, lanChannelCapability, probeLanChannel } from "./channel";

class FakeChannel extends EventTarget {
  binaryType = "blob";
  bufferedAmountLowThreshold = 0;
  bufferedAmount = 0;
  readyState = "open";
  ordered = true;
  maxRetransmits: number | null = null;
  maxPacketLifeTime: number | null = null;
  sent: Uint8Array[] = [];
  constructor(readonly label: string) { super(); }
  send(b: Uint8Array) { this.sent.push(b); }
  close() { this.readyState = "closed"; this.dispatchEvent(new Event("close")); }
}

class FakeConnection extends EventTarget {
  iceGatheringState = "complete";
  connectionState = "new";
  localDescription: RTCSessionDescriptionInit | undefined;
  channel: FakeChannel | undefined;
  close = vi.fn();
  createDataChannel = vi.fn((label: string, opts: RTCDataChannelInit) => {
    expect(opts.ordered).toBe(true);
    this.channel = new FakeChannel(label);
    return this.channel;
  });
  createOffer = vi.fn(() => Promise.resolve({ type: "offer", sdp: "offer-sdp" } satisfies RTCSessionDescriptionInit));
  createAnswer = vi.fn(() => Promise.resolve({ type: "answer", sdp: "answer-sdp" } satisfies RTCSessionDescriptionInit));
  setLocalDescription = vi.fn((d: RTCSessionDescriptionInit) => { this.localDescription = d; return Promise.resolve(); });
  setRemoteDescription = vi.fn((_d: RTCSessionDescriptionInit) => Promise.resolve());
}

const env = (made: FakeConnection[]) => ({ peer: (config: RTCConfiguration) => {
  expect(config.iceServers).toEqual([]);
  const c = new FakeConnection();
  made.push(c);
  return c as unknown as RTCPeerConnection;
} });

describe("om-lan-v1 channel", () => {
  it("reports unavailable without a peer connection and never builds a weaker channel", () => {
    expect(lanChannelCapability({})).toBe("unavailable");
    expect(() => createLanPeer({})).toThrow(LanChannelError);
    expect(lanChannelCapability(env([]))).toBe("available");
  });

  it("offers on an ordered om-lan-v1 channel with no ICE servers and returns the SDP as text", async () => {
    const made: FakeConnection[] = [];
    const peer = createLanPeer(env(made));
    expect(await peer.offer()).toBe("offer-sdp");
    expect(made[0]?.channel?.label).toBe(LAN_CHANNEL_LABEL);
    await peer.finish("answer-sdp");
    const ch = await peer.opened();
    ch.send(new Uint8Array([1, 2]));
    expect(made[0]?.channel?.sent).toHaveLength(1);
  });

  it("applies the profile's backpressure marks and reports drain", async () => {
    const made: FakeConnection[] = [];
    const peer = createLanPeer(env(made));
    await peer.offer();
    const ch = await peer.opened();
    const raw = made[0]?.channel;
    expect(raw?.bufferedAmountLowThreshold).toBe(LOW_WATER);
    const drained = vi.fn();
    ch.onDrain(drained);
    if (raw) raw.bufferedAmount = HIGH_WATER + 1;
    expect(ch.canSend()).toBe(false);
    if (raw) raw.bufferedAmount = LOW_WATER;
    raw?.dispatchEvent(new Event("bufferedamountlow"));
    expect(drained).toHaveBeenCalledTimes(1);
    expect(ch.canSend()).toBe(true);
  });

  it("accepts an offer, ignores channels with another label and refuses an oversize SDP", async () => {
    const made: FakeConnection[] = [];
    const peer = createLanPeer(env(made));
    expect(await peer.accept("offer-sdp")).toBe("answer-sdp");
    const other = new FakeChannel("something-else");
    const e = Object.assign(new Event("datachannel"), { channel: other });
    made[0]?.dispatchEvent(e);
    const ours = new FakeChannel(LAN_CHANNEL_LABEL);
    made[0]?.dispatchEvent(Object.assign(new Event("datachannel"), { channel: ours }));
    const ch = await peer.opened();
    const got = vi.fn();
    ch.onMessage(got);
    ours.dispatchEvent(Object.assign(new Event("message"), { data: new Uint8Array([9]).buffer }));
    ours.dispatchEvent(Object.assign(new Event("message"), { data: "text is not a frame" }));
    expect(got).toHaveBeenCalledTimes(1);
    await expect(createLanPeer(env([])).accept("x".repeat(12_289))).rejects.toMatchObject({ reason: "signal" });
  });

  it("rejects when the connection fails before the channel opens", async () => {
    const made: FakeConnection[] = [];
    const peer = createLanPeer(env(made));
    await peer.offer();
    const raw = made[0];
    if (!raw?.channel) throw new Error("no channel");
    // Not yet open: the channel was created but the link never came up.
    raw.connectionState = "failed";
    raw.dispatchEvent(new Event("connectionstatechange"));
    // The fake channel reports open immediately, so the open promise settled first; a fresh peer proves the failure path.
    const made2: FakeConnection[] = [];
    const peer2 = createLanPeer({ peer: (c) => { const f = new FakeConnection(); made2.push(f); void c; return f as unknown as RTCPeerConnection; } });
    await peer2.accept("offer-sdp");
    made2[0]!.connectionState = "failed";
    made2[0]!.dispatchEvent(new Event("connectionstatechange"));
    await expect(peer2.opened()).rejects.toMatchObject({ reason: "connection" });
  });

  it("the capability row comes from a real establish attempt, not from the constructor existing", async () => {
    expect(await probeLanChannel({})).toBe("unavailable");
    // A runtime whose connections construct but never open reports unavailable.
    class Stuck extends FakeConnection {
      override createDataChannel = vi.fn((label: string) => { const c = new FakeChannel(label); c.readyState = "connecting"; return c; });
    }
    vi.useFakeTimers();
    const p = probeLanChannel({ peer: () => new Stuck() as unknown as RTCPeerConnection });
    await vi.advanceTimersByTimeAsync(9000);
    expect(await p).toBe("unavailable");
    vi.useRealTimers();
    // One whose channels open on both sides reports available and leaves nothing open.
    const made: FakeConnection[] = [];
    class Delivers extends FakeConnection {
      override setRemoteDescription = vi.fn((d: RTCSessionDescriptionInit) => {
        if (d.type === "offer") queueMicrotask(() => { this.dispatchEvent(Object.assign(new Event("datachannel"), { channel: new FakeChannel(LAN_CHANNEL_LABEL) })); });
        return Promise.resolve();
      });
    }
    const peer = (): RTCPeerConnection => { const c = new Delivers(); made.push(c); return c as unknown as RTCPeerConnection; };
    expect(await probeLanChannel({ peer })).toBe("available");
    expect(made).toHaveLength(2);
    for (const c of made) expect(c.close).toHaveBeenCalled();
  });

  it("a runtime whose peer connection throws on construction reports unavailable instead of rejecting", async () => {
    expect(await probeLanChannel({ peer: () => { throw new Error("SecurityError"); } })).toBe("unavailable");
  });

  const opened = async (): Promise<{ raw: FakeConnection; ch: FakeChannel; peer: ReturnType<typeof createLanPeer> }> => {
    const made: FakeConnection[] = [];
    const peer = createLanPeer(env(made));
    await peer.offer();
    const raw = made[0];
    if (!raw?.channel) throw new Error("no channel");
    return { raw, ch: raw.channel, peer };
  };

  it("refuses an answerer-side channel that is not ordered and fully reliable", async () => {
    for (const shape of [{ ordered: false }, { maxRetransmits: 0 }, { maxPacketLifeTime: 100 }]) {
      const made: FakeConnection[] = [];
      const peer = createLanPeer(env(made));
      await peer.accept("offer-sdp");
      const bad = Object.assign(new FakeChannel(LAN_CHANNEL_LABEL), shape);
      made[0]!.dispatchEvent(Object.assign(new Event("datachannel"), { channel: bad }));
      await expect(peer.opened()).rejects.toMatchObject({ reason: "connection" });
      expect(made[0]!.close).toHaveBeenCalled();
    }
  });

  it("send stays paused above the high mark until the buffer is at or below the low mark", async () => {
    const { ch, peer } = await opened();
    const dc = await peer.opened();
    expect(dc.canSend()).toBe(true);
    ch.bufferedAmount = HIGH_WATER + 1;
    dc.send(new Uint8Array(1));
    expect(dc.canSend()).toBe(false);
    ch.bufferedAmount = HIGH_WATER - 1; // below the high mark but not yet at the low mark
    expect(dc.canSend()).toBe(false);
    ch.bufferedAmount = LOW_WATER;
    expect(dc.canSend()).toBe(true);
  });

  it("keeps frames that arrive before the consumer attaches", async () => {
    const { ch, peer } = await opened();
    const dc = await peer.opened();
    const buf = new Uint8Array([1, 2]).buffer;
    ch.dispatchEvent(Object.assign(new Event("message"), { data: buf }));
    const got: number[][] = [];
    dc.onMessage((b) => got.push([...b]));
    ch.dispatchEvent(Object.assign(new Event("message"), { data: new Uint8Array([3]).buffer }));
    expect(got).toEqual([[1, 2], [3]]);
  });

  it("closing the channel tears down the peer connection, and a peer is single use", async () => {
    const { raw, peer } = await opened();
    await expect(peer.offer()).rejects.toMatchObject({ reason: "signal" });
    await expect(peer.accept("offer-sdp")).rejects.toMatchObject({ reason: "signal" });
    (await peer.opened()).close();
    expect(raw.close).toHaveBeenCalled();
  });
});
