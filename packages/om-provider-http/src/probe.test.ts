// SPDX-License-Identifier: MPL-2.0
import { describe, expect, it } from "vitest";
import { checkProviderUrl } from "./config";
import { probeHome } from "./probe";

const doc = (over: Record<string, unknown> = {}): Record<string, unknown> => ({
  schema_version: 1, provider_id: "om-reference-home", provider_kinds: ["relay", "key_material"], durability_class: "durable", features: [], api_versions: ["1"], health: "healthy", ...over,
});
const json = (body: unknown, status = 200): Response => new Response(typeof body === "string" ? body : JSON.stringify(body), { status, headers: { "content-type": "application/json" } });
const BASE = "https://home.example";

describe("checkProviderUrl", () => {
  it("names the reason a URL is unusable", () => {
    const why = (u: string): unknown => { const r = checkProviderUrl(u); return r.ok ? "ok" : r.reason; };
    expect(why("https://home.example/")).toBe("ok");
    expect(why("http://home.example")).toBe("not_https");
    expect(why("http://127.0.0.1:8788")).toBe("not_https");
    expect(why("https://u:p@home.example")).toBe("credentials");
    expect(why("https://home.example/?cap=1")).toBe("query_or_fragment");
    expect(why("https://home.example/#x")).toBe("query_or_fragment");
    for (const bad of ["", "home.example", "javascript:alert(1)", "ftp://home.example", "https://"]) expect(why(bad), bad).toBe("malformed");
  });
});

describe("probeHome", () => {
  const asks = (res: Response | Error): { f: typeof fetch; calls: { url: string; init: RequestInit }[] } => {
    const calls: { url: string; init: RequestInit }[] = [];
    const f: typeof fetch = (i, init) => {
      calls.push({ url: typeof i === "string" ? i : i instanceof URL ? i.href : i.url, init: init ?? {} });
      return res instanceof Error ? Promise.reject(res) : Promise.resolve(res);
    };
    return { f, calls };
  };

  it("reports a healthy Home with the id it gives itself; the request carries nothing identifying", async () => {
    const { f, calls } = asks(json(doc()));
    expect(await probeHome(BASE, { fetch: f })).toEqual({ kind: "ok", baseUrl: BASE, providerId: "om-reference-home", health: "healthy" });
    expect(calls).toHaveLength(1);
    expect(calls[0]?.url).toBe(`${BASE}/.well-known/om-provider-capabilities`);
    expect(calls[0]?.init).toMatchObject({ method: "GET", credentials: "omit", redirect: "error", referrerPolicy: "no-referrer" });
    expect(Object.keys((calls[0]?.init.headers as Record<string, string> | undefined) ?? {})).toEqual([]);
  });

  it("passes degraded, unhealthy and unknown health through, and treats a missing or odd value as unknown", async () => {
    for (const h of ["degraded", "unhealthy", "unknown"]) expect(await probeHome(BASE, { fetch: asks(json(doc({ health: h }))).f })).toMatchObject({ kind: "ok", health: h });
    expect(await probeHome(BASE, { fetch: asks(json(doc({ health: "great" }))).f })).toMatchObject({ kind: "ok", health: "unknown" });
    const noHealth = doc();
    delete noHealth["health"];
    expect(await probeHome(BASE, { fetch: asks(json(noHealth)).f })).toMatchObject({ kind: "ok", health: "unknown" });
  });

  it("separates unreachable, failing, not-a-Home, wrong version and no mailbox", async () => {
    expect(await probeHome(BASE, { fetch: asks(new TypeError("Failed to fetch")).f })).toEqual({ kind: "unreachable" });
    expect(await probeHome(BASE, { fetch: asks(json("<html>", 503)).f })).toEqual({ kind: "failing", status: 503 });
    expect(await probeHome(BASE, { fetch: asks(json("nope", 404)).f })).toEqual({ kind: "not_a_home" });
    expect(await probeHome(BASE, { fetch: asks(new Response("<!doctype html><p>hi", { status: 200, headers: { "content-type": "text/html" } })).f })).toEqual({ kind: "not_a_home" });
    expect(await probeHome(BASE, { fetch: asks(json([1, 2])).f })).toEqual({ kind: "not_a_home" });
    expect(await probeHome(BASE, { fetch: asks(json(doc({ schema_version: 2 }))).f })).toEqual({ kind: "not_a_home" });
    expect(await probeHome(BASE, { fetch: asks(json(doc({ provider_id: "bad\u0000id" }))).f })).toEqual({ kind: "not_a_home" });
    expect(await probeHome(BASE, { fetch: asks(json(doc({ provider_id: "x".repeat(65) }))).f })).toEqual({ kind: "not_a_home" });
    expect(await probeHome(BASE, { fetch: asks(json(doc({ api_versions: ["2", "3"] }))).f })).toEqual({ kind: "wrong_version", offered: ["2", "3"] });
    expect(await probeHome(BASE, { fetch: asks(json(doc({ api_versions: [] }))).f })).toEqual({ kind: "wrong_version", offered: [] });
    expect(await probeHome(BASE, { fetch: asks(json(doc({ provider_kinds: ["key_material"] }))).f })).toEqual({ kind: "no_mailbox" });
    expect(await probeHome(BASE, { fetch: asks(json("x".repeat(70_000))).f })).toEqual({ kind: "not_a_home" });
  });

  it("a redirect is an error, not something to follow", async () => {
    // fetch with redirect:"error" rejects; the probe reports an unreachable address.
    expect(await probeHome(BASE, { fetch: asks(new TypeError("redirect mode is set to error")).f })).toEqual({ kind: "unreachable" });
  });
});
