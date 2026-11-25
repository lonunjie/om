// SPDX-License-Identifier: MPL-2.0
// Portability rendering evidence against the dev gallery service. No archive is read or written.
import { expect, test, type Page } from "@playwright/test";

async function reviewReadable(page: Page) {
  const wizard = page.getByRole("region", { name: "Export history", exact: true });
  await wizard.getByRole("radio", { name: "Readable copy" }).check();
  await wizard.getByRole("checkbox", { name: "Example chat" }).check();
  await wizard.getByRole("button", { name: "Review export" }).click();
  await expect(wizard.getByRole("heading", { name: "Review export" })).toBeFocused();
  return wizard;
}

for (const viewport of [{ width: 390, height: 844 }, { width: 1280, height: 800 }]) {
  test.describe(`${String(viewport.width)}×${String(viewport.height)}`, () => {
    test.use({ viewport });

    for (const colorScheme of ["light", "dark"] as const) {
      test(`shows both warnings and a read-only fidelity result in ${colorScheme}`, async ({ page }, testInfo) => {
        await page.emulateMedia({ colorScheme, reducedMotion: "reduce" });
        await page.goto("/?portability=ready");
        const wizard = await reviewReadable(page);
        await expect(wizard.getByText("The export is unencrypted.", { exact: false })).toBeVisible();
        await expect(wizard.getByText("Disappearing messages you include", { exact: false })).toBeVisible();
        const disappearing = wizard.getByRole("checkbox", { name: "Include disappearing messages in this copy" });
        await expect(disappearing).not.toBeChecked();
        await disappearing.focus();
        await page.keyboard.press("Space");
        await expect(disappearing).toBeChecked();
        expect(await page.evaluate(() => document.documentElement.scrollWidth)).toBeLessThanOrEqual(viewport.width);
        await wizard.screenshot({ path: testInfo.outputPath("review.png") });
        await wizard.getByRole("button", { name: "Save unencrypted export" }).click();
        await expect(wizard.getByRole("heading", { name: "Export saved" })).toBeFocused();

        const importer = page.getByRole("region", { name: "Import history", exact: true });
        await importer.getByRole("button", { name: "Choose archive file" }).click();
        await expect(importer.getByRole("radio", { name: "Read-only history" })).toBeChecked();
        await expect(importer.getByRole("radio", { name: "Restore history to this account" })).toBeDisabled();
        await importer.getByRole("button", { name: "Import history" }).click();
        await expect(importer.getByRole("heading", { name: "Imported as read-only history" })).toBeFocused();
        await expect(importer.getByText("Attachments missing from the archive (1)")).toBeVisible();
        await importer.screenshot({ path: testInfo.outputPath("import-report.png") });
      });
    }

    test("announces a slow save and disables repeat submission", async ({ page }, testInfo) => {
      await page.goto("/?portability=slow");
      const wizard = await reviewReadable(page);
      const save = wizard.getByRole("button", { name: "Save unencrypted export" });
      await save.click();
      await expect(wizard.getByText("Saving your export…")).toBeVisible();
      await expect(save).toBeDisabled();
      await wizard.screenshot({ path: testInfo.outputPath("saving.png") });
      await expect(wizard.getByRole("heading", { name: "Export saved" })).toBeVisible();
    });

    test("shows a refusal alongside the affected action", async ({ page }, testInfo) => {
      await page.goto("/?portability=refused");
      const wizard = await reviewReadable(page);
      await wizard.getByRole("button", { name: "Save unencrypted export" }).click();
      await expect(wizard.getByRole("alert")).toHaveText("OM couldn’t save the export. Try again.");
      await expect(wizard.getByRole("button", { name: "Save unencrypted export" })).toBeEnabled();
      await wizard.screenshot({ path: testInfo.outputPath("refused.png") });
    });
  });
}
