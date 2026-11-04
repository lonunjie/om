// SPDX-License-Identifier: AGPL-3.0-or-later
// Optional feature om.blob.spool.v1: the bounded encrypted blob spool of the reference Home (no R2, free baseline). Cases PC-S01..S13
// run against the real Worker bundle in workerd. Ciphertext here is random bytes: the Home must not care what it holds.
import { ADMISSION_HEADER, admissionHash, blobCiphertextDigest, encodeQueueCreate, quotaDetail, toBase64Url, toHex } from "@om/home-wire";
import { afterAll, beforeAll, describe, expect, it } from "vitest";
import { scratch, startHome, type Home } from "./harness";
import { asBody, bearer, blobs, newCaps, relay, sealed } from "./kit";

/** Random bytes of any length (getRandomValues is capped at 64 KiB per call). */
const rand = (n: number): Uint8Array => {
  const out = new Uint8Array(n);
  for (let i = 0; i < n; i += 65536) crypto.getRandomValues(out.subarray(i, Math.min(n, i + 65536)));
  return out;
};

const secret = rand(32);
const owner = { [ADMISSION_HEADER]: toBase64Url(secret) };
const MIB = 1024 * 1024;
let home: Home;
let open: Home;
let unbound: Home;
beforeAll(async () => {
  const vars = { OM_ADMISSION: await admissionHash(secret) };
  home = await startHome({ vars });
  open = await startHome(); // no admission: a shared or test Home
  unbound = await startHome({ vars, blobs: false }); // a deployment whose configuration predates the spool
}, 120_000);
afterAll(async () => { await home.dispose(); await open.dispose(); await unbound.dispose(); });

const put = (ct: Uint8Array, extra: Partial<Parameters<typeof blobs.put>[1]> = {}, h = home) => blobs.put(h, { ciphertext: ct, admission: owner, ...extra });
interface Put { readCap: Uint8Array; digest: Uint8Array; expiresAt: number }
const okOf = (r: { ok: boolean } & Record<string, unknown>): Put => { if (!r.ok) throw new Error(`unexpected failure ${JSON.stringify(r["error"])}`); return r["value"] as Put; };
const errName = (r: { ok: boolean; error?: { name: string } }): string => (r.ok ? "ok" : (r.error?.name ?? "?"));

