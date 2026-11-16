import { describe, expect, it } from "vitest";
import { createRecoveryTicker, randomSessionId, type MonotonicSample } from "./recovery-ticker";

/** A fake monotonic clock and a fake timer queue: nothing here reads a real clock. */
function rig(opts: { pending?: () => boolean; maxGapMs?: number } = {}) {
  let t = 1000;
  let skew = 0;
  let ids = 0;
  const timers = new Map<number, { at: number; fn: () => void }>();
  let nextTimer = 1;
  const calls: { p: string; s: MonotonicSample }[] = [];
  let pending = opts.pending ?? ((): boolean => true);
  const ticker = createRecoveryTicker({
    now: () => t - skew,
    newSessionId: () => `0${String(++ids)}`.padStart(2, "0") + "ab",
    intervalMs: 30_000,
    ...(opts.maxGapMs === undefined ? {} : { maxGapMs: opts.maxGapMs }),
    pending: () => pending(),
    tick: (p, s) => { calls.push({ p, s }); return Promise.resolve(); },
    setTimer: (fn, ms) => { const id = nextTimer++; timers.set(id, { at: t + ms, fn }); return id; },
    clearTimer: (h) => { timers.delete(h as number); },
  });
  /** Advance the clock (`wall` ms) and fire what is due, as a browser would. */
  const advance = async (ms: number): Promise<void> => {
    t += ms;
    for (const [id, x] of [...timers]) if (x.at <= t) { timers.delete(id); x.fn(); }
    await Promise.resolve(); await Promise.resolve(); await Promise.resolve();
  };
  return { ticker, calls, timers, advance, setSkew: (v: number) => { skew = v; }, setPending: (f: () => boolean) => { pending = f; } };
}
const flush = async (): Promise<void> => { for (let i = 0; i < 5; i++) await Promise.resolve(); };

describe("recovery ticker (strict timer, monotonic clock only)", () => {
  it("ticks at start-up and then on an interval while a recovery waits, in one boot session", async () => {
    const r = rig();
    r.ticker.wake("a");
    await flush();
    await r.advance(30_000);
    await r.advance(30_000);
    expect(r.calls.map((c) => c.s.elapsedMs)).toEqual([1000, 31_000, 61_000]);
    expect(new Set(r.calls.map((c) => c.s.bootSession)).size).toBe(1);
  });

  it("does not poll, and keeps no timer, when nothing is pending; it stops by itself when the wait ends", async () => {
    const r = rig({ pending: () => false });
    r.ticker.wake("a");
    await flush();
    expect(r.calls).toEqual([]);
    expect(r.timers.size).toBe(0);
    const w = rig();
    let waiting = true;
    w.setPending(() => waiting);
    w.ticker.wake("a");
    await flush();
    waiting = false;
    await w.advance(30_000);
    expect(w.calls.length).toBe(2);
    await w.advance(30_000);
    expect(w.calls.length).toBe(2);
    expect(w.timers.size).toBe(0);
  });

  it("a restart is a new boot session: credit never counts across it", async () => {
    const seen: string[] = [];
    for (let boot = 0; boot < 2; boot++) { // two page loads: each makes a new ticker, and a new random id
      const t = createRecoveryTicker({ now: () => 100 + boot * 50_000, newSessionId: randomSessionId, intervalMs: 30_000, pending: () => true, tick: (_p, s) => { seen.push(s.bootSession); return Promise.resolve(); }, setTimer: () => 1, clearTimer: () => undefined });
      t.wake("a");
      await flush();
      t.dispose();
    }
    expect(seen).toHaveLength(2);
    expect(seen[0]).not.toBe(seen[1]);
    expect(seen[0]).toMatch(/^[0-9a-f]{32}$/);
  });

  it("a clock that goes backwards is ignored and cuts the run; nothing after it can credit the lost span", async () => {
    const r = rig();
    r.ticker.wake("a");
    await flush();
    const before = r.calls[0]?.s.bootSession;
    r.setSkew(30_500); // the clock now reads earlier than before
    await r.advance(30_000);
    expect(r.calls.length).toBe(1); // that reading was not sent
    await r.advance(30_000);
    expect(r.calls.length).toBe(2);
    expect(r.calls[1]?.s.bootSession).not.toBe(before);
  });

  it("a long gap (sleep, a frozen tab) starts a new session instead of crediting it", async () => {
    const r = rig();
    r.ticker.wake("a");
    await flush();
    await r.advance(10 * 60_000);
    expect(r.calls.length).toBe(2);
    expect(r.calls[1]?.s.bootSession).not.toBe(r.calls[0]?.s.bootSession);
    await r.advance(30_000);
    expect(r.calls[2]?.s.bootSession).toBe(r.calls[1]?.s.bootSession);
  });

  it("lock or profile switch stops it at once, and a reopen is a new session", async () => {
    const r = rig();
    r.ticker.wake("a");
    await flush();
    r.ticker.stop("a");
    expect(r.timers.size).toBe(0);
    await r.advance(5 * 30_000);
    expect(r.calls.length).toBe(1);
    r.ticker.wake("a");
    await flush();
    expect(r.calls.length).toBe(2);
    expect(r.calls[1]?.s.bootSession).not.toBe(r.calls[0]?.s.bootSession);
  });

  it("profiles are independent: stopping one leaves the other ticking", async () => {
    const r = rig();
    r.ticker.wake("a"); r.ticker.wake("b");
    await flush();
    r.ticker.stop("a");
    await r.advance(30_000);
    expect(r.calls.filter((c) => c.p === "a").length).toBe(1);
    expect(r.calls.filter((c) => c.p === "b").length).toBe(2);
  });

  it("dispose clears every timer and ignores later wakes", async () => {
    const r = rig();
    r.ticker.wake("a");
    await flush();
    r.ticker.dispose();
    expect(r.timers.size).toBe(0);
    r.ticker.wake("a");
    await flush();
    expect(r.calls.length).toBe(1);
  });

  it("a refused tick or a throwing pending check credits nothing and never throws out", async () => {
    let n = 0;
    const timers: (() => void)[] = [];
    const t = createRecoveryTicker({
      now: () => 5, newSessionId: () => "aabb", intervalMs: 10,
      pending: () => { if (++n > 1) throw new Error("closed"); return true; },
      tick: () => Promise.reject(new Error("stale")),
      setTimer: (fn) => { timers.push(fn); return timers.length; }, clearTimer: () => undefined,
    });
    t.wake("a");
    await flush();
    expect(timers.length).toBe(0); // pending threw: counted as no, so the run ended
  });

  it("refuses a malformed session id (fail closed)", () => {
    const t = createRecoveryTicker({ now: () => 1, newSessionId: () => "NOT HEX", intervalMs: 1, pending: () => true, tick: () => Promise.resolve() });
    expect(() => { t.wake("a"); }).toThrow();
  });
});
