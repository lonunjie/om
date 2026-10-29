// OPT-IN (OM_LIVE_RELAY=1, run by `pnpm --filter @om/web e2e:live`): the PRODUCTION entry (real Core, vault, HTTP adapter, built
// with VITE_OM_RELAY_URL pointing at the live reference Home, no test flags) with two fresh identities exchanging a handful
// of messages through the real Home from a real browser. It talks to a deployed service, so CI never runs it.
//
// It cleans up after itself: with the install secret read back from the page it deletes the invite descriptor and the
// device key bundles it published, drains and acknowledges whatever is left in the queues it used, and checks the
// descriptor is gone. Queues, being capability-addressed, hold no data once drained; whatever else a Home keeps is bounded
// by its own TTLs (a few small rows). Message count is deliberately tiny (three), as is the request count (asserted).
import { createHmac, randomBytes } from "node:crypto";
import { expect, test, type Page } from "@playwright/test";
import { decodeDepositRequest, decodeDrainResponse, decodeKeyPublish, encodeAckRequest, encodeDrainRequest, readBundleFacts, toBase64Url } from "@om/home-wire";
import { noPasskeys, onboard, walkSetup } from "./helpers.ts";

test.skip(process.env["OM_LIVE_RELAY"] !== "1", "opt-in: set OM_LIVE_RELAY=1 (this talks to the live Home)");
test.use({ baseURL: "http://localhost:4178" });

const HOME = (process.env["OM_LIVE_RELAY_URL"] ?? "https://om-home-lonunjie.majal.workers.dev").replace(/\/+$/, "");
const MAX_HOME_REQUESTS = 1500; // free-tier sanity: a run is a few hundred polls at most

type Who = "alice" | "bob";
async function asIdentity(page: Page, state: { who: Who }, who: Who): Promise<void> {
  if (state.who === who) return;
  await page.getByRole("button", { name: "Settings" }).waitFor();
  const other = page.getByRole("list", { name: "Identities" }).getByRole("button").and(page.locator(":not([aria-current])"));
  await other.click();
  await expect(page.getByRole("list", { name: "Identities" }).getByRole("button").and(page.locator("[aria-current]"))).toHaveCount(1);
  state.who = who;
}

interface Hit { readonly url: string; readonly method: string; readonly body: Buffer | null; readonly status: number; readonly ms: number; readonly at: number }

const median = (xs: number[]): number => { const s = [...xs].sort((a, b) => a - b); return s[Math.floor(s.length / 2)] ?? 0; };
const p95 = (xs: number[]): number => { const s = [...xs].sort((a, b) => a - b); return s[Math.min(s.length - 1, Math.floor(s.length * 0.95))] ?? 0; };

const hmac = (secret: Buffer, label: string, data: Uint8Array): Uint8Array => new Uint8Array(createHmac("sha256", secret).update(label).update(data).digest());
const bearer = (cap: Uint8Array): Record<string, string> => ({ authorization: `Bearer ${toBase64Url(cap)}` });
const fromHexPath = (s: string): Uint8Array => Uint8Array.from(s.match(/../g) ?? [], (h) => parseInt(h, 16));

