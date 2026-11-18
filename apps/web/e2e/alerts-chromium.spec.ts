// Settings > Alerts (ADR-0210) in the production entry, connected to a locked local Home (8789: it holds the hash of the owner's secret, so it offers
// the push wake). The opt-in flow end to end: nothing registers and no permission is asked before the person turns it on; turning it on subscribes,
// registers the endpoint at the Home with the owner's admission (nothing in a URL); a reload renews the registration; turning it off removes it; at
// Maximum Privacy the switch asks first and registers nothing until confirmed.
//
// What this cannot test: delivery. The browser's push subscription (PushManager) and the notification permission are replaced by a small stand-in
// here, because a CI browser has no vendor push service. A real push, through Google, Mozilla or Apple, to a real device is a manual check
// (docs/reports/2026-10-28-relay-status.md). The Home's side of delivery (an empty, signed push to a local service) is tested in
// infra/cloudflare-home/test/push.test.ts against the real Worker bundle.
import { expect, test, type Page } from "@playwright/test";
import { E2E_ADMISSION_SECRET, LOCKED_RELAY } from "./admission-fixture.ts";
import { chooseOwnRelay, noPasskeys, reopen, walkSetup } from "./helpers.ts";

test.use({ baseURL: "http://localhost:4179" });

const hex = (b: Buffer): string => b.toString("hex");

async function seedPending(page: Page, secretHex: string): Promise<void> {
  await page.evaluate((h) => new Promise<void>((resolve, reject) => {
    const open = indexedDB.open("om-provider-secrets", 1);
    open.onupgradeneeded = () => { open.result.createObjectStore("secrets"); };
    open.onerror = () => { reject(new Error("seed failed")); };
    open.onsuccess = () => {
      const tx = open.result.transaction("secrets", "readwrite");
      tx.objectStore("secrets").put(Uint8Array.from(h.match(/../g) ?? [], (b) => parseInt(b, 16)), "admission-pending-v1");
      tx.oncomplete = () => { open.result.close(); resolve(); };
    };
  }), secretHex);
}

/** A stand-in for the browser's push subscription and notification permission. It keeps its state in sessionStorage so a reload sees the same subscription. */
const standIn = (): void => {
  const KEY = "e2e-push";
  const read = (): { sub: string | null; perm: string; asked: number; subscribed: number } => { try { return JSON.parse(sessionStorage.getItem(KEY) ?? "") as never; } catch { return { sub: null, perm: "default", asked: 0, subscribed: 0 }; } };
  const write = (v: ReturnType<typeof read>): void => { sessionStorage.setItem(KEY, JSON.stringify(v)); };
  const sub = (): { endpoint: string; unsubscribe: () => Promise<boolean> } | null => {
    const s = read().sub;
    return s === null ? null : { endpoint: s, unsubscribe: () => { write({ ...read(), sub: null }); return Promise.resolve(true); } };
  };
  const pm = {
    getSubscription: () => Promise.resolve(sub()),
    subscribe: (o: { userVisibleOnly: boolean; applicationServerKey: Uint8Array }) => {
      if (!o.userVisibleOnly || o.applicationServerKey.length !== 65) return Promise.reject(new Error("bad subscribe"));
      const s = read();
      write({ ...s, sub: `https://fcm.googleapis.com/fcm/send/e2e-${Math.random().toString(36).slice(2)}`, subscribed: s.subscribed + 1 });
      return Promise.resolve(sub());
    },
  };
  Object.defineProperty(navigator.serviceWorker, "ready", { configurable: true, get: () => Promise.resolve({ pushManager: pm }) });
  (window as unknown as { PushManager: unknown }).PushManager = (window as unknown as { PushManager?: unknown }).PushManager ?? function PushManager() { return undefined; };
  Object.defineProperty(window, "Notification", {
    configurable: true,
    value: { get permission() { return read().perm; }, requestPermission: () => { const s = read(); write({ ...s, perm: "granted", asked: s.asked + 1 }); return Promise.resolve("granted"); } },
  });
};
const stand = (page: Page): Promise<{ sub: string | null; perm: string; asked: number; subscribed: number }> =>
  page.evaluate(() => JSON.parse(sessionStorage.getItem("e2e-push") ?? '{"sub":null,"perm":"default","asked":0,"subscribed":0}') as never);

