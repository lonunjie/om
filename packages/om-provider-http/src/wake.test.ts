// SPDX-License-Identifier: MPL-2.0
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { WAKE_CLOSE, WAKE_PING, decodeWakeAuth, encodeWakeEvent } from "@om/home-wire";
import { createWakeHub, type WakeHub, type WakeHubOptions } from "./wake";

class FakeSocket {
  static all: FakeSocket[] = [];
  binaryType = "blob";
  onopen: (() => void) | null = null;
  onmessage: ((e: { data: unknown }) => void) | null = null;
  onclose: ((e: { code: number }) => void) | null = null;
  onerror: (() => void) | null = null;
  sent: (string | Uint8Array)[] = [];
  closed: number | undefined;
  constructor(readonly url: string, readonly protocols: string[]) { FakeSocket.all.push(this); }
  send(d: string | Uint8Array): void { this.sent.push(d); }
  close(code?: number): void { this.closed = code ?? 1000; }
  // test controls
  open(): void { this.onopen?.(); }
  frame(kind: "ready" | "wake", seq: number): void { const b = encodeWakeEvent({ kind, seq }); this.onmessage?.({ data: b.buffer.slice(b.byteOffset, b.byteOffset + b.byteLength) }); }
  text(s: string): void { this.onmessage?.({ data: s }); }
  drop(code = 1006): void { this.onclose?.({ code }); }
}

const QUEUE = new Uint8Array(16).fill(7);
const RECEIVE = new Uint8Array(48).fill(9);
const flush = async (): Promise<void> => { for (let i = 0; i < 6; i++) await Promise.resolve(); };

