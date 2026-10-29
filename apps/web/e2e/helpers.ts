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

/** The setup steps after the landing page ("Protect my chats" through the recovery key check). */
export async function walkSetup(page: Page, o: OnboardOptions = {}): Promise<{ readonly lockMs: number }> {
  if (o.passphrase !== undefined) {
    await page.getByLabel("Also add a password of my own").check();
    await page.getByLabel("Password", { exact: true }).fill(o.passphrase);
    await page.getByLabel("Type it again").fill(o.passphrase);
  }
  const t0 = Date.now();
  await page.getByRole("button", { name: "Protect my chats" }).click();
  await expect(page.getByRole("heading", { name: "How your data is locked" })).toBeVisible({ timeout: 60_000 });
  const lockMs = Date.now() - t0;
  if (o.stopAt === "protected") return { lockMs };
  await page.getByRole("button", { name: "Continue" }).click();
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
  await expect(page.getByRole("heading", { name: "Add your first contact" })).toBeVisible();
  if (o.stopAt === "invite") return { lockMs };
  await page.getByRole("button", { name: "Skip for now" }).click();
  await page.getByRole("button", { name: "Open my chats" }).click();
  await expect(page.getByRole("button", { name: "Settings" })).toBeVisible();
  return { lockMs };
}
