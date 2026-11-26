// SPDX-License-Identifier: MPL-2.0
// Bridge surface rendering evidence against the dev gallery service. No bridge or network is contacted.
import { expect, test } from "@playwright/test";

for (const viewport of [{ width: 390, height: 844 }, { width: 1280, height: 800 }]) {
  test.describe(`${String(viewport.width)}×${String(viewport.height)}`, () => {
    test.use({ viewport });

    for (const colorScheme of ["light", "dark"] as const) {
      test(`shows the active service and its review in ${colorScheme}`, async ({ page }, testInfo) => {
        await page.emulateMedia({ colorScheme, reducedMotion: "reduce" });
        await page.goto("/?services=active");
        await expect(page.getByRole("heading", { name: "Connected services", exact: true })).toBeVisible();
        const review = page.getByRole("button", { name: "Review Agenda assistant" });
        await expect(review).toBeVisible();
        expect(await page.evaluate(() => document.documentElement.scrollWidth)).toBeLessThanOrEqual(viewport.width);
        await page.screenshot({ path: testInfo.outputPath("list.png"), fullPage: true });
        await review.click();
        await expect(page.getByText("can read all Room content", { exact: false }).first()).toBeVisible();
        expect(await page.evaluate(() => document.documentElement.scrollWidth)).toBeLessThanOrEqual(viewport.width);
        await page.screenshot({ path: testInfo.outputPath("review.png"), fullPage: true });
      });
    }

    test("shows an offered service", async ({ page }, testInfo) => {
      await page.goto("/?services=install");
      await expect(page.getByRole("button", { name: "Review Agenda assistant" })).toBeVisible();
      await page.screenshot({ path: testInfo.outputPath("proposed.png"), fullPage: true });
    });

    test("says it is checking, then says when it cannot check", async ({ page }, testInfo) => {
      await page.goto("/?services=slow");
      await expect(page.getByText("Checking services…").first()).toBeVisible();
      await page.screenshot({ path: testInfo.outputPath("slow.png"), fullPage: true });
      await page.goto("/?services=failed");
      await expect(page.getByText("Couldn’t check connected services.").first()).toBeVisible();
      await page.screenshot({ path: testInfo.outputPath("failed.png"), fullPage: true });
    });
  });
}
