// The plain production build (`pnpm build`, no test flag, not dev-marked) starts the real Core from the base module, which the page has checked against
// the digest compiled into the bundle, and does not fetch the groups module at first load (ADR-0187). Needs `pnpm build` first.
import { expect, test } from "@playwright/test";

test.use({ baseURL: "http://localhost:4181" });

test("production build: the base Core starts verified and the groups module is not fetched at first load", async ({ page }) => {
  const wasm: string[] = [];
  page.on("request", (r) => { if (r.url().endsWith(".wasm")) wasm.push(new URL(r.url()).pathname); });
  await page.goto("/");
  await page.getByRole("button", { name: /Get started/ }).first().click();
  // The app got past loading: it shows the first-run page, not the "could not start" notice.
  await expect(page.getByText("Getting OM ready")).toBeHidden({ timeout: 30_000 });
  await page.waitForLoadState("networkidle");
  await expect(page.getByText("OM could not start")).toHaveCount(0);
  expect(wasm.filter((u) => /\/om_wasm-[\w-]+\.wasm$/.test(u)).length).toBeGreaterThan(0);
  expect(wasm.filter((u) => /om_wasm_groups/.test(u))).toEqual([]);
});
