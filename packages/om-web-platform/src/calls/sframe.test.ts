import { assert, describe, expect, it, vi } from "vitest";
import { decodeHeader, encodeHeader, SFrameReceiver, SFrameSender } from "./sframe";

const bytes = (hex: string) => Uint8Array.from(hex.match(/../g) ?? [], (byte) => Number.parseInt(byte, 16));
const plain = bytes("64726166742d696574662d736672616d652d656e63");
const key = (value = 1) => new Uint8Array(32).fill(value);

describe("RFC 9605 SFrame", () => {
  it.each([
    [0n, 0n, "00"],
    [0x123n, 0x4567n, "9901234567"],
    [1n, 0xffn, "18ff"],
    [0x100000000n, 0x100000000000000n, "cf01000000000100000000000000"],
    [0xffffffffffffffffn, 0xffffffffffffffffn, "ffffffffffffffffffffffffffffffffff"],
  ])("encodes and decodes KID %s and CTR %s", (kid, counter, hex) => {
    const expected = bytes(hex);
    expect(encodeHeader(kid, counter)).toEqual(expected);
    expect(decodeHeader(expected)).toEqual({ kid, counter, length: expected.length });
  });

  it("matches Appendix C.3 for suite 0x0004 in both directions", async () => {
    const base = "000102030405060708090a0b0c0d0e0f";
    const metadata = bytes("4945544620534672616d65205747");
    const ciphertext = bytes("9901234567b7412c2513a1b66dbb48841bbaf17f598751176ad847681a69c6d0b091c07018ce4adb34eb");
    const sender = new SFrameSender();
    const receiver = new SFrameReceiver();
    const sendBytes = bytes(base);
    const receiveBytes = bytes(base);
    await Promise.all([sender.install(0x123n, sendBytes), receiver.install(0x123n, receiveBytes)]);
    expect(sendBytes).toEqual(new Uint8Array(16));
    expect(receiveBytes).toEqual(new Uint8Array(16));
    expect(await sender.seal(plain, metadata, 0x4567n)).toEqual(ciphertext);
    expect(await receiver.open(ciphertext)).toBeUndefined();
    expect(await receiver.open(ciphertext, metadata)).toEqual(plain.buffer);
    expect(await receiver.open(ciphertext, metadata)).toBeUndefined();
  });

  it("rejects truncated and noncanonical headers", () => {
    for (const malformed of ["", "90ff", "8007", "9000ff", "0800"]) {
      expect(() => decodeHeader(bytes(malformed))).toThrow();
    }
  });

  it("allocates counters across concurrent tracks and refuses reuse and wrap", async () => {
    const sender = new SFrameSender();
    await sender.install(0n, key());
    const [audio, video] = await Promise.all([sender.seal(plain), sender.seal(plain)]);
    assert(audio !== undefined && video !== undefined, "both tracks must produce a frame");
    expect(decodeHeader(audio).counter).toBe(0n);
    expect(decodeHeader(video).counter).toBe(1n);
    await expect(sender.seal(plain, undefined, 0n)).rejects.toThrow();
    await sender.seal(plain, undefined, 0xffffffffffffffffn);
    await expect(sender.seal(plain)).rejects.toThrow();
  });

  it("drops wrong key IDs, wrong directional keys, tampering and simultaneous replay", async () => {
    const sender = new SFrameSender();
    const receiver = new SFrameReceiver();
    const wrongDirection = new SFrameReceiver();
    await Promise.all([sender.install(0n, key()), receiver.install(0n, key()), wrongDirection.install(0n, key(2))]);
    const protectedFrame = await sender.seal(plain);
    assert(protectedFrame !== undefined);
    const badId = protectedFrame.slice();
    badId[0] = 0x10;
    const badTag = protectedFrame.slice();
    const finalByte = badTag.at(-1);
    assert(finalByte !== undefined);
    badTag[badTag.length - 1] = finalByte ^ 1;
    expect(await receiver.open(badId)).toBeUndefined();
    expect(await receiver.open(badTag)).toBeUndefined();
    expect(await wrongDirection.open(protectedFrame)).toBeUndefined();
    const results = await Promise.all([receiver.open(protectedFrame), receiver.open(protectedFrame)]);
    expect(results.filter(Boolean)).toEqual([plain.buffer]);
  });

  it("accepts reordered counters within the window and drops older ones", async () => {
    const sender = new SFrameSender();
    const receiver = new SFrameReceiver();
    await Promise.all([sender.install(0n, key()), receiver.install(0n, key())]);
    const stale = await sender.seal(plain);
    const reordered = await sender.seal(plain);
    const newest = await sender.seal(plain, undefined, 128n);
    assert(stale !== undefined && reordered !== undefined && newest !== undefined);
    expect(await receiver.open(newest)).toEqual(plain.buffer);
    expect(await receiver.open(stale)).toBeUndefined();
    expect(await receiver.open(reordered)).toEqual(plain.buffer);
  });

  it("expires the previous epoch and refuses rollback", async () => {
    const sender = new SFrameSender();
    const receiver = new SFrameReceiver();
    await Promise.all([sender.install(0n, key()), receiver.install(0n, key())]);
    const first = await sender.seal(plain);
    const late = await sender.seal(plain);
    assert(first !== undefined && late !== undefined);
    vi.useFakeTimers({ toFake: ["setTimeout", "clearTimeout", "performance"] });
    try {
      await Promise.all([sender.install(1n, key(2)), receiver.install(1n, key(2), 100)]);
      expect(await receiver.open(first)).toEqual(plain.buffer);
      const current = await sender.seal(plain);
      assert(current !== undefined);
      expect(decodeHeader(current)).toMatchObject({ kid: 1n, counter: 0n });
      expect(await receiver.open(current)).toEqual(plain.buffer);
      vi.advanceTimersByTime(100);
      expect(await receiver.open(late)).toBeUndefined();
      await expect(receiver.install(0n, key())).rejects.toThrow();
      await expect(sender.install(0n, key())).rejects.toThrow();
      sender.close();
      receiver.close();
      expect(await sender.seal(plain)).toBeUndefined();
      expect(await receiver.open(current)).toBeUndefined();
    } finally {
      vi.useRealTimers();
    }
  });

  it("does not resurrect an epoch whose key import finishes after close", async () => {
    const sender = new SFrameSender();
    const receiver = new SFrameReceiver();
    const otherSender = new SFrameSender();
    await otherSender.install(0n, key());
    const frame = await otherSender.seal(plain);
    assert(frame !== undefined);

    const sendKey = key();
    const receiveKey = key();
    const importing = Promise.all([sender.install(0n, sendKey), receiver.install(0n, receiveKey)]);
    sender.close();
    receiver.close();
    await importing;

    expect(sendKey).toEqual(new Uint8Array(32));
    expect(receiveKey).toEqual(new Uint8Array(32));
    expect(await sender.seal(plain)).toBeUndefined();
    expect(await receiver.open(frame)).toBeUndefined();
  });
});

