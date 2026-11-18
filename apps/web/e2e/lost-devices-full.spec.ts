// The whole "I lost my devices" path in two browsers on the real Core and the local reference Home that serves the recovery store
// (ADR-0199 to ADR-0203): the first device makes the account and saves the safety code, a second browser knows nothing, asks for the
// account back with the safety code, and the first device approves after both show the same six digits. The only things passed between
// the browsers are what a person would carry: the safety code and the approval link.
import { expect, test, type Page } from "@playwright/test";
import { E2E_ADMISSION_SECRET } from "./admission-fixture.ts";

// A name with a dot: a key sheet cannot name a bare address, and `*.localhost` is this machine (RFC 6761).
const HOME = "http://home.localhost:8789";
import { chooseOwnRelay, noPasskeys, walkSetup } from "./helpers.ts";

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

/** A device that knows the locked Home (the same owner secret, as a person's other device would hold). */
async function connectToHome(page: Page): Promise<void> {
  await page.goto("/");
  await seedPending(page, hex(E2E_ADMISSION_SECRET));
  await page.getByRole("button", { name: "Get started" }).click();
  await chooseOwnRelay(page);
  await page.getByLabel("Address of your delivery service").fill(HOME);
  await page.getByRole("button", { name: "Check and continue" }).click();
  await expect(page.getByRole("button", { name: "Start new" })).toBeVisible({ timeout: 30_000 });
}

/** Settings > Save my safety code: the groups, written back in as the check asks. */
async function saveSafetyCode(page: Page): Promise<string[]> {
  await page.getByRole("button", { name: "Settings" }).click();
  await page.getByRole("button", { name: "Save my safety code" }).click();
  return writeDown(page);
}

/** The safety-code step itself: read the groups, write them back as the check asks, confirm. */
async function writeDown(page: Page): Promise<string[]> {
  const items = page.getByRole("list", { name: "Safety code" }).getByRole("listitem");
  await expect(items.first()).toBeVisible();
  const groups = (await items.allTextContents()).map((t) => t.replace(/^Group \d+ of \d+:?\s*/, "").trim());
  await page.getByRole("button", { name: "I wrote it down" }).click();
  const boxes = page.getByRole("textbox");
  for (let i = 0; i < (await boxes.count()); i++) {
    const box = boxes.nth(i);
    const label = await box.evaluate((el) => el.closest("label")?.textContent ?? "");
    const n = Number(/(\d+)/.exec(label)?.[1]);
    await box.fill(groups[n - 1] ?? "");
  }
  await page.getByRole("button", { name: "Confirm" }).click();
  await expect(page.getByRole("button", { name: "Settings" })).toBeVisible({ timeout: 30_000 });
  return groups;
}

/** Settings > Recovery, whichever of the two screens the person is on. */
async function openRecovery(page: Page): Promise<void> {
  const recovery = page.getByRole("button", { name: "Recovery" }).first();
  for (const deadline = Date.now() + 30_000; Date.now() < deadline;) {
    if (await recovery.isVisible()) { await recovery.click(); return; }
    await page.waitForTimeout(1_500); // the menu may still be opening; only then press Settings
    if (!(await recovery.isVisible())) await page.getByRole("button", { name: "Settings" }).click({ timeout: 2_000 }).catch(() => undefined);
  }
  await recovery.click({ timeout: 1_000 });
}

