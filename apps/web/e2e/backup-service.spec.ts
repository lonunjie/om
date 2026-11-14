// A backup delivery service (D22, Settings > Message delivery > Advanced) against a bundle that carries no service of its own and TWO locked local
// Homes (8789, 8790), each holding only the hash of its own owner secret, as a person's two deployments would. The person sets up the first one on
// first run, adds the second as a backup with the same setup steps (a second setup value, the typed address, the probe), reconnects, sees both with
// how they are doing, then moves away from the first. What this does not cover: a second person, and a Home going down mid-conversation. Those are the
// Rust scenarios and infra/cloudflare-home/test/two-homes.e2e.test.ts (V-020), which run Core and the same adapter against two real Homes.
import { expect, test, type Locator, type Page } from "@playwright/test";
import { E2E_ADMISSION_SECRET, E2E_ADMISSION_SECRET_2, LOCKED_RELAY, LOCKED_RELAY_2 } from "./admission-fixture.ts";
import { chooseOwnRelay, noPasskeys, reopen, walkSetup } from "./helpers.ts";

test.use({ baseURL: "http://localhost:4179" });

const hex = (b: Buffer): string => b.toString("hex");

/** Makes this device hold `secretHex` as the setup secret not yet bound to an address, as if it had made it earlier. */
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

/** One entry of a tiny IndexedDB store, as JSON (secrets come back as hex), or null. */
async function read(page: Page, db: string, store: string, key: string): Promise<unknown> {
  return page.evaluate(({ db, store, key }) => new Promise<unknown>((resolve) => {
    const open = indexedDB.open(db, 1);
    open.onerror = () => { resolve(null); };
    open.onsuccess = () => {
      const d = open.result;
      if (!d.objectStoreNames.contains(store)) { resolve(null); return; }
      const q = d.transaction(store, "readonly").objectStore(store).get(key);
      q.onsuccess = () => {
        const v: unknown = q.result;
        resolve(v instanceof Uint8Array ? (v.length > 0 ? Array.from(v, (b) => b.toString(16).padStart(2, "0")).join("") : null) : (v ?? null));
      };
      q.onerror = () => { resolve(null); };
    };
  }), { db, store, key });
}

const row = (page: Page, role: "Main" | "Backup" | "Moving away"): Locator => page.getByRole("listitem").filter({ has: page.getByRole("heading", { name: role, exact: true }) });

async function openAdvanced(page: Page): Promise<void> {
  await page.goto("/settings/delivery"); // a fresh load: the key kept in this browser opens by itself
  await reopen(page);
  await expect(page.getByRole("heading", { name: "Message delivery" }).first()).toBeVisible();
  await page.getByRole("button", { name: "Advanced", exact: true }).click();
}