/** What the run left at the Home, and removal of it, using only capabilities this install owns (derived from its secret). */
async function cleanUp(page: Page, hits: readonly Hit[]): Promise<{ descriptors: number; bundles: number; queues: number; leftover: number; descriptorGone: boolean }> {
  const stored = await page.evaluate(() => new Promise<number[] | null>((resolve) => {
    const open = indexedDB.open("om-provider-secrets", 1);
    open.onerror = () => { resolve(null); };
    open.onsuccess = () => {
      const q = open.result.transaction("secrets", "readonly").objectStore("secrets").get("install-secret-v1");
      q.onsuccess = () => { resolve(q.result instanceof Uint8Array ? Array.from(q.result) : null); };
      q.onerror = () => { resolve(null); };
    };
  }));
  const out = { descriptors: 0, bundles: 0, queues: 0, leftover: 0, descriptorGone: true };
  if (stored === null) throw new Error("cleanup: the install secret could not be read; run again after removing the data by hand");
  const secret = Buffer.from(stored);
  const call = (method: string, path: string, init: { body?: Uint8Array; bearer?: Uint8Array } = {}): Promise<Response> =>
    fetch(`${HOME}${path}`, { method, headers: { ...(init.body ? { "content-type": "application/cbor" } : {}), ...(init.bearer ? bearer(init.bearer) : {}) }, ...(init.body ? { body: init.body as Uint8Array<ArrayBuffer> } : {}) });

  // Invite descriptors.
  const descriptorIds = new Set(hits.filter((h) => h.method === "PUT" && /\/v1\/discovery\/descriptors\/[0-9a-f]{32}$/.test(h.url)).map((h) => h.url.slice(-32)));
  for (const id of descriptorIds) {
    const r = await call("DELETE", `/v1/discovery/descriptors/${id}`, { bearer: hmac(secret, "om-descriptor-publish-v0", fromHexPath(id)) });
    if (r.ok) out.descriptors++;
    out.descriptorGone &&= (await call("GET", `/v1/discovery/descriptors/${id}`)).status === 404;
  }
  // Device key bundles (every generation published).
  const published = new Map<string, Set<number>>();
  for (const h of hits.filter((x) => x.method === "PUT" && /\/v1\/keys\/bundles\/[0-9a-f]{32}$/.test(x.url) && x.body)) {
    const dev = h.url.slice(-32);
    const gen = readBundleFacts(decodeKeyPublish(new Uint8Array(h.body as Buffer)).bundle)?.generation;
    if (gen !== undefined) published.set(dev, (published.get(dev) ?? new Set()).add(gen));
  }
  for (const [dev, gens] of published) {
    for (const g of gens) {
      const r = await call("DELETE", `/v1/keys/bundles/${dev}/generations/${String(g)}`, { bearer: hmac(secret, "om-key-publish-v0", fromHexPath(dev)) });
      if (r.ok) out.bundles++;
    }
  }
  // Queues this install owns: every deposit capability is `queue_id(16) || mailbox(16)`. Drain and acknowledge what remains.
  const mailboxes = new Map<string, Uint8Array>();
  for (const h of hits.filter((x) => x.method === "POST" && x.url.endsWith("/v1/relay/deposit") && x.body)) {
    const cap = decodeDepositRequest(new Uint8Array(h.body as Buffer)).cap;
    if (cap.length === 32) mailboxes.set(Buffer.from(cap).toString("hex"), cap.slice(16));
  }
  for (const [capHex, mailbox] of mailboxes) {
    const queue = Uint8Array.from(Buffer.from(capHex, "hex").subarray(0, 16));
    const receive = new Uint8Array([...queue, ...hmac(secret, "om-relay-receive-v0", mailbox)]);
    out.queues++;
    for (let page = 0; page < 5; page++) {
      const res = await call("POST", "/v1/relay/drain", { body: encodeDrainRequest({ requestId: randomBytes(16), cap: receive, after: 0 }) });
      if (!res.ok) break; // a queue this install never claimed answers like a full one: nothing to drain
      const drained = decodeDrainResponse(new Uint8Array(await res.arrayBuffer()));
      if (drained.items.length === 0) break;
      out.leftover += drained.items.length;
      await call("POST", "/v1/relay/ack", { body: encodeAckRequest({ requestId: randomBytes(16), cap: receive, itemIds: drained.items.map((i) => i.id) }) });
    }
  }
  return out;
}

/** Everything this run sent to the Home; cleanup (also after a failure or a timeout) works from it. */
const hits: Hit[] = [];
let summary: Record<string, unknown> = {};

test.afterEach(async ({ page }) => {
  // Always attempt cleanup, also after a failure, so a broken run does not leave data at the Home.
  const cleaned = await cleanUp(page, hits).catch((e: unknown) => ({ error: e instanceof Error ? e.message : String(e) }));
  summary = { ...summary, cleanup: cleaned };
  console.log(`LIVE-RELAY-SUMMARY ${JSON.stringify(summary)}`);
  const c = cleaned as { descriptorGone?: boolean; error?: string };
  expect(c.error).toBeUndefined();
  expect(c.descriptorGone).toBe(true);
});

