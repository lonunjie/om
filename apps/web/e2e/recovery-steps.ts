// Steps shared by the real-Core "I lost my devices" specs (lost-devices-full, peer-recovery-live): a device that knows the locked Home, the safety-code
// sheet, and the Recovery screen. Each does only what a person can do on screen.
import { expect, type Page } from "@playwright/test";
import { E2E_ADMISSION_SECRET } from "./admission-fixture.ts";
import { chooseOwnRelay } from "./helpers.ts";

// A name with a dot: a key sheet cannot name a bare address, and `*.localhost` is this machine (RFC 6761).
export const HOME = "http://home.localhost:8789";
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
export async function connectToHome(page: Page): Promise<void> {
  await page.goto("/");
  await seedPending(page, hex(E2E_ADMISSION_SECRET));
  await page.getByRole("button", { name: "Get started" }).click();
  await chooseOwnRelay(page);
  await page.getByLabel("Address of your delivery service").fill(HOME);
  await page.getByRole("button", { name: "Check and continue" }).click();
  await expect(page.getByRole("button", { name: "Start new" })).toBeVisible({ timeout: 30_000 });
}

/** Settings > Save my safety code: the groups, written back in as the check asks. */
export async function saveSafetyCode(page: Page): Promise<string[]> {
  await page.getByRole("button", { name: "Settings" }).click();
  await page.getByRole("button", { name: "Save my safety code" }).click();
  return writeDown(page);
}

/** The safety-code step itself: read the groups, write them back as the check asks, confirm. */
export async function writeDown(page: Page): Promise<string[]> {
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
export async function openRecovery(page: Page): Promise<void> {
  const recovery = page.getByRole("button", { name: "Recovery" }).first();
  for (const deadline = Date.now() + 30_000; Date.now() < deadline;) {
    if (await recovery.isVisible()) { await recovery.click(); return; }
    await page.waitForTimeout(1_500); // the menu may still be opening; only then press Settings
    if (!(await recovery.isVisible())) await page.getByRole("button", { name: "Settings" }).click({ timeout: 2_000 }).catch(() => undefined);
  }
  await recovery.click({ timeout: 1_000 });
}
