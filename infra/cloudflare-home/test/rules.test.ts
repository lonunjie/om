// SPDX-License-Identifier: MPL-2.0
// Time-dependent decisions with an explicit clock (workerd cannot be time-travelled), plus source-level privacy guards.
import { readdirSync, readFileSync } from "node:fs";
import { join } from "node:path";
import { describe, expect, it } from "vitest";
import { LIMITS } from "../src/limits";
import { depositCapStatus, receiveStatus, sealedExpiry } from "../src/rules";

const T = 1_800_000_000;

describe("PC-A03 / PC-D04 time rules", () => {
  const gen0 = { revoked: false, expiresAt: null, generation: 0 };
  it("an old deposit capability works through the rotation overlap and the skew, then expires", () => {
    const q = { current: 1, overlapUntil: T };
    expect(depositCapStatus(gen0, q, T - 1)).toBe("ok");
    expect(depositCapStatus(gen0, q, T + LIMITS.skewSeconds)).toBe("ok");
    expect(depositCapStatus(gen0, q, T + LIMITS.skewSeconds + 1)).toBe("expired");
    expect(depositCapStatus({ ...gen0, generation: 1 }, q, T + 10 * LIMITS.rotateOverlapSeconds)).toBe("ok");
  });
  it("revoked beats everything; a capability with its own expiry expires after the skew", () => {
    expect(depositCapStatus({ ...gen0, revoked: true }, { current: 0, overlapUntil: 0 }, T)).toBe("revoked");
    expect(depositCapStatus({ ...gen0, expiresAt: T }, { current: 0, overlapUntil: 0 }, T + LIMITS.skewSeconds)).toBe("ok");
    expect(depositCapStatus({ ...gen0, expiresAt: T }, { current: 0, overlapUntil: 0 }, T + LIMITS.skewSeconds + 1)).toBe("expired");
  });
  it("the previous receive capability drains only through the overlap", () => {
    expect(receiveStatus("c", "c", "p", 0, T)).toBe("current");
    expect(receiveStatus("p", "c", "p", T, T)).toBe("previous_in_overlap");
    expect(receiveStatus("p", "c", "p", T, T + LIMITS.skewSeconds + 1)).toBe("previous_expired");
    expect(receiveStatus("x", "c", "p", T, T)).toBe("unknown");
    expect(receiveStatus("x", undefined, undefined, 0, T)).toBe("unknown");
  });
  it("sealed expiry: past beyond skew is expired, beyond the retention bound is refused, otherwise capped to the bound", () => {
    expect(sealedExpiry(T - LIMITS.skewSeconds - 1, T)).toEqual({ ok: false, error: "expired" });
    expect(sealedExpiry(T - LIMITS.skewSeconds, T)).toEqual({ ok: true, retainUntil: T - LIMITS.skewSeconds });
    expect(sealedExpiry(T + LIMITS.maxRetentionSeconds + LIMITS.skewSeconds + 1, T)).toEqual({ ok: false, error: "invalid_request" });
    expect(sealedExpiry(T + LIMITS.maxRetentionSeconds + 1, T)).toEqual({ ok: true, retainUntil: T + LIMITS.maxRetentionSeconds });
  });
});

describe("PC-A06 / metadata minimisation (source guard)", () => {
  const dir = new URL("../src", import.meta.url).pathname;
  const files = readdirSync(dir).filter((f) => f.endsWith(".ts")).map((f) => ({ f, text: readFileSync(join(dir, f), "utf8") }));
  const code = (t: string): string => t.split("\n").filter((l) => !l.trim().startsWith("//")).join("\n");
  it("no logging, no network-address or client-identifying header, no cookie, no request.cf geolocation", () => {
    for (const { f, text } of files) {
      const c = code(text);
      expect(c, `${f} logs`).not.toMatch(/\bconsole\./);
      expect(c, `${f} reads an address`).not.toMatch(/cf-connecting-ip|x-forwarded-for|x-real-ip|true-client-ip|request\.cf\b|\.cf\?|cf-ipcountry|forwarded/i);
      expect(c, `${f} cookie`).not.toMatch(/set-cookie|headers\.get\("cookie|getCookie/i);
      expect(c, `${f} user agent`).not.toMatch(/user-agent|\.headers\.get\("(?!authorization|content-length)/i);
    }
  });
  it("capabilities never appear in a URL: the only dynamic path parts are object ids and device ids", () => {
    const index = code(files.find((x) => x.f === "index.ts")?.text ?? "");
    expect(index).not.toMatch(/searchParams|url\.search|\.query/);
  });
  it("wrangler config turns request logging and tracing off and carries no secret", () => {
    const w = readFileSync(new URL("../wrangler.jsonc", import.meta.url), "utf8").split("\n").filter((l) => !l.trim().startsWith("//")).join("\n");
    expect(w).toMatch(/"observability":\s*\{\s*"enabled":\s*false\s*\}/);
    expect(w).not.toMatch(/account_id|api[_-]?token|secret|zone_id|route/i);
  });
});
