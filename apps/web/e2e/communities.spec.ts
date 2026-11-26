// SPDX-License-Identifier: MPL-2.0
// Community and Publication surface rendering evidence against the dev gallery service. No network is contacted.
import { expect, test } from "@playwright/test";

for (const viewport of [{ width: 390, height: 844 }, { width: 1280, height: 800 }]) {
  test.describe(`${String(viewport.width)}×${String(viewport.height)}`, () => {
    test.use({ viewport });

    for (const colorScheme of ["light", "dark"] as const) {
      test(`shows the community and says joining does not join its Rooms in ${colorScheme}`, async ({ page }, testInfo) => {
        await page.emulateMedia({ colorScheme, reducedMotion: "reduce" });
        await page.goto("/?communities=active");
        await expect(page.getByText("Joining the Community does not join its Rooms.").first()).toBeVisible();
        expect(await page.evaluate(() => document.documentElement.scrollWidth)).toBeLessThanOrEqual(viewport.width);
        await page.screenshot({ path: testInfo.outputPath("community.png"), fullPage: true });
      });
    }

    test("says it is checking, then says when it cannot load", async ({ page }, testInfo) => {
      await page.goto("/?communities=slow");
      await expect(page.getByText("Checking for updates…").first()).toBeVisible();
      await page.screenshot({ path: testInfo.outputPath("slow.png"), fullPage: true });
      await page.goto("/?communities=failed");
      await expect(page.getByText("Couldn’t load this page. Try checking again.").first()).toBeVisible();
      await page.screenshot({ path: testInfo.outputPath("failed.png"), fullPage: true });
    });

    test("renders the orphan state without horizontal overflow", async ({ page }, testInfo) => {
      await page.goto("/?communities=orphan");
      await expect(page.getByRole("main")).toBeVisible();
      expect(await page.evaluate(() => document.documentElement.scrollWidth)).toBeLessThanOrEqual(viewport.width);
      await page.screenshot({ path: testInfo.outputPath("orphan.png"), fullPage: true });
    });
  });
}