test("the alerts switch: off until turned on, registers at the Home, renews on reload, removes on turning off, asks first at Maximum Privacy", async ({ page }) => {
  test.setTimeout(240_000);
  await noPasskeys(page);
  await page.addInitScript(standIn);
  const wire: { method: string; url: string; admission: boolean; body: string | null; status: number }[] = [];
  page.on("response", (r) => {
    if (!r.url().startsWith(`${LOCKED_RELAY}/v1/push`)) return;
    const q = r.request();
    wire.push({ method: q.method(), url: r.url(), admission: q.headers()["om-admission"] !== undefined, body: q.postData(), status: r.status() });
  });

  const firstRun = async (p: Page): Promise<void> => {
    await p.goto("/");
    await seedPending(p, hex(E2E_ADMISSION_SECRET));
    await p.getByRole("button", { name: "Get started" }).click();
    await chooseOwnRelay(p);
    await p.getByLabel("Address of your delivery service").fill(LOCKED_RELAY);
    await p.getByRole("button", { name: "Check and continue" }).click();
    await expect(p.getByRole("button", { name: "Start new" })).toBeVisible({ timeout: 30_000 });
    await walkSetup(p, { stopAt: "invite" });
  };
  // A second person (another browser profile) makes an invitation, so that this one has a conversation and therefore a mailbox at the Home.
  const other = await (await page.context().browser()?.newContext({ baseURL: "http://localhost:4179" }))?.newPage();
  if (other === undefined) throw new Error("no second browser profile");
  await noPasskeys(other);
  await firstRun(other);
  await other.getByRole("button", { name: "Show my QR" }).click();
  const link = ((await other.locator("code[translate=no]").first().textContent()) ?? "").trim();
  expect(link).toMatch(/^https?:\/\/[^/]+\/#invite\/[A-Za-z0-9_-]+$/);

  const all: string[] = [];
  page.on("request", (r) => { if (r.url().startsWith(LOCKED_RELAY)) all.push(`${r.method()} ${new URL(r.url()).pathname}`); });
  await firstRun(page);
  await page.getByText("Have an invite?").click();
  await page.getByLabel("Invitation link").fill(link);
  await page.getByRole("button", { name: "Accept invitation" }).click();
  await expect(page.getByRole("button", { name: "Settings" })).toBeVisible();
  await expect.poll(() => all.includes("POST /v1/relay/queues"), { timeout: 60_000 }).toBe(true);

  // Off by default: nothing was asked and nothing registered while the page was only used.
  expect(wire).toEqual([]);
  expect((await stand(page)).asked).toBe(0);

  await page.goto("/settings/alerts");
  await reopen(page);
  await expect(page.getByRole("heading", { name: "Alerts" }).first()).toBeVisible();
  await expect(page.getByText("The notice only says New message.", { exact: false })).toBeVisible();
  await expect(page.getByText("Add to Home Screen", { exact: false })).toBeVisible();
  await expect(page.getByText("Alerts are off.")).toBeVisible();
  expect((await stand(page)).asked).toBe(0);
  expect(wire).toEqual([]);

  // On: permission asked from the tap, a subscription made for the Home's key, the endpoint registered with the owner's admission.
  await page.getByRole("button", { name: "Turn on alerts" }).click();
  await expect(page.getByText("Alerts are on for this device.")).toBeVisible({ timeout: 30_000 });
  const after = await stand(page);
  expect(after.asked).toBe(1);
  expect(after.subscribed).toBe(1);
  await expect.poll(() => wire.filter((w) => w.method === "PUT").length, { timeout: 15_000 }).toBeGreaterThan(0);
  const puts = wire.filter((w) => w.method === "PUT");
  for (const w of puts) {
    expect(w.status).toBe(200);
    expect(w.admission).toBe(true);
    expect(new URL(w.url).search).toBe("");
    expect(JSON.parse(w.body ?? "")).toEqual({ endpoint: after.sub });
  }
  expect(wire.filter((w) => w.method === "GET").every((w) => new URL(w.url).pathname === "/v1/push/key")).toBe(true);

  // A reload renews the registration (it lapses after thirty days) and keeps the switch on without asking again.
  const before = puts.length;
  await page.goto("/settings/alerts");
  await reopen(page);
  await expect(page.getByText("Alerts are on.")).toBeVisible();
  await expect.poll(() => wire.filter((w) => w.method === "PUT").length, { timeout: 15_000 }).toBeGreaterThan(before);
  expect((await stand(page)).asked).toBe(1);

  // Off: the Home is told, the browser subscription is dropped, nothing is renewed after.
  await page.getByRole("button", { name: "Turn off alerts" }).click();
  await expect(page.getByText("Alerts are off for this device.")).toBeVisible();
  await expect.poll(() => wire.filter((w) => w.method === "DELETE").length, { timeout: 15_000 }).toBeGreaterThan(0);
  for (const w of wire.filter((w) => w.method === "DELETE")) { expect(w.admission).toBe(false); expect(w.status).toBe(204); }
  expect((await stand(page)).sub).toBeNull();

  // Maximum Privacy: the switch asks first, says the cost, and registers nothing until confirmed.
  await page.goto("/settings/protection");
  await reopen(page);
  await page.getByRole("button", { name: "Use Maximum Privacy" }).click();
  await expect(page.getByText("Now using Maximum Privacy.")).toBeVisible();
  const puts2 = wire.filter((w) => w.method === "PUT").length;
  await page.getByRole("button", { name: "Settings" }).click();
  await page.goto("/settings/alerts");
  await reopen(page);
  await page.getByRole("button", { name: "Turn on alerts" }).click();
  await expect(page.getByText("Alerts let your browser's push service see the time of each wake.", { exact: false })).toBeVisible();
  await page.getByRole("button", { name: "Keep off" }).click();
  await expect(page.getByText("Alerts are off.")).toBeVisible();
  expect(wire.filter((w) => w.method === "PUT").length).toBe(puts2);
  await page.getByRole("button", { name: "Turn on alerts" }).click();
  await page.getByRole("button", { name: "Turn on anyway" }).click();
  await expect(page.getByText("Alerts are on for this device.")).toBeVisible({ timeout: 30_000 });
  await expect.poll(() => wire.filter((w) => w.method === "PUT").length, { timeout: 15_000 }).toBeGreaterThan(puts2);
});