describe("wake hub (client side of om.relay.wake.v1)", () => {
  let hub: WakeHub;
  let supported = true;
  beforeEach(() => {
    vi.useFakeTimers();
    FakeSocket.all = [];
    supported = true;
  });
  afterEach(() => { hub.dispose(); vi.useRealTimers(); });

  const make = (extra: Partial<WakeHubOptions> = {}): WakeHub => {
    hub = createWakeHub({
      base: "https://home.example/om",
      webSocket: (u, p) => new FakeSocket(u, p) as unknown as WebSocket,
      supported: () => Promise.resolve(supported),
      credentials: () => Promise.resolve({ queueId: QUEUE, receive: RECEIVE }),
      now: () => Date.now(),
      ...extra,
    });
    return hub;
  };
  /** Active hub with one mailbox and an authenticated socket. */
  async function live(extra: Partial<WakeHubOptions> = {}, after = 0): Promise<FakeSocket> {
    make(extra);
    hub.setActive(true);
    hub.watch("aa", after);
    await flush();
    const s = FakeSocket.all[0];
    if (!s) throw new Error("no socket");
    s.open();
    s.frame("ready", 0);
    return s;
  }

  it("connects to the bare path of the Home with the feature id and routing label, and sends the capability only as the first frame", async () => {
    make();
    hub.setActive(true);
    hub.watch("aa", 0);
    await flush();
    const s = FakeSocket.all[0];
    expect(s?.url).toBe("wss://home.example/om/v1/relay/wake");
    expect(s?.protocols).toEqual(["om.relay.wake.v1", `om.q.${"07".repeat(16)}`]);
    expect(s?.binaryType).toBe("arraybuffer");
    s?.open();
    const first = s?.sent[0];
    expect(first).toBeInstanceOf(Uint8Array);
    expect(decodeWakeAuth(first as Uint8Array).cap).toEqual(RECEIVE);
    expect(s?.url).not.toContain("09090909");
  });

  it("does nothing until active, when the Home does not advertise the feature, or without a WebSocket", async () => {
    make();
    hub.watch("aa", 0);
    await flush();
    expect(FakeSocket.all).toHaveLength(0);
    supported = false;
    hub.setActive(true);
    await flush();
    expect(FakeSocket.all).toHaveLength(0);
    expect(hub.quiet("aa", 0)).toBe(false);
    hub.dispose();
    make({ webSocket: undefined });
    hub.setActive(true);
    hub.watch("aa", 0);
    await flush();
    expect(hub.stats().connects).toBe(0);
  });

  it("is quiet only while live and nothing newer is known; a wake makes it loud and notifies once", async () => {
    const s = await live();
    const heard = vi.fn();
    hub.onWake(heard);
    expect(hub.quiet("aa", 0)).toBe(true);
    s.frame("wake", 3);
    expect(heard).toHaveBeenCalledTimes(1);
    expect(hub.quiet("aa", 0)).toBe(false);
    expect(hub.quiet("aa", 3)).toBe(true); // Core holds seq 3 now
    hub.drained("aa", 3);
    expect(hub.quiet("aa", 0)).toBe(true);
  });

  it("a ready frame ahead of what the caller holds notifies; one at or below it does not", async () => {
    make();
    const heard = vi.fn();
    hub.onWake(heard);
    hub.setActive(true);
    hub.watch("aa", 5);
    await flush();
    const s = FakeSocket.all[0] as FakeSocket;
    s.open();
    s.frame("ready", 5);
    expect(heard).not.toHaveBeenCalled();
    expect(hub.quiet("aa", 5)).toBe(true);
    s.frame("wake", 6);
    expect(heard).toHaveBeenCalledTimes(1);
  });

  it("lets one real drain through after the safety interval, and a drain that finds nothing re-verifies", async () => {
    await live({ safetyMs: 1000 });
    expect(hub.quiet("aa", 0)).toBe(true);
    vi.advanceTimersByTime(1001);
    expect(hub.quiet("aa", 0)).toBe(false);
    hub.drained("aa", 0);
    expect(hub.quiet("aa", 0)).toBe(true);
  });

  it("pings while live; a missing pong closes the socket and reconnects after a backoff", async () => {
    const s = await live({ pingMs: 1000, pongMs: 500 });
    vi.advanceTimersByTime(1000);
    expect(s.sent.at(-1)).toBe(WAKE_PING);
    s.text("pong");
    vi.advanceTimersByTime(600);
    expect(s.closed).toBeUndefined();
    vi.advanceTimersByTime(400); // next ping
    expect(s.sent.at(-1)).toBe(WAKE_PING);
    vi.advanceTimersByTime(600); // no pong
    expect(s.closed).toBeDefined();
    expect(hub.quiet("aa", 0)).toBe(false);
    vi.advanceTimersByTime(4000);
    await flush();
    expect(FakeSocket.all).toHaveLength(2);
  });

  it("a dropped socket falls back to polling and reconnects; a refused capability is never retried", async () => {
    const s = await live();
    s.drop();
    expect(hub.quiet("aa", 0)).toBe(false);
    vi.advanceTimersByTime(4000);
    await flush();
    const s2 = FakeSocket.all[1] as FakeSocket;
    expect(s2).toBeDefined();
    s2.open();
    s2.drop(WAKE_CLOSE.unauthorized);
    vi.advanceTimersByTime(10 * 60_000);
    await flush();
    expect(FakeSocket.all).toHaveLength(2);
  });

  it("a rotated capability reconnects at once", async () => {
    const s = await live();
    s.drop(WAKE_CLOSE.superseded);
    vi.advanceTimersByTime(1);
    await flush();
    expect(FakeSocket.all).toHaveLength(2);
  });

  it("an attempt that never reaches ready times out, backs off, and after repeated failures rests for minutes", async () => {
    make({ connectMs: 100 });
    hub.setActive(true);
    hub.watch("aa", 0);
    await flush();
    for (let i = 0; i < 6; i++) { vi.advanceTimersByTime(100); vi.advanceTimersByTime(90_000); await flush(); }
    const attempts = FakeSocket.all.length;
    expect(attempts).toBeGreaterThanOrEqual(5);
    expect(attempts).toBeLessThanOrEqual(7);
    vi.advanceTimersByTime(60_000);
    await flush();
    expect(FakeSocket.all.length).toBe(attempts); // resting
  });

  it("going inactive closes every socket at once and polling takes over; becoming active reconnects without a new watch", async () => {
    const s = await live();
    hub.setActive(false);
    expect(s.closed).toBeDefined();
    expect(hub.quiet("aa", 0)).toBe(false);
    expect(hub.stats().sockets).toBe(0);
    hub.setActive(true);
    await flush();
    expect(FakeSocket.all).toHaveLength(2);
  });

  it("holds at most maxSockets mailboxes; the rest stay on polling", async () => {
    make({ maxSockets: 2 });
    hub.setActive(true);
    for (const m of ["a1", "a2", "a3"]) hub.watch(m, 0);
    await flush();
    expect(FakeSocket.all).toHaveLength(2);
    expect(hub.quiet("a3", 0)).toBe(false);
  });

  it("a malformed frame ends the session instead of being trusted", async () => {
    const s = await live();
    s.onmessage?.({ data: new Uint8Array([1, 2, 3]).buffer });
    expect(s.closed).toBeDefined();
    expect(hub.quiet("aa", 0)).toBe(false);
  });
});
