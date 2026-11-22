// SPDX-License-Identifier: AGPL-3.0-or-later
// Upgrading a deployed Home from the release before the push wake (migrations v1 to v4, no PushHub class) to this source (migration
// v5 adds PushHub, ADR-0210). Two checks, the same method as `upgrade.test.ts`: the migration list only grows by v5 (a deployed account
// has already applied v1 to v4), and the data a v4 Home stored is still served, byte for byte, by the new Worker on the same Durable
// Object storage, which then also takes a push registration for that old queue. The v4 bytes are `__fixtures__/home-v4-worker.js`,
// the output of `wrangler deploy --dry-run` for the last source before the push wake (commit 2f6b445, built with its own wire package; sha256 pinned below; build-host paths removed).
// Miniflare does not track migration tags and no Cloudflare account is involved: the real service applying v5 to a real account is
// not covered, and is the first step of the operator rollout kit.
import { createHash } from "node:crypto";
import { readFileSync } from "node:fs";
import { join } from "node:path";
import { ADMISSION_HEADER, FEATURES, PUSH_PATHS, admissionHash, encodeQueueCreate, toBase64Url } from "@om/home-wire";
import { describe, expect, it } from "vitest";
import { scratch, startHome, type Home } from "./harness";
import { asBody, bearer, blobs, drainAll, newCaps, rand, relay, sealed } from "./kit";

const homeRoot = new URL("../", import.meta.url).pathname;
const V4_WORKER = join(homeRoot, "test/__fixtures__/home-v4-worker.js");
const V4_WORKER_SHA256 = "0ccc0ad34d60d42b55d37453d6d3c23ecc94c3a0fdb180b1b44533a4161b64af";
/** The `migrations` as released before v5: already applied on every deployed account, so they never change. */
const MIGRATIONS_V4 = [
  { tag: "v1", new_sqlite_classes: ["MailboxQueue", "KeyPool", "DescriptorStore", "IntroGate"] },
  { tag: "v2", new_sqlite_classes: ["BlobSpool"] },
  { tag: "v3", new_sqlite_classes: ["UsageMeter"] },
  { tag: "v4", new_sqlite_classes: ["RecoveryStore"] },
];

describe("migration v5 is append-only", () => {
  it("v1 to v4 are exactly what shipped, v5 only adds PushHub", () => {
    const text = readFileSync(join(homeRoot, "wrangler.jsonc"), "utf8");
    const config = JSON.parse(text.split("\n").filter((l) => !l.trim().startsWith("//")).join("\n")) as { migrations: Record<string, unknown>[] };
    expect(config.migrations.slice(0, MIGRATIONS_V4.length)).toEqual(MIGRATIONS_V4);
    expect(config.migrations[MIGRATIONS_V4.length]).toEqual({ tag: "v5", new_sqlite_classes: ["PushHub"] });
  });
});

describe("a Home deployed at migration v4, upgraded in place to v5", () => {
  it("the v4 fixture is the pinned artifact and has no push class", () => {
    const bytes = readFileSync(V4_WORKER);
    expect(createHash("sha256").update(bytes).digest("hex")).toBe(V4_WORKER_SHA256);
    expect(bytes.toString("utf8")).not.toMatch(/\bPushHub\b/);
  });

  it("keeps its queue, messages and blob, and gains alerts for the old queue", async () => {
    const secret = rand(32);
    const owner = { [ADMISSION_HEADER]: toBase64Url(secret) };
    const vars = { OM_ADMISSION: await admissionHash(secret), OM_ADMISSION_REQUIRED: "1" };
    const state = scratch();
    const features = async (h: Home) => ((await (await h.fetch("/.well-known/om-provider-capabilities")).json()) as { features: string[] }).features;
    const c = newCaps();
    const create = async (h: Home, caps = c) => {
      const r = await h.fetch("/v1/relay/queues", { method: "POST", headers: { "content-type": "application/cbor", ...owner }, body: asBody(encodeQueueCreate({ requestId: rand(16), receiveCap: caps.receive, depositCap: caps.deposit, adminCap: caps.admin })) });
      expect(r.status).toBeLessThan(300);
    };
    const blob = rand(40_000);
    try {
      // 1. The Home as v4 left it: a queue with two messages (one read and acknowledged) and a stored blob.
      let h = await startHome({ bundle: V4_WORKER, vars, persist: state.dir });
      await create(h);
      const first = sealed();
      const second = sealed();
      expect((await relay.deposit(h, c.deposit, first)).ok).toBe(true);
      expect((await relay.deposit(h, c.deposit, second)).ok).toBe(true);
      const before = await drainAll(h, c.receive);
      expect(before).toHaveLength(2);
      expect((await relay.ack(h, c.receive, [before[0]?.id ?? new Uint8Array(0)])).ok).toBe(true);
      const put = await blobs.put(h, { ciphertext: blob, admission: owner });
      expect(put.result.ok).toBe(true);
      if (!put.result.ok) return;
      const locator = blobs.locator(put.cap);
      const readCap = put.result.value.readCap;
      expect(await features(h)).not.toContain(FEATURES.relayPush);
      expect((await h.fetch(PUSH_PATHS.key)).status).toBe(404);
      await h.dispose();

      // 2. The same storage under the new Worker (PushHub is bound, as migration v5 does on a real account).
      h = await startHome({ vars, persist: state.dir });
      try {
        const after = await drainAll(h, c.receive);
        expect(after).toHaveLength(1); // the acknowledgement survived
        expect(Buffer.from(after[0]?.bytes ?? new Uint8Array(0)).equals(Buffer.from(second))).toBe(true); // byte for byte
        expect(after[0]?.seq).toBe(before[1]?.seq);
        const g = await blobs.get(h, locator, readCap);
        expect(g.ok && Buffer.from(g.bytes).equals(Buffer.from(blob))).toBe(true); // the blob is unchanged
        expect((await relay.deposit(h, c.deposit, sealed())).ok).toBe(true); // the old queue still takes messages
        await create(h, newCaps()); // and new queues can be made
        // The new feature is offered, and works for the queue that existed before the upgrade.
        expect(await features(h)).toContain(FEATURES.relayPush);
        expect((await h.fetch(PUSH_PATHS.key)).status).toBe(200);
        const reg = await h.fetch(PUSH_PATHS.subscription, { method: "PUT", headers: { "content-type": "application/json", ...bearer(c.receive), ...owner }, body: JSON.stringify({ endpoint: "https://fcm.googleapis.com/fcm/send/upgrade-test" }) });
        expect(reg.status).toBe(200);
      } finally { await h.dispose(); }

      // 3. And once more: nothing the upgrade created is lost on the next restart.
      h = await startHome({ vars, persist: state.dir });
      try { expect(await drainAll(h, c.receive)).toHaveLength(2); } finally { await h.dispose(); }
    } finally { state.remove(); }
  }, 180_000);
});