test("add a backup delivery service, see both, then move away from the first", async ({ page }) => {
  test.setTimeout(240_000);
  await noPasskeys(page);
  const refused: string[] = [];
  const seen = { first: 0, second: 0 };
  page.on("request", (r) => { if (r.url().startsWith(LOCKED_RELAY)) seen.first++; if (r.url().startsWith(LOCKED_RELAY_2)) seen.second++; });
  page.on("response", (r) => { if ((r.url().startsWith(LOCKED_RELAY) || r.url().startsWith(LOCKED_RELAY_2)) && (r.status() === 401 || r.status() === 403)) refused.push(`${String(r.status())} ${r.url()}`); });

  // First run: the first delivery service, exactly as in the setup spec.
  await page.goto("/");
  await seedPending(page, hex(E2E_ADMISSION_SECRET));
  await page.getByRole("button", { name: "Get started" }).click();
  await chooseOwnRelay(page);
  await page.getByLabel("Address of your delivery service").fill(LOCKED_RELAY);
  await page.getByRole("button", { name: "Check and continue" }).click();
  await expect(page.getByRole("button", { name: "Start new" })).toBeVisible({ timeout: 30_000 }); // connected: first run now offers "Start new" and "I already use OM" (D24)
  await walkSetup(page, { stopAt: "invite" });
  await page.getByRole("button", { name: "Skip for now" }).click();
  await expect(page.getByRole("button", { name: "Settings" })).toBeVisible();

  // The config is stored in the new shape and, for an older build, in the old one too.
  expect(await read(page, "om-app-config", "config", "homes-v1")).toEqual([{ url: LOCKED_RELAY }]);
  expect(await read(page, "om-app-config", "config", "provider-url")).toBe(LOCKED_RELAY);

  // One service: Advanced lists it, offers a backup, and there is nothing to move to yet.
  await openAdvanced(page);
  await expect(row(page, "Main")).toHaveCount(1);
  await expect(row(page, "Main").getByText(/It reports that it is working normally/)).toBeVisible();
  await expect(page.getByRole("button", { name: "Move away from this one" })).toHaveCount(0);
  await expect(page.getByText("You need a second delivery service to move to. Add a backup first.")).toBeVisible();

  // Add the second one with the same steps: its own setup value, the typed address, the probe.
  await seedPending(page, hex(E2E_ADMISSION_SECRET_2));
  await page.getByRole("button", { name: "Add a backup delivery service" }).click();
  const shown = (await page.getByText(/^[0-9a-f]{64}$/).textContent()) ?? "";
  expect(shown).not.toBe("");
  await page.getByLabel("Address of your delivery service").fill("http://127.0.0.1:1");
  await page.getByRole("button", { name: "Check and add" }).click();
  await expect(page.getByRole("alert").filter({ hasText: /could not reach/i })).toBeVisible();
  expect(await read(page, "om-app-config", "config", "homes-v1")).toEqual([{ url: LOCKED_RELAY }]); // a typo stores nothing
  await page.getByLabel("Address of your delivery service").fill(LOCKED_RELAY_2);
  await page.getByRole("button", { name: "Check and add" }).click();
  await expect(page.getByText("Added. OM will use it as a backup the next time it connects.")).toBeVisible();
  expect(await read(page, "om-app-config", "config", "homes-v1")).toEqual([{ url: LOCKED_RELAY }, { url: LOCKED_RELAY_2 }]);
  expect(await read(page, "om-app-config", "config", "provider-url")).toBe(LOCKED_RELAY); // the main service did not change
  expect(await read(page, "om-provider-secrets", "secrets", `admission-v1:${LOCKED_RELAY_2}`)).toBe(hex(E2E_ADMISSION_SECRET_2));
  expect(await read(page, "om-provider-secrets", "secrets", `admission-v1:${LOCKED_RELAY}`)).toBe(hex(E2E_ADMISSION_SECRET));
  expect(await read(page, "om-provider-secrets", "secrets", "admission-pending-v1")).toBeNull();

  // Connect again: the app restarts against both and opens by itself.
  await page.getByRole("button", { name: /Connect now/ }).click();
  await reopen(page);
  await openAdvanced(page);
  await expect(row(page, "Main")).toHaveCount(1);
  await expect(row(page, "Backup")).toHaveCount(1);
  await expect(row(page, "Backup").getByText(/It reports that it is working normally/)).toBeVisible();
  await expect(page.getByRole("heading", { name: "Use a different delivery service" })).toHaveCount(0); // would replace more than the person sees
  await row(page, "Backup").getByRole("button", { name: "Show address" }).click();
  await expect(row(page, "Backup").getByText(`Address: ${LOCKED_RELAY_2}`)).toBeVisible();
  expect(seen.second).toBeGreaterThan(0); // the second Home is really being talked to

  // Both services work for the thing that needs a mailbox: an invite (a fresh load, so Core starts against both).
  await page.goto("/");
  await reopen(page);
  await page.getByRole("button", { name: "Show my QR" }).click();
  await expect(page.locator("code[translate=no]").first()).toContainText("/#invite/");
  expect(refused).toEqual([]);

  // Move away from the first: asked first, then done; the backup becomes the main service after the next connection.
  await openAdvanced(page);
  await row(page, "Main").getByRole("button", { name: "Move away from this one" }).click();
  await expect(page.getByRole("group", { name: "Move away from this delivery service?" })).toBeVisible();
  await page.getByRole("button", { name: "Move away", exact: true }).click();
  await expect(page.getByText("OM is moving your chats to your other delivery service. Connect again to finish.")).toBeVisible();
  await expect(row(page, "Moving away")).toHaveCount(1);
  await expect(row(page, "Backup").getByRole("button", { name: "Move away from this one" })).toHaveCount(0); // the only service left
  expect(await read(page, "om-app-config", "config", "homes-v1")).toEqual([{ url: LOCKED_RELAY_2 }, { url: LOCKED_RELAY, draining: true }]);
  expect(await read(page, "om-app-config", "config", "provider-url")).toBe(LOCKED_RELAY_2);
  await page.getByRole("button", { name: /Connect now/ }).click();
  await reopen(page);
  await openAdvanced(page);
  await expect(row(page, "Main")).toHaveCount(1);
  await expect(row(page, "Moving away")).toHaveCount(1);
  await row(page, "Main").getByRole("button", { name: "Show address" }).click();
  await expect(row(page, "Main").getByText(`Address: ${LOCKED_RELAY_2}`)).toBeVisible();
  await expect(row(page, "Moving away").getByRole("button", { name: "Try moving again" })).toBeVisible();
  // An invitation made before the move lives at the service that was main then; its introductions are collected at the new main one, which does not know it (said in the
  // confirmation: "Invitations you shared that nobody has accepted yet will stop working"). Nothing else may be refused.
  expect(refused.filter((r) => !r.includes("/v1/intro/drain"))).toEqual([]);
});

