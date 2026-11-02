// The production-shaped bundle (dev-signed, real Core, service worker): no provider is configured, and the app says so
// instead of pretending. Everything is queued on the device; nothing can reach a relay.
import { expect, test } from "@playwright/test";
import { BUNDLE_URL, noPasskeys, onboard } from "./helpers.ts";

test.use({ baseURL: BUNDLE_URL });

test("without a provider the real Core runs and the shell says it is not connected", async ({ page }) => {
  await noPasskeys(page);
  await onboard(page, { stopAt: "invite" });
  await page.getByRole("button", { name: "Skip for now" }).click();
  await expect(page.getByRole("button", { name: "Settings" })).toBeVisible();
  await expect(page.getByRole("status").filter({ hasText: "Not connected to a delivery service" })).toBeVisible();
  // No test banner: this is not the loopback build.
  await expect(page.getByText("LOOPBACK")).toHaveCount(0);
  // Identity data lives in the encrypted store: after a reload the identity is still there, and a key kept in this browser opens it without asking.
  await page.reload();
  await expect(page.getByRole("button", { name: "Settings" })).toBeVisible({ timeout: 30_000 });
  await expect(page.getByRole("button", { name: "Get started" })).toHaveCount(0);
});
