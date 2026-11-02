// Settings > Lock on the real Core in a real browser: add a password, lock now, get the "Welcome back!" step, unlock, then stop locking.
import { expect, test } from "@playwright/test";
import { noPasskeys, onboard, reopen } from "./helpers.ts";

test.use({ baseURL: "http://localhost:4176" });

const PASSWORD = "correct horse battery staple";

test("a password can be added, locks the app on demand and after a reload, and can be taken off again", async ({ page }) => {
  await noPasskeys(page);
  await onboard(page, { name: "Lena" });
  await page.getByRole("button", { name: "Settings" }).click();
  await page.getByRole("button", { name: /^Lock/ }).click();
  await expect(page.getByRole("heading", { name: "Lock", level: 1 })).toBeVisible();
  await expect(page.getByText("Not locked on this device")).toBeVisible();
  await expect(page.getByText("Anyone who can use this browser can read your chats.")).toBeVisible();

  // Add a password: refused when too short, accepted when long enough and typed twice.
  await page.getByRole("button", { name: /Add a password/ }).click();
  await page.getByLabel("New password").fill("short");
  await page.getByLabel("Type it again").fill("short");
  await page.getByRole("button", { name: "Save password" }).click();
  await expect(page.getByRole("alert").first()).toBeVisible();
  await page.getByLabel("New password").fill(PASSWORD);
  await page.getByLabel("Type it again").fill(PASSWORD);
  await page.getByRole("button", { name: "Save password" }).click();
  await expect(page.getByText("Locked by your password")).toBeVisible({ timeout: 60_000 });

  // Choosing when to lock is remembered on this device.
  await page.getByRole("radio", { name: "After 1 minute" }).check();
  expect(await page.evaluate(() => globalThis.localStorage.getItem("om.lock.idle-minutes"))).toBe("1");

  // Lock now: the welcome step has the password field in front of the person, a wrong password says so, the right one returns to the same page.
  await page.getByRole("button", { name: "Lock now" }).click();
  await expect(page.getByRole("heading", { name: "Welcome back!" })).toBeVisible();
  await expect(page.getByLabel("Password")).toBeFocused();
  await page.getByLabel("Password").fill("not the right one at all");
  await page.keyboard.press("Enter");
  await expect(page.getByRole("alert")).toContainText("not right", { timeout: 60_000 });
  await page.getByLabel("Password").fill(PASSWORD);
  await page.keyboard.press("Enter");
  await expect(page.getByRole("heading", { name: "Lock", level: 1 })).toBeVisible({ timeout: 60_000 });
  await expect(page.getByText("Locked by your password")).toBeVisible();

  // A reload asks again (it no longer opens by itself).
  await page.reload();
  await expect(page.getByRole("heading", { name: "Welcome back!" })).toBeVisible({ timeout: 30_000 });
  await page.getByLabel("Password").fill(PASSWORD);
  await page.keyboard.press("Enter");
  await expect(page.getByRole("heading", { name: "Lock", level: 1 })).toBeVisible({ timeout: 60_000 });

  // Stopping every lock says what it means first, needs the password, and then the app opens by itself again.
  await page.getByRole("button", { name: /Stop locking on this device/ }).click();
  await expect(page.getByText(/Anyone who can use this browser, or copy its data, can read your chats/)).toBeVisible();
  await page.getByLabel("Current password").fill(PASSWORD);
  await page.getByRole("button", { name: "Stop locking", exact: true }).click();
  await expect(page.getByText("Not locked on this device")).toBeVisible({ timeout: 60_000 });
  await page.reload();
  await reopen(page);
  await expect(page.getByRole("heading", { name: "Welcome back!" })).toHaveCount(0);
});