test("an install that stored one address before backups existed keeps it and its secret", async ({ page }) => {
  await noPasskeys(page);
  await page.goto("/");
  // As an older build left it: one address under the old key, nothing under the new one, and the secret bound to that address.
  await page.evaluate((args) => new Promise<void>((resolve, reject) => {
    const open = indexedDB.open("om-app-config", 1);
    open.onupgradeneeded = () => { open.result.createObjectStore("config"); };
    open.onerror = () => { reject(new Error("seed failed")); };
    open.onsuccess = () => {
      const tx = open.result.transaction("config", "readwrite");
      tx.objectStore("config").put(args.url, "provider-url");
      tx.oncomplete = () => { open.result.close(); resolve(); };
    };
  }), { url: LOCKED_RELAY });
  await page.evaluate((args) => new Promise<void>((resolve, reject) => {
    const open = indexedDB.open("om-provider-secrets", 1);
    open.onupgradeneeded = () => { open.result.createObjectStore("secrets"); };
    open.onerror = () => { reject(new Error("seed failed")); };
    open.onsuccess = () => {
      const tx = open.result.transaction("secrets", "readwrite");
      tx.objectStore("secrets").put(Uint8Array.from(args.h.match(/../g) ?? [], (b) => parseInt(b, 16)), `admission-v1:${args.url}`);
      tx.oncomplete = () => { open.result.close(); resolve(); };
    };
  }), { h: hex(E2E_ADMISSION_SECRET), url: LOCKED_RELAY });
  await page.reload();
  // No setup page: the stored service is the one in use.
  await page.getByRole("button", { name: "Get started" }).click();
  await expect(page.getByRole("button", { name: "Start new" })).toBeVisible({ timeout: 30_000 }); // connected: first run now offers "Start new" and "I already use OM" (D24)
  await walkSetup(page, { stopAt: "invite" });
  await page.getByRole("button", { name: "Skip for now" }).click();
  await expect(page.getByRole("button", { name: "Settings" })).toBeVisible();
  await openAdvanced(page);
  await expect(row(page, "Main")).toHaveCount(1);
  await row(page, "Main").getByRole("button", { name: "Show address" }).click();
  await expect(row(page, "Main").getByText(`Address: ${LOCKED_RELAY}`)).toBeVisible();
  await expect(row(page, "Main").getByText(/It reports that it is working normally/)).toBeVisible();
  expect(await read(page, "om-provider-secrets", "secrets", `admission-v1:${LOCKED_RELAY}`)).toBe(hex(E2E_ADMISSION_SECRET));
});