describe("PC-S blob spool (om.blob.spool.v1)", () => {
  it("PC-S01 is advertised, with its limits, only where the Home is bound, not switched off and has owner admission", async () => {
    const caps = async (h: Home) => (await (await h.fetch("/.well-known/om-provider-capabilities")).json()) as { features: string[]; provider_kinds: string[]; limits: Record<string, number> };
    const on = await caps(home);
    expect(on.features).toContain("om.blob.spool.v1");
    expect(on.provider_kinds).toContain("blob");
    expect(on.limits).toMatchObject({ max_blob_spool_bytes: 32 * MIB, max_blob_bytes: 4 * MIB, max_blobs: 64, blob_default_retention_seconds: 7 * 86400 });
    for (const h of [open, unbound]) {
      const c = await caps(h);
      expect(c.features).not.toContain("om.blob.spool.v1");
      expect(c.provider_kinds).not.toContain("blob");
      expect(c.limits).not.toHaveProperty("max_blob_spool_bytes");
    }
    const off = await startHome({ vars: { OM_ADMISSION: await admissionHash(secret), OM_BLOB_SPOOL: "0" } });
    try { expect((await caps(off)).features).not.toContain("om.blob.spool.v1"); expect((await put(rand(100), {}, off)).result.ok).toBe(false); } finally { await off.dispose(); }
  });

  it("PC-S02 a Home that does not offer the spool answers every blob request not_found, and its other features are untouched", async () => {
    for (const h of [open, unbound]) {
      const r = await put(rand(100), {}, h);
      expect(r.result.ok ? 0 : r.result.status).toBe(404);
      expect(errName(r.result)).toBe("not_found");
      expect((await blobs.get(h, "00".repeat(16), rand(48))).status).toBe(404);
    }
    const c = newCaps();
    const created = await unbound.fetch("/v1/relay/queues", { method: "POST", headers: { "content-type": "application/cbor", ...owner }, body: asBody(encodeQueueCreate({ requestId: rand(16), receiveCap: c.receive, depositCap: c.deposit, adminCap: c.admin })) });
    expect(created.status).toBeLessThan(300);
    expect((await relay.deposit(unbound, c.deposit, sealed())).ok).toBe(true);
    expect((await relay.drain(unbound, c.receive).then((r) => r.ok && r.value.items.length))).toBe(1);
  });

  it("PC-S03 creating a blob needs the owner's admission; a missing or wrong secret is unauthorized, identically, and stores nothing", async () => {
    for (const admission of [{}, { [ADMISSION_HEADER]: toBase64Url(rand(32)) }, { [ADMISSION_HEADER]: toBase64Url(rand(16)) }]) {
      const r = await blobs.put(home, { ciphertext: rand(500), admission });
      expect(errName(r.result)).toBe("unauthorized");
      expect(r.result.ok ? 0 : r.result.status).toBe(401);
      expect((await blobs.get(home, blobs.locator(r.cap), r.cap)).ok).toBe(false);
    }
  });

  it("PC-S04 put verifies the ciphertext digest: a missing, malformed, wrong or truncated digest stores nothing", async () => {
    const ct = rand(2000);
    const good = await blobCiphertextDigest(ct);
    const wrong = await blobCiphertextDigest(rand(2000));
    for (const [what, digest, want] of [["missing", null, "invalid_request"], ["wrong", wrong, "invalid_request"], ["short", good.slice(0, 31), "invalid_request"]] as const) {
      const r = await put(ct, { digest });
      expect(errName(r.result), what).toBe(want);
      expect((await blobs.get(home, blobs.locator(r.cap), r.cap)).ok, what).toBe(false);
    }
    const altered = ct.slice(); altered[10] = (altered[10] ?? 0) ^ 1;
    const r = await put(altered, { digest: good }); // digest of the original, bytes altered in transit
    expect(errName(r.result)).toBe("invalid_request");
    expect((await put(ct)).result.ok).toBe(true);
  });

  it("PC-S05 put returns the digest the Home computed, a read capability and the expiry; both capabilities fetch the same bytes", async () => {
    const ct = rand(30_000);
    const p = await put(ct);
    const res = okOf(p.result);
    expect(toHex(res.digest)).toBe(toHex(await blobCiphertextDigest(ct)));
    expect(res.readCap.length).toBe(48);
    expect(toHex(res.readCap)).not.toBe(toHex(p.cap));
    expect(toHex(res.readCap.slice(0, 16))).toBe(toHex(p.cap.slice(0, 16))); // same locator
    const loc = blobs.locator(p.cap);
    for (const cap of [res.readCap, p.cap]) {
      const g = await blobs.get(home, loc, cap);
      expect(g.ok && toHex(g.bytes)).toBe(toHex(ct));
      expect(g.headers.get("content-type")).toBe("application/octet-stream");
      expect(g.headers.get("cache-control")).toBe("no-store");
      expect(g.headers.get("referrer-policy")).toBe("no-referrer");
      expect(g.headers.get("set-cookie")).toBeNull();
    }
    const now = Math.floor(Date.now() / 1000);
    expect(res.expiresAt).toBeGreaterThan(now + 7 * 86400 - 60);
    expect(res.expiresAt).toBeLessThan(now + 7 * 86400 + 60);
  });

  it("PC-S06 a wrong capability, an unknown locator and a capability for another locator all answer not_found; no capability is unauthorized", async () => {
    const p = await put(rand(900));
    const loc = blobs.locator(p.cap);
    const other = await put(rand(900));
    const cases: [string, Uint8Array][] = [["wrong secret", new Uint8Array([...p.cap.slice(0, 16), ...rand(32)])], ["random", rand(48)], ["another blob's owner cap", other.cap]];
    for (const [what, cap] of cases) {
      const g = await blobs.get(home, loc, cap);
      expect(g.ok ? "ok" : g.error.name, what).toBe("not_found");
      expect(g.status, what).toBe(404);
    }
    const unknown = await blobs.get(home, toHex(rand(16)), p.cap);
    expect(unknown.ok ? "ok" : unknown.error.name).toBe("not_found");
    expect((await home.fetch(`/v1/blobs/${loc}`)).status).toBe(401);
    expect((await home.fetch(`/v1/blobs/not-a-locator`, { headers: bearer(p.cap) })).status).toBe(404);
    expect((await home.fetch(`/v1/blobs/${loc}`, { method: "PATCH", headers: bearer(p.cap) })).status).toBe(400);
  });

  it("PC-S07 only the owner capability deletes; deleting is idempotent and frees the space", async () => {
    const p = await put(rand(1200));
    const loc = blobs.locator(p.cap);
    const read = okOf(p.result).readCap;
    const denied = await blobs.remove(home, loc, read);
    expect(denied.ok ? "ok" : denied.error.name).toBe("unauthorized");
    expect((await blobs.get(home, loc, read)).ok).toBe(true);
    expect((await blobs.remove(home, loc, p.cap)).ok).toBe(true);
    expect((await blobs.get(home, loc, read)).ok).toBe(false);
    expect((await blobs.get(home, loc, p.cap)).ok).toBe(false);
    expect((await blobs.remove(home, loc, p.cap)).ok).toBe(true); // already gone: success-equivalent
    expect((await blobs.remove(home, toHex(rand(16)), rand(48))).ok).toBe(true); // unknown: no oracle
  });

  it("PC-S08 an object larger than max_blob_bytes is payload_too_large, even when the spool is empty", async () => {
    const r = await put(rand(4 * MIB + 1));
    expect(errName(r.result)).toBe("payload_too_large");
    expect(r.result.ok ? 0 : r.result.status).toBe(413);
    const huge = await put(rand(6 * MIB)); // beyond the body bound as well: still typed, not a reset connection
    expect(errName(huge.result)).toBe("payload_too_large");
  });

  it("PC-S09 a multi-chunk object round-trips byte for byte", async () => {
    const ct = rand(3 * MIB + 12_345);
    const p = await put(ct);
    expect(p.result.ok).toBe(true);
    const g = await blobs.get(home, blobs.locator(p.cap), p.cap);
    expect(g.ok && g.bytes.length).toBe(ct.length);
    expect(g.ok && toHex(await blobCiphertextDigest(g.bytes))).toBe(toHex(await blobCiphertextDigest(ct)));
  });

  it("PC-S10 retention: 7 days by default, the sender's shorter choice honoured, a longer or zero one refused; an expired blob is `expired`", async () => {
    const now = Math.floor(Date.now() / 1000);
    const short = await put(rand(300), { retentionSeconds: 3600 });
    const e = okOf(short.result).expiresAt;
    expect(e).toBeGreaterThan(now + 3500);
    expect(e).toBeLessThan(now + 3700);
    for (const retentionSeconds of [0, 14 * 86400 + 1]) expect(errName((await put(rand(300), { retentionSeconds })).result)).toBe("invalid_request");
    expect((await put(rand(300), { retentionSeconds: 14 * 86400 })).result.ok).toBe(true);
    const fleeting = await put(rand(300), { retentionSeconds: 1 });
    await new Promise((r) => setTimeout(r, 2200));
    const g = await blobs.get(home, blobs.locator(fleeting.cap), fleeting.cap);
    expect(g.ok ? "ok" : g.error.name).toBe("expired");
    expect(g.status).toBe(410);
  });

  it("PC-S11 an exact retry returns the original answer (same read capability); a different object under the same locator or request id is invalid_request", async () => {
    const ct = rand(700);
    const rid = rand(16);
    const a = await put(ct, { requestId: rid });
    const b = await put(ct, { requestId: rid, cap: a.cap });
    const c = await put(ct, { cap: a.cap }); // a new request id for the same operation: the same object
    expect(toHex(okOf(b.result).readCap)).toBe(toHex(okOf(a.result).readCap));
    expect(toHex(okOf(c.result).readCap)).toBe(toHex(okOf(a.result).readCap));
    expect(errName((await put(rand(700), { cap: a.cap })).result)).toBe("invalid_request"); // other bytes, same locator
    expect(errName((await put(rand(700), { requestId: rid })).result)).toBe("invalid_request"); // same request id, other operation
    const squat = newCapWithPrefix(a.cap);
    expect(errName((await put(ct, { cap: squat })).result)).toBe("invalid_request"); // someone else's capability for a used locator
  });

  it("PC-S12 the spool is bounded in bytes and count with typed quota errors; a retry still works when full; deleting frees room", async () => {
    const h = await startHome({ vars: { OM_ADMISSION: await admissionHash(secret) } });
    try {
      const made: { cap: Uint8Array; ct: Uint8Array }[] = [];
      for (let i = 0; i < 8; i++) { const ct = rand(4 * MIB); const r = await put(ct, {}, h); expect(r.result.ok, String(i)).toBe(true); made.push({ cap: r.cap, ct }); } // 32 MiB: full
      const full = await put(rand(1000), {}, h);
      expect(errName(full.result)).toBe("quota");
      expect(full.result.ok ? 0 : full.result.status).toBe(429);
      expect(full.result.ok ? undefined : toHex(full.result.error.detail ?? new Uint8Array())).toBe(toHex(quotaDetail(4, true)));
      const first = made[0];
      if (!first) throw new Error("no blob");
      expect((await put(first.ct, { cap: first.cap }, h)).result.ok).toBe(true); // an exact retry never fails for lack of room
      expect((await blobs.remove(h, blobs.locator(first.cap), first.cap)).ok).toBe(true);
      expect((await put(rand(1000), {}, h)).result.ok).toBe(true);
      // count bound: many tiny blobs
      let n = 0;
      for (; n < 80; n++) { const r = await put(rand(10), {}, h); if (!r.result.ok) { expect(errName(r.result)).toBe("quota"); break; } }
      expect(n).toBeLessThan(80);
      expect(n + 7 + 1).toBe(64); // 7 big ones kept, 1 small one after the delete, then tiny ones up to the count bound of 64
    } finally { await h.dispose(); }
  }, 120_000);

  it("PC-S13 a stored blob survives a restart of the Home, with the same capabilities; a corrupted object is detected, deleted and reported absent", async () => {
    const dir = scratch();
    try {
      const vars = { OM_ADMISSION: await admissionHash(secret) };
      let h = await startHome({ vars, persist: dir.dir });
      const ct = rand(1_500_000);
      const p = await put(ct, {}, h);
      const read = okOf(p.result).readCap;
      await h.dispose();
      h = await startHome({ vars, persist: dir.dir });
      try {
        const g = await blobs.get(h, blobs.locator(p.cap), read);
        expect(g.ok && toHex(g.bytes)).toBe(toHex(ct));
        // Same operation after restart: same answer.
        expect(toHex(okOf((await put(ct, { cap: p.cap }, h)).result).readCap)).toBe(toHex(read));
      } finally { await h.dispose(); }
    } finally { dir.remove(); }
  }, 120_000);
});