it("gates the worker's encoded streams on directional keys and authenticates a fresh media probe", async () => {
  const events = new EventTarget();
  const post = vi.fn();
  vi.stubGlobal("postMessage", post);
  vi.stubGlobal("addEventListener", events.addEventListener.bind(events));

  function transform(direction: "send" | "receive") {
    const source = new TransformStream<{ data: ArrayBuffer }>();
    const frames: ArrayBuffer[] = [];
    const writable = new WritableStream<{ data: ArrayBuffer }>({
      write(frame) { frames.push(frame.data); },
    });
    events.dispatchEvent(Object.assign(new Event("rtctransform"), {
      transformer: { options: { direction }, readable: source.readable, writable },
    }));
    return { frames, writer: source.writable.getWriter() };
  }

  try {
    await import("./sframe-worker");
    expect(post.mock.calls).toEqual([["ready"]]);
    const unkeyed = transform("send");
    await unkeyed.writer.write({ data: plain.buffer });
    await unkeyed.writer.close();
    expect(unkeyed.frames).toEqual([]);

    const send = key(1);
    const receive = key(2);
    events.dispatchEvent(new MessageEvent("message", { data: { generation: 0, send, receive } }));
    await vi.waitFor(() => { expect(post).toHaveBeenLastCalledWith("keyed"); });
    expect(send).toEqual(new Uint8Array(32));
    expect(receive).toEqual(new Uint8Array(32));

    const outgoing = transform("send");
    await outgoing.writer.write({ data: plain.buffer });
    await outgoing.writer.close();
    await vi.waitFor(() => { expect(outgoing.frames).toHaveLength(1); });
    const [protectedFrame] = outgoing.frames;
    assert(protectedFrame !== undefined);
    const peer = new SFrameReceiver();
    await peer.install(0n, key(1));
    expect(await peer.open(new Uint8Array(protectedFrame))).toEqual(plain.buffer);

    const incoming = transform("receive");
    const reply = new SFrameSender();
    await reply.install(0n, key(2));
    const frame = await reply.seal(plain);
    assert(frame !== undefined);
    events.dispatchEvent(new MessageEvent("message", { data: { probe: 1 } }));
    // Our own outgoing ciphertext uses the other direction's key.
    await incoming.writer.write({ data: protectedFrame });
    await incoming.writer.write({ data: frame.buffer });
    await incoming.writer.close();
    await vi.waitFor(() => { expect(incoming.frames).toEqual([plain.buffer]); });
    expect(post.mock.calls).toEqual([["ready"], ["keyed"], [{ received: 1 }]]);
  } finally {
    vi.unstubAllGlobals();
  }
});

it("an opaque forwarder and a listener provisioned with an unrelated epoch secret cannot open forwarded media", async () => {
  const sender = new SFrameSender();
  const joined = new SFrameReceiver();
  const unjoined = new SFrameReceiver();
  const noKeys = new SFrameReceiver();
  const senderSecret = key(17);
  const unrelatedEpoch = key(19);
  await Promise.all([sender.install(0n, senderSecret.slice()), joined.install(0n, senderSecret), unjoined.install(0n, unrelatedEpoch)]);

  // A forwarder copies the same opaque packet to every subscriber, including the unjoined listener.
  const ciphertext = await sender.seal(plain);
  assert(ciphertext !== undefined);
  expect(await joined.open(ciphertext.slice())).toEqual(plain.buffer);
  expect(await unjoined.open(ciphertext.slice())).toBeUndefined();
  expect(await noKeys.open(ciphertext.slice())).toBeUndefined();
});
