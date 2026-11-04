// SPDX-License-Identifier: AGPL-3.0-or-later
// Upgrading a deployed relay from release 0.1.1 (four Durable Object classes, migration v1) to a release that adds the blob spool
// (migration v2). Two things are checked: the migration list is append-only (a deployed account has already applied v1, so v1 must not
// move and v2 may only add a class), and the data a 0.1.1 relay stored is still served, unchanged, by the new Worker on the same
// Durable Object storage. Miniflare does not track migration tags, so the first check reads the configuration and the second runs
// the released 0.1.1 bytes (`__fixtures__/relay-0.1.1-worker.js`, sha256 pinned below) and then the new bytes on one persisted state.
// No Cloudflare account, token or network is involved; the real service applying v2 to a real account is not covered (see the
// relay deploy template report, "Untested against the real service").
import { createHash } from "node:crypto";
import { readFileSync } from "node:fs";
import { join } from "node:path";
import { ADMISSION_HEADER, admissionHash, encodeQueueCreate, toBase64Url } from "@om/home-wire";
import { describe, expect, it } from "vitest";
import { scratch, startHome, type Home } from "./harness";
import { asBody, blobs, drainAll, newCaps, rand, relay, sealed } from "./kit";

const homeRoot = new URL("../", import.meta.url).pathname;
const OLD_WORKER = join(homeRoot, "test/__fixtures__/relay-0.1.1-worker.js");
const OLD_WORKER_SHA256 = "8481748a2ef898627d1950c5be949f33ba7d758e37c0fb2436dfeca01325d8e6"; // om-relay 0.1.0 and 0.1.1 relay/worker.js
/** The `migrations` of the published 0.1.1 template (`relay-manifest.json` config.wrangler.migrations): already applied on every deployed account. */
const MIGRATIONS_0_1_1 = [{ tag: "v1", new_sqlite_classes: ["MailboxQueue", "KeyPool", "DescriptorStore", "IntroGate"] }];

interface Migration { tag: string; new_sqlite_classes?: string[]; deleted_classes?: string[]; renamed_classes?: unknown[]; transferred_classes?: unknown[]; new_classes?: string[] }
interface Config { durable_objects: { bindings: { name: string; class_name: string }[] }; migrations: Migration[] }
const readConfig = (file: string, comments: boolean): Config => {
  const text = readFileSync(join(homeRoot, file), "utf8");
  return JSON.parse(comments ? text.split("\n").filter((l) => !l.trim().startsWith("//")).join("\n") : text) as Config;
};

describe("migrations are append-only (upgrade from 0.1.1)", () => {
  for (const [file, comments] of [["wrangler.jsonc", true], ["release/template/wrangler.json", false]] as const) {
    it(`${file}: v1 is exactly what 0.1.1 shipped, v2 only adds BlobSpool, a fresh deploy applies v1 then v2`, () => {
      const m = readConfig(file, comments).migrations;
      expect(m.slice(0, MIGRATIONS_0_1_1.length)).toEqual(MIGRATIONS_0_1_1); // the applied history is unchanged
      expect(m.map((x) => x.tag)).toEqual(["v1", "v2"]); // ordered, unique
      expect(m[1]).toEqual({ tag: "v2", new_sqlite_classes: ["BlobSpool"] });
      for (const x of m) for (const destructive of ["deleted_classes", "renamed_classes", "transferred_classes", "new_classes"] as const) expect(x, `${x.tag} ${destructive}`).not.toHaveProperty(destructive); // nothing that drops, moves or makes a non-SQLite class
    });

    it(`${file}: every bound Durable Object class is created by exactly one migration`, () => {
      const c = readConfig(file, comments);
      const created = c.migrations.flatMap((x) => x.new_sqlite_classes ?? []);
      expect(new Set(created).size).toBe(created.length);
      expect([...created].sort()).toEqual(c.durable_objects.bindings.map((b) => b.class_name).sort());
    });
  }
});

describe("a relay deployed from 0.1.1, upgraded in place", () => {
  it("the 0.1.1 fixture is the published release (so the test really starts from what users have)", () => {
    expect(createHash("sha256").update(readFileSync(OLD_WORKER)).digest("hex")).toBe(OLD_WORKER_SHA256);
  });

  it("keeps its queues and stored messages, and gains the blob spool without losing anything", async () => {
    const secret = rand(32);
    const owner = { [ADMISSION_HEADER]: toBase64Url(secret) };
    const vars = { OM_ADMISSION: await admissionHash(secret), OM_ADMISSION_REQUIRED: "1" }; // the template's configuration
    const state = scratch();
    const create = async (h: Home) => {
      const c = newCaps();
      const r = await h.fetch("/v1/relay/queues", { method: "POST", headers: { "content-type": "application/cbor", ...owner }, body: asBody(encodeQueueCreate({ requestId: rand(16), receiveCap: c.receive, depositCap: c.deposit, adminCap: c.admin })) });
      expect(r.status).toBeLessThan(300);
      return c;
    };
    const features = async (h: Home) => ((await (await h.fetch("/.well-known/om-provider-capabilities")).json()) as { features: string[] }).features;
    try {
      // 1. The relay as 0.1.1 left it: a queue holding two messages, one of them already read and acknowledged.
      let h = await startHome({ bundle: OLD_WORKER, vars, persist: state.dir });
      const c = await create(h);
      const first = sealed();
      const second = sealed();
      expect((await relay.deposit(h, c.deposit, first)).ok).toBe(true);
      expect((await relay.deposit(h, c.deposit, second)).ok).toBe(true);
      const before = await drainAll(h, c.receive);
      expect(before).toHaveLength(2);
      expect((await relay.ack(h, c.receive, [before[0]?.id ?? new Uint8Array(0)])).ok).toBe(true);
      expect(await features(h)).not.toContain("om.blob.spool.v1");
      await h.dispose();

      // 2. The same storage under the new Worker (the new class is bound, as migration v2 does on a real account).
      h = await startHome({ vars, persist: state.dir });
      try {
        expect(await features(h)).toContain("om.blob.spool.v1");
        const after = await drainAll(h, c.receive);
        expect(after).toHaveLength(1); // the acknowledgement survived
        expect(Buffer.from(after[0]?.bytes ?? new Uint8Array(0)).equals(Buffer.from(second))).toBe(true); // byte for byte
        expect(after[0]?.seq).toBe(before[1]?.seq);
        expect((await relay.deposit(h, c.deposit, sealed())).ok).toBe(true); // the old queue still takes new messages
        expect((await create(h)).prefix).toBeDefined(); // and new queues can be made
        const ct = rand(60_000);
        const put = await blobs.put(h, { ciphertext: ct, admission: owner });
        expect(put.result.ok).toBe(true); // the new class works on the same account
        if (put.result.ok) {
          const g = await blobs.get(h, blobs.locator(put.cap), put.result.value.readCap);
          expect(g.ok && Buffer.from(g.bytes).equals(Buffer.from(ct))).toBe(true);
        }
      } finally { await h.dispose(); }

      // 3. And once more: nothing the upgrade created is lost on the next restart.
      h = await startHome({ vars, persist: state.dir });
      try { expect(await drainAll(h, c.receive)).toHaveLength(2); } finally { await h.dispose(); }
    } finally { state.remove(); }
  }, 180_000);
});
