// SPDX-License-Identifier: MPL-2.0
import { expect, test } from "@playwright/test";

for (const viewport of [{ width: 390, height: 844 }, { width: 1280, height: 800 }]) {
  test.describe(`${String(viewport.width)}×${String(viewport.height)}`, () => {
    test.use({ viewport });

    for (const colorScheme of ["light", "dark"] as const) {
      test(`separates plugin controls from host review in ${colorScheme}`, async ({ page }, testInfo) => {
        await page.emulateMedia({ colorScheme, reducedMotion: "reduce" });
        await page.goto("/?plugins=active");
        await expect(page.getByRole("heading", { name: "Plugin settings" })).toBeVisible();
        await expect(page.getByRole("region", { name: "Controls provided by the plugin" })).toBeVisible();
        await expect(page.getByText("This plugin has read chat content.")).toBeVisible();
        expect(await page.evaluate(() => document.documentElement.scrollWidth)).toBeLessThanOrEqual(viewport.width);
        await page.screenshot({ path: testInfo.outputPath("settings.png"), fullPage: true });

        const review = page.getByRole("button", { name: "Review message" });
        await review.focus();
        await page.keyboard.press("Enter");
        const send = page.getByRole("dialog", { name: "Send this message?" });
        await expect(send.getByRole("heading", { name: "Note tools" })).toBeFocused();
        await expect(send.getByText("Weekend plans · Ari and Jo")).toBeVisible();
        await expect(send.getByText("Bring the notes.", { exact: false })).toBeVisible();
        await page.screenshot({ path: testInfo.outputPath("send.png"), fullPage: true });
        await page.keyboard.press("Escape");
        await expect(review).toBeFocused();
        await page.getByRole("button", { name: "Review update" }).click();
        const update = page.getByRole("dialog", { name: "Review plugin update" });
        await expect(update.getByText("It stays off until you approve it separately.", { exact: false })).toHaveCount(2);
        expect(await page.evaluate(() => document.documentElement.scrollWidth)).toBeLessThanOrEqual(viewport.width);
        await page.screenshot({ path: testInfo.outputPath("update.png"), fullPage: true });
      });
    }

    test("discloses the permissions before install", async ({ page }, testInfo) => {
      await page.goto("/?plugins=install");
      await page.getByRole("button", { name: "Review install" }).click();
      const review = page.getByRole("dialog", { name: "Review plugin install" });
      await expect(review.getByText("You confirm every send.", { exact: false })).toBeVisible();
      await expect(review.getByText("This permission stays off until you approve it.")).toHaveCount(3);
      await page.screenshot({ path: testInfo.outputPath("install.png"), fullPage: true });
    });

    test("announces slow and failed checks", async ({ page }, testInfo) => {
      await page.goto("/?plugins=slow");
      await expect(page.getByRole("status")).toHaveText("Checking plugin settings…");
      await page.screenshot({ path: testInfo.outputPath("slow.png"), fullPage: true });
      await page.goto("/?plugins=failed");
      await expect(page.getByRole("alert")).toHaveText("Couldn’t check plugin settings.");
      await page.screenshot({ path: testInfo.outputPath("failed.png"), fullPage: true });
    });
  });
}
