// A device that has not been told about a newly linked device is listed as "Not told yet" on the approving device (ADR-0193).
// Real Core in two or three browser contexts over the local reference Home. The account has no conversation, so the first
// linked device has no route to be told through: the second link is approved anyway (the default), and the first device
// stays listed until it can be told. The strict option is a plain checkbox that Core reports back.
import { expect, test, type Browser, type Page } from "@playwright/test";
import { newDevice } from "./invite-flow.ts";
import { walkSetup } from "./helpers.ts";

test.use({ baseURL: "http://localhost:4177" });
const BASE = "http://localhost:4177";

async function showLink(page: Page): Promise<string> {
  await page.getByRole("button", { name: "Add a device" }).click();
  await page.getByRole("button", { name: "Show the code and link" }).click();
  const qr = page.locator("figure[data-kind=device-link]");
  await expect(qr).toBeVisible({ timeout: 60_000 });
  return (await page.locator("code.om-devices__link").textContent())?.trim() ?? "";
}

/** A new device joins with `link`; the existing device approves. Leaves the existing device on the "linked" step. */
async function joinAndApprove(browser: Browser, existing: Page, link: string): Promise<Page> {
  const next = await newDevice(browser, BASE);
  await next.goto("/");
  await next.getByRole("button", { name: "Get started" }).first().click();
  await next.getByRole("button", { name: "I already use OM" }).click();
  await next.getByRole("button", { name: "Next: Secure this device" }).click();
  await next.getByLabel("Link from your other device").fill(link);
  await next.getByRole("button", { name: "Continue" }).click();
  await expect(existing.getByText(/is asking to join your account/)).toBeVisible({ timeout: 60_000 });
  await existing.getByRole("button", { name: "The codes match. Link it" }).click();
  await expect(existing.getByText("The new device is linked.").first()).toBeVisible({ timeout: 60_000 });
  await expect(next.getByRole("button", { name: "Settings" })).toBeVisible({ timeout: 60_000 });
  return next;
}

test("a device that cannot be told yet shows Not told yet, with Try now, and the strict option is a setting", async ({ browser }) => {
  test.setTimeout(240_000);
  const first = await newDevice(browser, BASE);
  await first.goto("/");
  await first.getByRole("button", { name: "Get started" }).first().click();
  await walkSetup(first, { stopAt: "invite", name: "Maya" });
  await first.getByRole("button", { name: "Skip for now" }).click();
  await first.getByRole("button", { name: "Settings" }).click();
  await first.getByRole("button", { name: "Devices" }).filter({ hasText: "The phones" }).click();

  // The first link: there is no other device to tell.
  await joinAndApprove(browser, first, await showLink(first));
  await first.getByRole("button", { name: "Back to devices" }).first().click();
  await expect(first.getByText("Not told yet")).toHaveCount(0);

  // The second link is approved even though the first new device has no route to hear about it.
  await joinAndApprove(browser, first, await showLink(first));
  await first.getByRole("button", { name: "Back to devices" }).first().click();
  const owed = first.getByRole("button").filter({ hasText: "Not told yet" });
  await expect(owed).toHaveCount(1);
  await owed.click();
  await expect(first.getByText(/hasn't heard about the new device yet/)).toBeVisible();
  const tryNow = first.getByRole("button", { name: "Try now" });
  await expect(tryNow).toBeVisible();
  await tryNow.click();
  await expect(first.getByText(/hasn't heard about the new device yet/)).toBeVisible(); // still not told: nothing is cleared silently
  await first.getByRole("button", { name: "Back to devices" }).first().click();
  await expect(first.getByRole("button").filter({ hasText: "Not told yet" })).toHaveCount(1);

  // The strict option is off by default and is read back from Core.
  const strict = first.getByRole("checkbox", { name: /Only add a device when all my other devices can be told right away/ });
  await expect(strict).not.toBeChecked();
  await strict.click(); // controlled by Core: it turns on once Core reports it
  await expect(strict).toBeChecked({ timeout: 30_000 });
  // Turning it off loosens what a link needs: one explicit confirmation first; "Keep it on" changes nothing.
  await strict.click();
  await first.getByRole("button", { name: "Keep it on" }).click();
  await expect(strict).toBeChecked();
  await strict.click();
  await first.getByRole("button", { name: "Yes, turn it off" }).click();
  await expect(strict).not.toBeChecked({ timeout: 30_000 });
});
