// SPDX-License-Identifier: AGPL-3.0-or-later
// The Home's own usage counter (personal profile, OM_PROFILE=personal). One SQLite Durable Object counts what the Worker serves
// per UTC day, so the figure is the Home's accounting and needs no Cloudflare analytics token (which the Home must not hold).
// What is counted: every request the Worker front door receives (shed ones too) is one Worker request; the Durable Object
// calls made for it are counted too (the meter's own call, plus the one routed call; the usage read adds its admin check).
// A key fetch is the one request that fans out: the front door counts it as one Worker request and the meter's own call, and the
// per-device Durable Object calls it then really makes are charged after the fact (`charge`, plus the charge call itself), so a
// 64-device fetch is not metered as a 1-device one and a fetch that fails on its first device is charged for that one call only.
// A charge that cannot be recorded is dropped (fail open, like `unknown`): the free plan's own hard stop is the backstop.
// Not counted: WebSocket messages after the upgrade. (Decision D3 of the limits report charged a fixed two calls per request and
// so undercounted batched key fetches; superseded by the above, see the limits report.)
import { DurableObject } from "cloudflare:workers";
import type { UsageMetric, UsageStateName } from "@om/home-wire";
import type { Env } from "./env";
import { DAY, USAGE_CONFIG, profileOf } from "./limits";
import { all, run } from "./sql";
import { nowSeconds } from "./util";

/** What one day's counter holds, as the Worker sees it. */
export interface UsageSnapshot { readonly day: number; readonly worker: number; readonly durable: number }

export const usageOn = (env: Env): boolean => profileOf(env) === "personal" && env.USAGE !== undefined;

const ceilingOf = (raw: string | undefined, fallback: number): number => {
  const n = Number(raw);
  return raw !== undefined && Number.isSafeInteger(n) && n > 0 ? n : fallback;
};

export const ceilings = (env: Env): { readonly worker: number; readonly durable: number } => ({
  worker: ceilingOf(env.OM_USAGE_CEILING_WORKER, USAGE_CONFIG.workerRequestsPerDay),
  durable: ceilingOf(env.OM_USAGE_CEILING_DO, USAGE_CONFIG.durableObjectRequestsPerDay),
});

/** The worst of the two metrics decides. */
export function classify(s: UsageSnapshot, c: { readonly worker: number; readonly durable: number }): Exclude<UsageStateName, "unknown"> {
  const ratio = Math.max(s.worker / c.worker, s.durable / c.durable);
  return ratio >= USAGE_CONFIG.hardRatio ? "hard" : ratio >= USAGE_CONFIG.softRatio ? "soft" : "healthy";
}

/** The wire form read by `parseUsageState` (snake_case JSON). */
export function usageDocument(env: Env, s: UsageSnapshot | undefined): Record<string, unknown> {
  const profile = profileOf(env);
  if (s === undefined) return { state: "unknown", profile };
  const c = ceilings(env);
  const metrics: UsageMetric[] = [
    { name: "worker_requests", used: s.worker, ceiling: c.worker },
    { name: "durable_object_requests", used: s.durable, ceiling: c.durable },
  ];
  return { state: classify(s, c), billable_overage: "never", budget_remaining: 0, window_starts_at: s.day * DAY, resets_at: (s.day + 1) * DAY, metrics, profile };
}

/** The one object that counts. Named "meter" by the Worker; it holds nothing but counts and learns nothing about who asked. */
export class UsageMeter extends DurableObject<Env> {
  private readonly sql: SqlStorage;

  constructor(ctx: DurableObjectState, env: Env) {
    super(ctx, env);
    this.sql = ctx.storage.sql;
    this.sql.exec("CREATE TABLE IF NOT EXISTS usage_day (day INTEGER PRIMARY KEY, worker INTEGER NOT NULL, durable INTEGER NOT NULL)");
  }

  private today(): number {
    return Math.floor(nowSeconds() / DAY);
  }

  private snapshot(day: number): UsageSnapshot {
    const r = all<{ worker: number; durable: number }>(this.sql, "SELECT worker, durable FROM usage_day WHERE day = ?", day)[0];
    return { day, worker: r?.worker ?? 0, durable: r?.durable ?? 0 };
  }

  /** Adds one served request (and its Durable Object calls) to today's count and returns the count including it. */
  note(durableCalls: number): UsageSnapshot {
    const day = this.today();
    run(this.sql, "INSERT INTO usage_day (day, worker, durable) VALUES (?, 1, ?) ON CONFLICT(day) DO UPDATE SET worker = worker + 1, durable = durable + excluded.durable", day, durableCalls);
    run(this.sql, "DELETE FROM usage_day WHERE day < ?", day - 7);
    return this.snapshot(day);
  }

  /** Adds Durable Object calls a request made after it was noted (the per-device fan-out of a key fetch). Counts no Worker request. */
  charge(durableCalls: number): void {
    if (!Number.isSafeInteger(durableCalls) || durableCalls <= 0) return;
    const day = this.today();
    run(this.sql, "INSERT INTO usage_day (day, worker, durable) VALUES (?, 0, ?) ON CONFLICT(day) DO UPDATE SET durable = durable + excluded.durable", day, durableCalls);
  }
}
