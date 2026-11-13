// SPDX-License-Identifier: MPL-2.0
// ProviderUsageState as a Home reports it (IMPLEMENTATION_INTERFACES.md "ProviderUsageState"; conformance PC-C03). The design fixes what the
// state must distinguish, not its encoding: the JSON shape below is an implementation choice of the reference Home (decision D1 of
// docs/reports/2026-11-13-limits-profile.md), read only with the owner's admin capability, never in a URL.

/** Advertised feature id and path of the usage read. Kept together so a rename is one edit. */
export const USAGE_FEATURE = "om.relay.usage.v1";
export const USAGE_PATH = "/v1/relay/usage";

/** `healthy` below the soft threshold, `soft` approaching a quota, `hard` at the free-tier failure threshold, `unknown` when the count cannot be read or the Home does not say. */
export const USAGE_STATES = ["healthy", "soft", "hard", "unknown"] as const;
export type UsageStateName = (typeof USAGE_STATES)[number];

export const USAGE_METRICS = ["worker_requests", "durable_object_requests"] as const;
export type UsageMetricName = (typeof USAGE_METRICS)[number];

export interface UsageMetric { readonly name: UsageMetricName; readonly used: number; readonly ceiling: number }

export interface ProviderUsageState {
  readonly state: UsageStateName;
  /** False once deposits are shed (`hard`) or when nothing is known. */
  readonly healthy: boolean;
  /** `never`: the Home has no billable resource. `unknown` whenever the state is. */
  readonly billableOverage: "never" | "unknown";
  /** Configured paid budget left: 0 by policy (no spending), null when unknown. */
  readonly budgetRemaining: number | null;
  /** The counting window in POSIX seconds, or null when unknown. */
  readonly window: { readonly startsAt: number; readonly resetsAt: number } | null;
  readonly metrics: readonly UsageMetric[];
  readonly profile: string;
}

export const UNKNOWN_USAGE: ProviderUsageState = { state: "unknown", healthy: false, billableOverage: "unknown", budgetRemaining: null, window: null, metrics: [], profile: "unknown" };

const isCount = (v: unknown): v is number => typeof v === "number" && Number.isSafeInteger(v) && v >= 0;

/** Strict reader: anything that is not exactly this shape is `unknown`, never a guess. */
export function parseUsageState(raw: unknown): ProviderUsageState {
  if (typeof raw !== "object" || raw === null) return UNKNOWN_USAGE;
  const o = raw as Record<string, unknown>;
  const state = o["state"];
  if (typeof state !== "string" || !(USAGE_STATES as readonly string[]).includes(state)) return UNKNOWN_USAGE;
  const metrics: UsageMetric[] = [];
  if (!Array.isArray(o["metrics"])) return UNKNOWN_USAGE;
  for (const m of o["metrics"] as unknown[]) {
    if (typeof m !== "object" || m === null) return UNKNOWN_USAGE;
    const { name, used, ceiling } = m as Record<string, unknown>;
    if (typeof name !== "string" || !(USAGE_METRICS as readonly string[]).includes(name) || !isCount(used) || !isCount(ceiling)) return UNKNOWN_USAGE;
    metrics.push({ name: name as UsageMetricName, used, ceiling });
  }
  const starts = o["window_starts_at"];
  const resets = o["resets_at"];
  const window = isCount(starts) && isCount(resets) ? { startsAt: starts, resetsAt: resets } : null;
  const profile = typeof o["profile"] === "string" ? o["profile"] : "unknown";
  if (state === "unknown") return { ...UNKNOWN_USAGE, profile };
  if (o["billable_overage"] !== "never" || o["budget_remaining"] !== 0 || window === null) return UNKNOWN_USAGE; // a Home that does not vouch for zero cost is not trusted for it
  return { state: state as UsageStateName, healthy: state === "healthy" || state === "soft", billableOverage: "never", budgetRemaining: 0, window, metrics, profile };
}

/** The routing question under a hard zero-cost requirement: only a Home that said so, in a state that is not unknown, qualifies. */
export const zeroCostSafe = (u: ProviderUsageState): boolean => u.state !== "unknown" && u.billableOverage === "never" && u.budgetRemaining === 0;