describe("PC-S14 corrupt-object detection", () => {
  it("a stored chunk that no longer matches its digest is never served: the blob is deleted and reported absent, and its space is freed", async () => {
    const { DatabaseSync } = await import("node:sqlite");
    const { readdirSync, statSync } = await import("node:fs");
    const { join } = await import("node:path");
    const dir = scratch();
    try {
      const vars = { OM_ADMISSION: await admissionHash(secret) };
      let h = await startHome({ vars, persist: dir.dir });
      const ct = rand(MIB + 500);
      const p = await put(ct, {}, h);
      expect(p.result.ok).toBe(true);
      await h.dispose();
      const files: string[] = [];
      const walk = (d: string): void => { for (const f of readdirSync(d)) { const full = join(d, f); if (statSync(full).isDirectory()) walk(full); else if (f.endsWith(".sqlite")) files.push(full); } };
      walk(dir.dir);
      let flipped = 0;
      for (const f of files) {
        const db = new DatabaseSync(f);
        try {
          const has = db.prepare("SELECT name FROM sqlite_master WHERE name = 'chunks'").get();
          if (has) { db.exec("UPDATE chunks SET bytes = x'00' || substr(bytes, 2) WHERE idx = 1"); flipped += 1; }
        } finally { db.close(); }
      }
      expect(flipped).toBe(1);
      h = await startHome({ vars, persist: dir.dir });
      try {
        const g = await blobs.get(h, blobs.locator(p.cap), p.cap);
        expect(g.ok ? "ok" : g.error.name).toBe("not_found");
        const again = await blobs.get(h, blobs.locator(p.cap), p.cap);
        expect(again.ok ? "ok" : again.error.name).toBe("not_found");
        expect((await put(ct, { cap: p.cap }, h)).result.ok).toBe(true); // the locator is free again
      } finally { await h.dispose(); }
    } finally { dir.remove(); }
  }, 60_000);
});

/** A capability with the locator of `of` but a different secret. */
const newCapWithPrefix = (of: Uint8Array): Uint8Array => new Uint8Array([...of.slice(0, 16), ...rand(32)]);
