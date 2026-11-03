// First run against a bundle that carries no delivery service (ADR-0186) and a LOCKED local Home (it holds only the hash of the owner's admission
// secret, as a person's own deployment does): the person picks "Set up my own, free", sees the setup value, follows the link in a new tab, comes
// back, types the address, and first run goes on connected. The setup value shown is the hash of the secret this browser stored, and the app then
// creates its mailboxes at the locked Home with that secret, which only works if the secret was kept for exactly that address.
import { createHash } from "node:crypto";
import { expect, test, type Page } from "@playwright/test";
import { E2E_ADMISSION_HASH, E2E_ADMISSION_SECRET, LOCKED_RELAY } from "./admission-fixture.ts";
import { noPasskeys, walkSetup } from "./helpers.ts";

test.use({ baseURL: "http://localhost:4179" });

const RELAY = LOCKED_RELAY;
const DEPLOY = "https://deploy.example.test/start";

/** Reads one entry of the adapter's secret store (a tiny IndexedDB) from the page, as hex, or null. */
async function secret(page: Page, name: string): Promise<string | null> {
  return page.evaluate((key) => new Promise<string | null>((resolve) => {
    const open = indexedDB.open("om-provider-secrets", 1);
    open.onerror = () => { resolve(null); };
    open.onsuccess = () => {
      const db = open.result;
      if (!db.objectStoreNames.contains("secrets")) { resolve(null); return; }
      const q = db.transaction("secrets", "readonly").objectStore("secrets").get(key);
      q.onsuccess = () => { const v: unknown = q.result; resolve(v instanceof Uint8Array && v.length > 0 ? Array.from(v, (b) => b.toString(16).padStart(2, "0")).join("") : null); };
      q.onerror = () => { resolve(null); };
    };
  }), name);
}

test("set up my own: setup value, new-tab link, typed address, then first run continues connected", async ({ page, context }) => {
  await noPasskeys(page);
  // The vendor's page is not part of this hermetic run: whatever opens at the setup link is a blank page.
  await context.route(DEPLOY, (route) => route.fulfill({ status: 200, contentType: "text/html", body: "<title>vendor setup</title>" }));
  const wire: string[] = [];
  const refused: string[] = [];
  page.on("request", (r) => { if (r.url().startsWith(RELAY)) wire.push(`${r.method()} ${new URL(r.url()).pathname}`); });
  page.on("response", (r) => { if (r.url().startsWith(RELAY) && (r.status() === 401 || r.status() === 403)) refused.push(`${String(r.status())} ${new URL(r.url()).pathname}`); });

  await page.goto("/");
  // As if the device had made its secret earlier: seed the adapter's (empty) secret store, so the setup value is the hash this locked Home was started with.
  await page.evaluate((hex) => new Promise<void>((resolve, reject) => {
    const open = indexedDB.open("om-provider-secrets", 1);
    open.onupgradeneeded = () => { open.result.createObjectStore("secrets"); };
    open.onerror = () => { reject(new Error("seed failed")); };
    open.onsuccess = () => {
      const tx = open.result.transaction("secrets", "readwrite");
      tx.objectStore("secrets").put(Uint8Array.from(hex.match(/../g) ?? [], (b) => parseInt(b, 16)), "admission-pending-v1");
      tx.oncomplete = () => { open.result.close(); resolve(); };
    };
  }), E2E_ADMISSION_SECRET.toString("hex"));
  await page.getByRole("button", { name: "Get started" }).click();
  await expect(page.getByRole("heading", { name: "OM is different" })).toBeVisible();
  expect(wire).toEqual([]); // nothing is asked of any address before the person has typed one
  await page.getByRole("button", { name: "Set up my own, free" }).click();

  const shown = (await page.getByText(/^[0-9a-f]{64}$/).textContent()) ?? "";
  expect(shown).toBe(E2E_ADMISSION_HASH);
  const pending = await secret(page, "admission-pending-v1");
  expect(pending).not.toBeNull();
  expect(createHash("sha256").update("OM-RELAY-ADMISSION-v1").update(Buffer.from(pending ?? "", "hex")).digest("hex")).toBe(shown);

  // The link opens in a new tab and does not hand that tab this page.
  const link = page.getByRole("link", { name: /Open Cloudflare setup/ });
  const [tab] = await Promise.all([context.waitForEvent("page"), link.click()]);
  await tab.waitForLoadState();
  expect(tab.url()).toBe(DEPLOY);
  expect(await tab.evaluate(() => window.opener as unknown)).toBeNull();
  await tab.close();

  // Back here: a typo is explained, the right address is probed and stored, and first run continues.
  await page.getByLabel("Address of your delivery service").fill("http://127.0.0.1:1");
  await page.getByRole("button", { name: "Check and continue" }).click();
  await expect(page.getByRole("alert")).toBeVisible();
  await page.getByLabel("Address of your delivery service").fill(RELAY);
  await page.getByRole("button", { name: "Check and continue" }).click();
  await expect(page.getByLabel("Your name")).toBeVisible({ timeout: 30_000 }); // the protect step: first run goes on without the landing page again
  expect(wire).toContain("GET /.well-known/om-provider-capabilities");

  // Stored for this address only, and the pending secret is retired.
  expect(await secret(page, `admission-v1:${RELAY}`)).toBe(pending);
  expect(await secret(page, "admission-pending-v1")).toBeNull();

  await walkSetup(page, { stopAt: "invite" });
  await expect(page.getByText("Not connected to a delivery service")).toHaveCount(0);
  // Making an invite creates a mailbox at the locked Home: it only works with the admission secret kept for this address.
  await page.getByRole("button", { name: "Show as QR code" }).click();
  await expect(page.locator("code[translate=no]").first()).toContainText("/#invite/");
  expect(refused).toEqual([]);
  expect(wire.some((w) => w.startsWith("POST /v1/"))).toBe(true);
  await page.getByRole("button", { name: "Skip for now" }).click();
  await expect(page.getByRole("button", { name: "Settings" })).toBeVisible();
  await page.reload();
  await expect(page.getByRole("button", { name: "Settings" })).toBeVisible({ timeout: 30_000 });
  await expect(page.getByText("Not connected to a delivery service")).toHaveCount(0);
});
