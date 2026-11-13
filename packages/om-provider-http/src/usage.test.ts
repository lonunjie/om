// SPDX-License-Identifier: MPL-2.0
import { describe, expect, it } from "vitest";
import { FEATURES, USAGE_PATH, UNKNOWN_USAGE, parseUsageState, zeroCostSafe } from "@om/home-wire";
import { createHttpProvider } from "./provider";
import { createMemorySecretStore } from "./secrets";
import { parseProviderUrl } from "./config";

const parsed = parseProviderUrl("http://127.0.0.1:1", { allowLoopbackHttp: true });
if (!parsed) throw new Error("url");
const config = parsed;

const doc = (state: string, extra: Record<string, unknown> = {}) => ({
  state, billable_overage: "never", budget_remaining: 0, window_starts_at: 1000, resets_at: 87_400, profile: "personal",
  metrics: [{ name: "worker_requests", used: 5, ceiling: 100 }], ...extra,
});

function provider(opts: { features?: string[]; usage?: () => Response; seen?: Request[] } = {}) {
  const seen = opts.seen ?? [];
  return createHttpProvider({
    config, secrets: createMemorySecretStore(), allowLoopbackHttp: true, webSocket: false,
    fetch: (i, init) => {
      const req = new Request(i, init);
      seen.push(req);
      const path = new URL(req.url).pathname;
      if (path === "/.well-known/om-provider-capabilities") return Promise.resolve(new Response(JSON.stringify({ schema_version: 1, api_versions: ["1"], features: opts.features ?? [FEATURES.relayUsage] })));
      if (path === USAGE_PATH && opts.usage) return Promise.resolve(opts.usage());
      if (path === "/v1/relay/queues") return Promise.resolve(new Response(null, { status: 204 }));
      return Promise.resolve(new Response(null, { status: 404 }));
    },
  });
}

const MAILBOX = "00".repeat(16);

describe("parseUsageState", () => {
  it("reads each state and fails to unknown on anything else", () => {
    for (const s of ["healthy", "soft", "hard"] as const) expect(parseUsageState(doc(s))).toMatchObject({ state: s, billableOverage: "never", budgetRemaining: 0, window: { startsAt: 1000, resetsAt: 87_400 } });
    expect(parseUsageState(doc("hard")).healthy).toBe(false);
    expect(parseUsageState(doc("soft")).healthy).toBe(true);
    for (const bad of [null, "x", {}, doc("great"), doc("healthy", { metrics: "no" }), doc("healthy", { billable_overage: "maybe" }), doc("healthy", { budget_remaining: 5 }), doc("healthy", { resets_at: -1 }), doc("healthy", { metrics: [{ name: "x", used: 1, ceiling: 2 }] })]) {
      expect(parseUsageState(bad)).toEqual(UNKNOWN_USAGE);
    }
    expect(parseUsageState({ state: "unknown", profile: "personal" })).toMatchObject({ state: "unknown", healthy: false });
  });

  it("only a stated, known state is safe under a zero-cost requirement", () => {
    expect(zeroCostSafe(parseUsageState(doc("soft")))).toBe(true);
    expect(zeroCostSafe(UNKNOWN_USAGE)).toBe(false);
  });
});

describe("HttpProvider.usageState", () => {
  it("is unknown when the Home does not advertise the feature, without asking for it", async () => {
    const seen: Request[] = [];
    expect(await provider({ features: [], seen }).usageState(MAILBOX)).toEqual(UNKNOWN_USAGE);
    expect(seen.map((r) => new URL(r.url).pathname)).toEqual(["/.well-known/om-provider-capabilities"]);
  });

  it("is unknown with no mailbox held here", async () => {
    expect(await provider({ usage: () => new Response(JSON.stringify(doc("healthy"))) }).usageState()).toEqual(UNKNOWN_USAGE);
  });

  it("maps the answer, sends the admin capability only in the Authorization header, and never throws", async () => {
    const seen: Request[] = [];
    const p = provider({ seen, usage: () => new Response(JSON.stringify(doc("soft"))) });
    const u = await p.usageState(MAILBOX);
    expect(u).toMatchObject({ state: "soft", healthy: true, profile: "personal" });
    const r = seen.find((x) => new URL(x.url).pathname === USAGE_PATH);
    expect(r?.url).toBe(`http://127.0.0.1:1${USAGE_PATH}`);
    expect(r?.headers.get("authorization")).toMatch(/^Bearer /);
    expect(await provider({ usage: () => new Response("{", { status: 200 }) }).usageState(MAILBOX)).toEqual(UNKNOWN_USAGE);
    expect(await provider({ usage: () => new Response(null, { status: 401 }) }).usageState(MAILBOX)).toEqual(UNKNOWN_USAGE);
    expect(await provider({ usage: () => { throw new Error("down"); } }).usageState(MAILBOX)).toEqual(UNKNOWN_USAGE);
  });
});
