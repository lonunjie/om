// Runs on Firefox and WebKit (device-key path: no authenticator in headless runs). Chromium has its own specs.
import { expect, test } from "@playwright/test";
import { BUNDLE_URL, noPasskeys, onboard } from "./helpers.ts";

test("onboarding with a passphrase, QR code, Settings > Devices and Recovery", async ({ page, browserName }, info) => {
  await noPasskeys(page);
  const { lockMs } = await onboard(page, { passphrase: "orbit lantern velvet coast", stopAt: "invite" });
  info.annotations.push({ type: "argon2id-lock-ms", description: `${browserName}: ${String(lockMs)}` });
  console.log(`argon2id default cost, lock step: ${browserName} ${String(lockMs)} ms`); // desktop figure, not a phone figure
  expect(lockMs).toBeLessThan(45_000);

  await page.getByRole("button", { name: "Show as QR code" }).click();
  await expect(page.getByRole("figure", { name: "QR code for your invite" }).locator("svg path")).toBeVisible();

  await page.getByRole("button", { name: "Skip for now" }).click();
  await page.getByRole("button", { name: "Open my chats" }).click();
  await page.getByRole("button", { name: "Settings" }).click();
  await page.getByRole("button", { name: "Devices" }).click();
  await expect(page.getByRole("heading", { name: "Devices", exact: true })).toBeVisible();
  await expect(page.getByText("No other devices are linked.")).toBeVisible();
  await page.getByRole("button", { name: "Recovery" }).click();
  await expect(page.getByRole("heading", { name: "Recovery", exact: true })).toBeVisible();
});

test("service worker installs, controls the page and serves the shell offline", async ({ page, context, browserName }) => {
  await page.goto(BUNDLE_URL + "/");
  await expect(page.getByRole("heading", { name: "OM is not connected yet" }).first()).toBeVisible();
  await page.evaluate(async () => { await navigator.serviceWorker.ready; });
  await page.reload();
  await expect.poll(() => page.evaluate(() => navigator.serviceWorker.controller !== null)).toBe(true);
  if (browserName === "webkit") {
    // Playwright's WebKit cannot reload offline with a service worker (the emulation fails before the worker answers).
    // Prove the same thing from the other side: the worker's cache already holds the shell and its assets.
    const cached = await page.evaluate(async () => {
      const names = await caches.keys();
      const hits: string[] = [];
      for (const n of names) for (const k of await (await caches.open(n)).keys()) hits.push(new URL(k.url).pathname);
      return hits;
    });
    expect(cached).toContain("/index.html");
    expect(cached.some((p) => p.startsWith("/assets/") && p.endsWith(".js"))).toBe(true);
    return;
  }
  await context.setOffline(true);
  await page.reload();
  await expect(page.getByRole("heading", { name: "OM is not connected yet" }).first()).toBeVisible();
  await context.setOffline(false);
});