test("two fresh identities exchange messages through the LIVE Home with the production build", async ({ page, browserName }) => {
  page.on("requestfinished", (r) => {
    if (!r.url().startsWith(HOME)) return;
    void r.response().then((res) => {
      const t = r.timing();
      hits.push({ url: r.url(), method: r.method(), body: r.postDataBuffer(), status: res?.status() ?? 0, ms: Math.max(0, t.responseEnd), at: Date.now() });
    });
  });
  const consoleErrors: string[] = [];
  page.on("console", (m) => { if (m.type() === "error" && !/Failed to load resource/.test(m.text())) consoleErrors.push(m.text()); });
  await noPasskeys(page);

  const at = { accept: 0, relationship: 0 };
  const t0 = Date.now();
  const who = { who: "alice" as Who };
  {
    await onboard(page, { stopAt: "invite" });
    // The production entry, connected to the live Home by build configuration: no loopback banner, not "unconnected".
    await expect(page.getByText("E2E LOCAL RELAY")).toHaveCount(0);
    await expect(page.getByText("Not connected to a provider")).toHaveCount(0);
    await page.getByRole("button", { name: "Show as QR code" }).click();
    const link = (await page.locator("code[translate=no]").first().textContent())?.trim() ?? "";
    expect(link).toMatch(/^om:invite\/[A-Za-z0-9_-]+$/);
    await page.getByRole("button", { name: "Skip for now" }).click();
    await expect(page.getByRole("button", { name: "Settings" })).toBeVisible();

    await page.getByRole("button", { name: "Add identity" }).click();
    await walkSetup(page, { stopAt: "invite" });
    await page.getByLabel("Invitation link").fill(link);
    at.accept = Date.now();
    await page.getByRole("button", { name: "Accept invitation" }).click();
    await expect(page.getByRole("button", { name: "Settings" })).toBeVisible();

    // Bob's request reaches Alice through the Home; she accepts.
    await page.getByRole("region", { name: "Message request" }).getByRole("button", { name: "Accept", exact: true }).click();
    await expect(page.getByRole("option")).toHaveCount(1, { timeout: 60_000 });
    at.relationship = Date.now();
    // The first message can only be queued once the introduction has completed: wait for Alice's deposit after her key fetch.
    const afterFetch = (): boolean => {
      const at = hits.findIndex((w) => w.url.endsWith("/v1/keys/fetch"));
      return at >= 0 && hits.slice(at).some((w) => w.url.endsWith("/v1/relay/deposit"));
    };
    await expect.poll(afterFetch, { timeout: 60_000 }).toBe(true);
    await page.waitForTimeout(1500);

    const latencies: number[] = [];
    const send = async (from: Who, text: string): Promise<number> => {
      await asIdentity(page, who, from);
      await page.getByRole("option").click();
      await page.getByRole("textbox", { name: /message/i }).fill(text);
      const sentAt = Date.now();
      await page.getByRole("button", { name: /send/i }).click();
      await expect(page.getByText(text)).toBeVisible();
      return sentAt;
    };
    const receive = async (at: Who, text: string, sentAt: number): Promise<void> => {
      await asIdentity(page, who, at);
      await page.getByRole("option").click();
      await expect(page.getByText(text)).toBeVisible({ timeout: 60_000 });
      latencies.push(Date.now() - sentAt);
    };
    const s1 = await send("alice", "live check one");
    await receive("bob", "live check one", s1);
    const s2 = await send("bob", "live check two");
    await receive("alice", "live check two", s2);

    // A reload (both identities unlock again) and one more message: the install keeps its right to its own mailboxes.
    await page.reload();
    for (let n = 0; n < 2; n++) {
      await expect(page.getByRole("heading", { name: /unlock/i }).first()).toBeVisible({ timeout: 60_000 });
      await page.getByRole("button", { name: "Unlock", exact: true }).click();
    }
    await expect(page.getByRole("button", { name: "Settings" })).toBeVisible({ timeout: 60_000 });
    who.who = "alice";
    const s3 = await send("alice", "live check three");
    await receive("bob", "live check three", s3);

    // The Home saw only API paths on its own origin, no capability or query in a URL, and no plaintext.
    expect(hits.length).toBeGreaterThan(15);
    expect(hits.length, "free-tier sanity").toBeLessThan(MAX_HOME_REQUESTS);
    for (const w of hits) {
      expect(w.url).not.toMatch(/[?#]/);
      const body = w.body?.toString("latin1") ?? "";
      for (const secret of ["live check one", "live check two", "live check three"]) expect(body).not.toContain(secret);
    }
    const paths = new Set(hits.filter((w) => w.method !== "OPTIONS").map((w) => new URL(w.url).pathname.replace(/[0-9a-f]{32}/g, ":id")));
    for (const p of ["/v1/relay/deposit", "/v1/relay/drain", "/v1/relay/ack", "/v1/relay/queues", "/v1/keys/fetch", "/v1/keys/bundles/:id", "/v1/discovery/descriptors/:id"]) expect(paths, p).toContain(p);
    expect(consoleErrors).toEqual([]);

    const byEndpoint = (suffix: string): number[] => hits.filter((h) => h.url.endsWith(suffix) && h.status < 400).map((h) => h.ms);
    summary = {
      browser: browserName, home: HOME,
      messages: 3, homeRequests: hits.length, failedRequests: hits.filter((h) => h.status >= 400).length,
      firstContactMs: at.relationship - at.accept,
      sendToVisibleMs: latencies, // click Send in one identity -> text visible in the other (includes the identity switch and a <=3 s poll)
      requestMs: { deposit: { median: median(byEndpoint("/v1/relay/deposit")), p95: p95(byEndpoint("/v1/relay/deposit")) }, drain: { median: median(byEndpoint("/v1/relay/drain")), p95: p95(byEndpoint("/v1/relay/drain")) } },
      totalMs: Date.now() - t0,
    };
  }
});
