// Drives Core's `recovery_tick` from the running app (ADR-0190 section 6, D73, D88). Credit is this device's own monotonic clock
// while the app runs and the profile is open, and nothing else: no wall-clock difference, no time a contact, a sender, a Home or any
// other device supplies. A boot session id names one unbroken run of the clock; Core credits only growth inside one id, so a new id
// (a restart, a lock, a long gap, a clock that went backwards) starts again from zero. Pure scheduling logic: no Core, no DOM.

/** The reading Core takes: the id of one boot of the monotonic clock, and the milliseconds that clock has run in it. */
export interface MonotonicSample {
  readonly bootSession: string;
  readonly elapsedMs: number;
}

export interface RecoveryTickerOptions {
  /** A monotonic clock in milliseconds (`performance.now`). Never `Date.now`, never a value from the network. */
  readonly now: () => number;
  /** A fresh unpredictable id for one boot session, 2 to 64 lowercase hex characters (Core takes 1 to 32 bytes). */
  readonly newSessionId: () => string;
  /** Time between ticks while a recovery is waiting. */
  readonly intervalMs: number;
  /** More than this between two ticks of one profile is a gap (sleep, a frozen tab): it starts a new session and credits nothing. Default five intervals. */
  readonly maxGapMs?: number;
  /** Whether this profile has a recovery that still needs credit. Cheap and local; a throw counts as no. */
  readonly pending: (profileId: string) => boolean;
  /** Hands one sample to Core for this profile. A refusal (profile locked, context stale) is ignored: nothing is credited. */
  readonly tick: (profileId: string, sample: MonotonicSample) => Promise<void>;
  readonly setTimer?: (fn: () => void, ms: number) => unknown;
  readonly clearTimer?: (handle: unknown) => void;
}

export interface RecoveryTicker {
  /** Something may have started waiting (a profile opened, a commit released): tick now, then keep ticking while a recovery waits. */
  wake(profileId: string): void;
  /** The profile locked or closed: no further tick, and its next run is a new boot session. */
  stop(profileId: string): void;
  dispose(): void;
}

interface Run {
  session: string;
  last: number | null;
  busy: boolean;
  timer: unknown;
}

const HEX = /^[0-9a-f]{2,64}$/;

export function createRecoveryTicker(o: RecoveryTickerOptions): RecoveryTicker {
  const setTimer = o.setTimer ?? ((fn, ms) => setTimeout(fn, ms));
  const clearTimer = o.clearTimer ?? ((h) => { clearTimeout(h as ReturnType<typeof setTimeout>); });
  const maxGap = o.maxGapMs ?? o.intervalMs * 5;
  const runs = new Map<string, Run>();
  let disposed = false;

  const isPending = (p: string): boolean => { try { return o.pending(p); } catch { return false; } };
  const fresh = (): string => {
    const id = o.newSessionId();
    if (!HEX.test(id)) throw new Error("boot session id must be lowercase hex");
    return id;
  };

  /** The next sample, or null when this reading must not be used (the clock went backwards: the run is cut and credits nothing). */
  function sample(r: Run): MonotonicSample | null {
    const t = Math.floor(o.now());
    if (!Number.isFinite(t) || t < 0) { r.session = fresh(); r.last = null; return null; }
    const last = r.last;
    r.last = t;
    if (last !== null) {
      if (t < last) { r.session = fresh(); return null; }
      if (t - last > maxGap) r.session = fresh();
    }
    return { bootSession: r.session, elapsedMs: t };
  }

  async function run(p: string, r: Run): Promise<void> {
    r.busy = true;
    try {
      const s = sample(r);
      if (s !== null) await o.tick(p, s);
    } catch { /* the profile closed or Core refused: nothing was credited */ }
    r.busy = false;
    if (disposed || runs.get(p) !== r) return; // stopped while the tick was in flight
    if (!isPending(p)) { runs.delete(p); return; } // nothing waits any more: no timer, nothing retained
    r.timer = setTimer(() => { r.timer = undefined; void run(p, r); }, o.intervalMs);
  }

  return {
    wake(p) {
      if (disposed) return;
      const cur = runs.get(p);
      if (cur !== undefined) return; // already ticking (or in flight): it re-arms itself
      if (!isPending(p)) return;
      const r: Run = { session: fresh(), last: null, busy: false, timer: undefined };
      runs.set(p, r);
      void run(p, r);
    },
    stop(p) {
      const r = runs.get(p);
      if (r === undefined) return;
      if (r.timer !== undefined) clearTimer(r.timer);
      runs.delete(p);
    },
    dispose() {
      disposed = true;
      for (const r of runs.values()) if (r.timer !== undefined) clearTimer(r.timer);
      runs.clear();
    },
  };
}

/** 16 random bytes as hex from the platform's CSPRNG. Fails closed (throws) when there is none: no id, no credit. */
export function randomSessionId(): string {
  const c = (globalThis as { crypto?: { getRandomValues?: (a: Uint8Array) => Uint8Array } }).crypto;
  if (c?.getRandomValues === undefined) throw new Error("no secure random source");
  const b = c.getRandomValues(new Uint8Array(16));
  return Array.from(b, (x) => x.toString(16).padStart(2, "0")).join("");
}
