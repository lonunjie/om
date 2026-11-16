// Settings > Protection on the real Core (om_wasm.wasm): the person's own level, at phone and desktop widths. Tightening applies at once,
// loosening names what changes and needs its own confirmation, and the choice is still there after the app is reopened.
import { createRequire } from "node:module";
import { expect, test, type Page } from "@playwright/test";
import { noPasskeys, onboard, reopen } from "./helpers.ts";
import { shot } from "./invite-flow.ts";

const axeSource = createRequire(import.meta.url).resolve("axe-core/axe.min.js");
test.use({ baseURL: "http://localhost:4176", bypassCSP: true }); // bypassCSP only so the test can inject axe

const SIZES = [{ name: "phone", width: 390, height: 780 }, { name: "desktop", width: 1280, height: 800 }] as const;

/** axe over the page with colour contrast, in the light and dark appearance, once entrance motion has settled. */
async function noViolations(page: Page): Promise<void> {
  await page.addScriptTag({ path: axeSource });
  for (const scheme of ["light", "dark"] as const) {
    await page.emulateMedia({ colorScheme: scheme });
    await page.evaluate(async () => { await Promise.all(document.getAnimations().filter((a) => a.effect?.getComputedTiming().iterations !== Infinity).map((a) => a.finished.catch(() => undefined))); });
    const found = await page.evaluate(async () => {
      const axe = (globalThis as unknown as { axe: { run: (el: Element, o: unknown) => Promise<{ violations: { id: string; help: string; nodes: { target: unknown }[] }[] }> } }).axe;
      const el = document.querySelector(".om-protection");
      if (el === null) return ["missing"];
      const r = await axe.run(el, { rules: { region: { enabled: false } } });
      return r.violations.map((v) => `${v.id}: ${v.help} ${JSON.stringify(v.nodes.map((n) => n.target))}`);
    });
    expect(found, scheme).toEqual([]);
  }
  await page.emulateMedia({ colorScheme: "light" });
}

async function openProtection(page: Page): Promise<void> {
  const heading = page.getByRole("heading", { name: "Protection", exact: true });
  if (await heading.isVisible()) return; // a reload keeps the address, so the page can already be open
  await page.getByRole("button", { name: "Settings" }).click();
  await page.getByRole("button", { name: /^Protection/ }).click();
  await expect(page.getByRole("heading", { name: "Protection", exact: true })).toBeVisible();
}
const row = (page: Page, name: string) => page.locator(".om-settings__row").filter({ has: page.getByRole("heading", { name, exact: true }) });

for (const size of SIZES) {
  test.describe(size.name, () => {
    test.use({ viewport: { width: size.width, height: size.height } });

    test("Recommended by default; tightening is immediate; loosening asks first; the level survives a reopen", async ({ page }) => {
      await noPasskeys(page);
      await onboard(page, { stopAt: "invite" });
      await page.getByRole("button", { name: "Skip for now" }).click();
      await openProtection(page);
      await expect(page.getByText("All three levels use the same strong encryption.")).toBeVisible();
      await expect(row(page, "Recommended").getByText("In use")).toBeVisible();
      await noViolations(page);
      await shot(page, `43-protection-${size.name}`);

      // Tightening: one tap, with its cost said before the tap.
      await expect(row(page, "Hardened")).toContainText("You will be asked each time a contact gets their account back.");
      await row(page, "Hardened").getByRole("button", { name: "Use Hardened" }).click();
      await expect(page.getByRole("status").filter({ hasText: "Now using Hardened." })).toBeVisible();
      await expect(row(page, "Hardened").getByText("In use")).toBeVisible();
      await row(page, "Maximum Privacy").getByRole("button", { name: "Use Maximum Privacy" }).click();
      await expect(row(page, "Maximum Privacy").getByText("In use")).toBeVisible();

      // Loosening: names what changes, nothing happens until the person confirms.
      await row(page, "Recommended").getByRole("button", { name: "Use Recommended" }).click();
      const ask = page.getByRole("group", { name: "Use Recommended instead?" });
      await expect(ask).toBeVisible();
      await expect(ask).toContainText("without asking you");
      await expect(ask).toContainText("less careful");
      await noViolations(page);
      await shot(page, `44-protection-loosen-${size.name}`);
      await ask.getByRole("button", { name: "Keep what I have now" }).click();
      await expect(row(page, "Maximum Privacy").getByText("In use")).toBeVisible();
      await row(page, "Hardened").getByRole("button", { name: "Use Hardened" }).click(); // lower than Maximum: also a loosening
      await page.getByRole("group", { name: "Use Hardened instead?" }).getByRole("button", { name: "Yes, use Hardened" }).click();
      await expect(row(page, "Hardened").getByText("In use")).toBeVisible();

      // Durable: still Hardened after the app is closed and opened again.
      await reopen(page);
      await openProtection(page);
      await expect(row(page, "Hardened").getByText("In use")).toBeVisible();
      await expect(row(page, "Recommended").getByRole("button", { name: "Use Recommended" })).toBeVisible();
    });
  });
}
