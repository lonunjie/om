import { expect, type Page } from "@playwright/test";

export const BUNDLE_URL = "http://localhost:4175";

/** Browsers without an authenticator UI hang on credentials.create; make it fail so the vault falls back to the device key. */
export async function noPasskeys(page: Page): Promise<void> {
  await page.addInitScript(() => {
    navigator.credentials.create = () => Promise.reject(new DOMException("no authenticator", "NotAllowedError"));
  });
}

export interface OnboardOptions { readonly passphrase?: string; readonly stopAt?: "protected" | "invite" }

/** Walks first run on the dev harness. Returns how long the "Protect my chats" step took (Argon2id shows up here). */
export async function onboard(page: Page, o: OnboardOptions = {}): Promise<{ readonly lockMs: number }> {
  await page.goto("/");
  await page.getByRole("button", { name: "Get started" }).click();
  return walkSetup(page, o);
}

/** The setup steps after the landing page: one tap on "Protect my chats" lands on the ready state (invite first). */
export async function walkSetup(page: Page, o: OnboardOptions = {}): Promise<{ readonly lockMs: number }> {
  if (o.passphrase !== undefined) {
    await page.getByText("More options").click();
    await page.getByLabel("Also add a password of my own").check();
    await page.getByLabel("Password", { exact: true }).fill(o.passphrase);
    await page.getByLabel("Type it again").fill(o.passphrase);
  }
  const t0 = Date.now();
  await page.getByRole("button", { name: "Protect my chats" }).click();
  await expect(page.getByRole("heading", { name: "Add your first contact" })).toBeVisible({ timeout: 60_000 });
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

/** The optional safety-code flow, from the "Save my safety code" reminder in the chat list. */
export async function saveSafetyCode(page: Page): Promise<void> {
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
  await expect(page.getByRole("button", { name: "Save my safety code" })).toHaveCount(0);
}
