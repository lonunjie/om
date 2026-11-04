import { expect, type Page } from "@playwright/test";

export const BUNDLE_URL = "http://localhost:4175";

/** Browsers without an authenticator UI hang on credentials.create; make it fail so the vault falls back to the device key. */
export async function noPasskeys(page: Page): Promise<void> {
  await page.addInitScript(() => {
    navigator.credentials.create = () => Promise.reject(new DOMException("no authenticator", "NotAllowedError"));
  });
}

export interface OnboardOptions { readonly passphrase?: string; readonly stopAt?: "protected" | "invite"; readonly name?: string }

/** Walks first run on the dev harness. Returns how long the "Next: Secure your identity" step took (Argon2id shows up here). */
export async function onboard(page: Page, o: OnboardOptions = {}): Promise<{ readonly lockMs: number }> {
  await page.goto("/");
  await page.getByRole("button", { name: "Get started" }).click();
  return walkSetup(page, o);
}

/** The setup steps after the landing page: one tap on "Next: Secure your identity" lands on the ready state (invite first). */
export async function walkSetup(page: Page, o: OnboardOptions = {}): Promise<{ readonly lockMs: number }> {
  if (o.passphrase !== undefined) {
    await page.getByText("More options").click();
    await page.getByLabel("Also add a password of my own").check();
    await page.getByLabel("Password", { exact: true }).fill(o.passphrase);
    await page.getByLabel("Type it again").fill(o.passphrase);
  }
  await page.getByLabel("Your name").fill(o.name ?? "Tester");
  const t0 = Date.now();
  await page.getByRole("button", { name: "Next: Secure your identity" }).click();
  await expect(page.getByRole("heading", { name: "Invite someone to chat" })).toBeVisible({ timeout: 60_000 });
  const lockMs = Date.now() - t0;
  // "protected": open the explanation of how the chats are protected, for specs that assert on it.
  if (o.stopAt === "protected") {
    await page.getByText("How your chats are protected").click();
    return { lockMs };
  }
  if (o.stopAt === "invite") return { lockMs };
  await page.getByRole("button", { name: "Skip for now" }).click();
  await expect(page.getByRole("button", { name: "Settings" })).toBeVisible();
  return { lockMs };
}

/** The optional safety-code flow, from the "Save my safety code" reminder at the top of Settings (the reminder card is the first thing in Settings). */
export async function saveSafetyCode(page: Page): Promise<void> {
  await page.getByRole("button", { name: "Settings" }).click();
  await page.getByRole("button", { name: "Save my safety code" }).click();
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
  await page.getByRole("button", { name: "Settings" }).click();
  await expect(page.getByRole("heading", { name: "Settings", level: 1 })).toBeVisible();
  await expect(page.getByRole("button", { name: "Save my safety code" })).toHaveCount(0);
}

/** A further identity in the same browser: Settings > Add identity, then the same setup steps as a first run. */
export async function addIdentity(page: Page): Promise<void> {
  await page.getByRole("button", { name: "Settings" }).click();
  await page.getByRole("button", { name: "Add identity" }).click();
}

/**
 * After a reload every identity is behind its vault. A key kept in this browser opens by itself; a vault that wants a click
 * shows "Open my chats". Either way, finish when the app proper (its Settings button) is up.
 */
export async function reopen(page: Page, timeout = 30_000): Promise<void> {
  const settings = page.getByRole("button", { name: "Settings" });
  const unlock = page.getByRole("button", { name: "Open my chats", exact: true });
  for (const deadline = Date.now() + timeout; Date.now() < deadline;) {
    if (await settings.isVisible()) return;
    if (await unlock.isVisible()) await unlock.click({ timeout: 2_000 }).catch(() => undefined); // it may have unlocked by itself meanwhile
    await page.waitForTimeout(200);
  }
  await expect(settings).toBeVisible({ timeout: 1_000 });
}
