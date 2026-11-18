import { describe, expect, it, vi } from "vitest";
import { decodeHeader, encodeHeader, SFrameReceiver, SFrameSender } from "./sframe";

const bytes = (hex: string) => Uint8Array.from(hex.match(/../g) ?? [], (byte) => Number.parseInt(byte, 16));
const plain = bytes("64726166742d696574662d736672616d652d656e63");
const key = (value = 1) => new Uint8Array(32).fill(value);

describe("RFC 9605 SFrame", () => {
  it.each([
    [0n, 0n, "00"],
    [0x123n, 0x4567n, "9901234567"],
    [1n, 0xffn, "18ff"],
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
    expect(decodeHeader(audio!).counter).toBe(0n);
    expect(decodeHeader(video!).counter).toBe(1n);
    await expect(sender.seal(plain, undefined, 0n)).rejects.toThrow();
    await sender.seal(plain, undefined, 0xffffffffffffffffn);
    await expect(sender.seal(plain)).rejects.toThrow();
  });

  it("drops wrong key IDs, wrong directional keys, tampering and simultaneous replay", async () => {
    const sender = new SFrameSender();
    const receiver = new SFrameReceiver();
    const wrongDirection = new SFrameReceiver();
    await Promise.all([sender.install(0n, key()), receiver.install(0n, key()), wrongDirection.install(0n, key(2))]);
    const protectedFrame = (await sender.seal(plain))!;
    const badId = protectedFrame.slice();
    badId[0] = 0x10;
    const badTag = protectedFrame.slice();
    badTag[badTag.length - 1] = badTag.at(-1)! ^ 1;
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
    const stale = (await sender.seal(plain))!;
    const reordered = (await sender.seal(plain))!;
    const newest = (await sender.seal(plain, undefined, 128n))!;
    expect(await receiver.open(newest)).toEqual(plain.buffer);
    expect(await receiver.open(stale)).toBeUndefined();
    expect(await receiver.open(reordered)).toEqual(plain.buffer);
  });

  it("expires the previous epoch and refuses rollback", async () => {
    const sender = new SFrameSender();
    const receiver = new SFrameReceiver();
    await Promise.all([sender.install(0n, key()), receiver.install(0n, key())]);
    const first = (await sender.seal(plain))!;
    const late = (await sender.seal(plain))!;
    vi.useFakeTimers({ toFake: ["setTimeout", "clearTimeout", "performance"] });
    try {
      await Promise.all([sender.install(1n, key(2)), receiver.install(1n, key(2), 100)]);
      expect(await receiver.open(first)).toEqual(plain.buffer);
      const current = (await sender.seal(plain))!;
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
});
