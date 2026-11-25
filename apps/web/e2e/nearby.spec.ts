// The "find each other on the same Wi-Fi" row in the contact sheet, on the dev-harness build with the dev gallery's fake service
// (`?nearby=on|policy|device`; without it the row is not offered at all). Looks at both reference sizes: `OM_SHOTS=<dir>` saves a
// screenshot of each state. The row's behaviour against a real Core is covered by the om-core and om-core-wasm suites.
import { expect, test, type Page } from "@playwright/test";
import { noPasskeys } from "./helpers.ts";

const SIZES = [{ name: "phone", width: 390, height: 844 }, { name: "desktop", width: 1280, height: 800 }] as const;
const SHOTS = process.env["OM_SHOTS"];

async function openSheet(page: Page, query: string): Promise<void> {
  await noPasskeys(page);
  await page.goto(`/?${query}`);
  await page.getByRole("button", { name: "Get started" }).click();
  await page.getByRole("textbox", { name: "Your name" }).fill("Tester");
  await page.getByRole("button", { name: "Next: Secure your identity" }).click();
  await page.getByRole("button", { name: "Skip for now" }).click();
  await page.getByRole("list", { name: "Identities" }).getByRole("button", { name: "Work" }).click();
  await page.getByRole("listbox", { name: "Conversations" }).getByRole("option", { name: /Ana Reyes/ }).click();
  await page.getByRole("button", { name: "About Ana Reyes" }).click();
  await expect(page.getByRole("dialog", { name: "About Ana Reyes" })).toBeVisible();
}

for (const size of SIZES) {
  test.describe(`${size.name} ${String(size.width)}x${String(size.height)}`, () => {
    test.use({ viewport: { width: size.width, height: size.height } });

    test("is not offered without a host service", async ({ page }) => {
      await openSheet(page, "");
      await expect(page.getByText("Find each other on the same Wi-Fi")).toHaveCount(0);
    });

    test("off by default, turns on and off, and is operable by keyboard", async ({ page }) => {
      await openSheet(page, "nearby=on");
      const toggle = page.getByRole("switch", { name: /Find each other on the same Wi-Fi/ });
      await expect(toggle).not.toBeChecked();
      await expect(page.getByText("They can tell when you are nearby", { exact: false })).toBeVisible();
      if (SHOTS !== undefined) await page.screenshot({ path: `${SHOTS}/nearby-off-${String(size.width)}.png` });
      await toggle.focus();
      await page.keyboard.press("Space");
      await expect(toggle).toBeChecked();
      await expect(page.getByText("On for this person.")).toBeVisible();
      if (SHOTS !== undefined) await page.screenshot({ path: `${SHOTS}/nearby-on-${String(size.width)}.png` });
      await page.keyboard.press("Space");
      await expect(toggle).not.toBeChecked();
    });

    test("says why it cannot be turned on", async ({ page }) => {
      await openSheet(page, "nearby=policy");
      const toggle = page.getByRole("switch", { name: /Find each other on the same Wi-Fi/ });
      await expect(toggle).toBeDisabled();
      await expect(page.getByText(/Not available right now/)).toBeVisible();
      if (SHOTS !== undefined) await page.screenshot({ path: `${SHOTS}/nearby-policy-${String(size.width)}.png` });
    });

    test("says when this device cannot do it", async ({ page }) => {
      await openSheet(page, "nearby=device");
      await expect(page.getByRole("switch", { name: /Find each other on the same Wi-Fi/ })).toBeDisabled();
      await expect(page.getByText("This device can’t do this.")).toBeVisible();
    });
  });
}