test("a person with no device left asks for the account back, and the first device approves", async ({ browser }) => {
  test.setTimeout(300_000);
  const ctxA = await browser.newContext({ baseURL: "http://localhost:4179" });
  const a = await ctxA.newPage();
  await noPasskeys(a);
  await connectToHome(a);
  await walkSetup(a, { name: "Maya" });
  const groups = await saveSafetyCode(a);
  expect(groups.length).toBeGreaterThan(20); // the sheet carries where the account is saved, not only the key
  await openRecovery(a);
  await expect(a.getByText(/Recovery is on/)).toBeVisible({ timeout: 30_000 });
  await a.waitForTimeout(10_000); // the Home has the record before a second device looks

  const ctxB = await browser.newContext({ baseURL: "http://localhost:4179" });
  const b = await ctxB.newPage();
  await noPasskeys(b);
  await connectToHome(b);
  await b.getByRole("button", { name: "I lost my devices" }).click();
  await b.getByRole("button", { name: "Next: Secure this device" }).click();
  await expect(b.getByRole("heading", { name: "Get your account back" })).toBeVisible({ timeout: 60_000 });
  await b.getByLabel("Your recovery key").fill(groups.join(" "));
  await b.getByRole("button", { name: "Find my account" }).click();
  await expect(b.getByRole("heading", { name: "Your saved account was found" })).toBeVisible({ timeout: 60_000 });
  await b.getByRole("button", { name: "Ask for my account back" }).click();
  await expect(b.getByRole("heading", { name: "Waiting to get your account back" })).toBeVisible({ timeout: 60_000 });

  // The first device offers the approval and shows the link.
  await a.getByRole("button", { name: "Help another device get the account back" }).click();
  await a.getByRole("button", { name: "Show the link and code" }).click();
  const link = ((await a.locator("code.om-devices__link").textContent()) ?? "").trim();
  expect(link).toMatch(/^om:recover-approve\//);
  await b.getByLabel("Link from your other device").fill(link);
  await b.getByRole("button", { name: "Continue" }).click();
  const codeB = await b.getByText(/^\d{6}$/).first().textContent({ timeout: 90_000 });
  const codeA = await a.getByText(/^\d{6}$/).first().textContent({ timeout: 90_000 });
  expect(codeA).toBe(codeB);
  await a.getByRole("button", { name: "Yes, this is me" }).click();
  await expect(b.getByRole("button", { name: "Settings" })).toBeVisible({ timeout: 120_000 });
});

test("a new safety code replaces the first: only the new one brings the account back", async ({ browser }) => {
  test.setTimeout(300_000);
  const ctxA = await browser.newContext({ baseURL: "http://localhost:4179" });
  const a = await ctxA.newPage();
  await noPasskeys(a);
  await connectToHome(a);
  await walkSetup(a, { name: "Maya" });
  const first = await saveSafetyCode(a);
  await a.waitForTimeout(8_000);
  await openRecovery(a);
  await a.getByRole("button", { name: "Make a new safety code" }).first().click();
  const second = await writeDown(a);
  expect(second.join("")).not.toBe(first.join(""));
  await a.waitForTimeout(8_000); // the record under the new key is written before a new device looks

  const ctxB = await browser.newContext({ baseURL: "http://localhost:4179" });
  const b = await ctxB.newPage();
  await noPasskeys(b);
  await connectToHome(b);
  await b.getByRole("button", { name: "I lost my devices" }).click();
  await b.getByRole("button", { name: "Next: Secure this device" }).click();
  await expect(b.getByRole("heading", { name: "Get your account back" })).toBeVisible({ timeout: 60_000 });
  await b.getByLabel("Your recovery key").fill(second.join(" "));
  await b.getByRole("button", { name: "Find my account" }).click();
  await expect(b.getByRole("heading", { name: "Your saved account was found" })).toBeVisible({ timeout: 60_000 });
  await b.getByRole("button", { name: "Ask for my account back" }).click();
  await a.getByRole("button", { name: "Settings" }).click().catch(() => undefined);
  await openRecovery(a);
  await a.getByRole("button", { name: "Help another device get the account back" }).click();
  await a.getByRole("button", { name: "Show the link and code" }).click();
  const link = ((await a.locator("code.om-devices__link").textContent()) ?? "").trim();
  await b.getByLabel("Link from your other device").fill(link);
  await b.getByRole("button", { name: "Continue" }).click();
  const codeB = await b.getByText(/^\d{6}$/).first().textContent({ timeout: 90_000 });
  const codeA = await a.getByText(/^\d{6}$/).first().textContent({ timeout: 90_000 });
  expect(codeA).toBe(codeB);
  await a.getByRole("button", { name: "Yes, this is me" }).click();
  await expect(b.getByRole("button", { name: "Settings" })).toBeVisible({ timeout: 120_000 });
});
