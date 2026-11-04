// Linking a second device (deviation D24): the existing device shows the link, the new device types it, both show the same
// code, the existing device approves, the new device is in and the old one tells the person. Two browser contexts (two
// devices, two storages) on the real Core connected to the local reference Home; the only thing passed between them is the
// link a person would scan or type.
// Core wraps every ceremony message in a structurally valid profile-1 SealedDeliveryEnvelope (D24 note), so the reference
// Home accepts the deposits; it never opens them.
import { expect, test, type Page } from "@playwright/test";
import { newDevice } from "./invite-flow.ts";
import { walkSetup } from "./helpers.ts";

test.use({ baseURL: "http://localhost:4177" });
const BASE = "http://localhost:4177";

/** A person with a finished first run opens Settings > Devices > Add a device and asks for the code. Returns the link. */
async function existingDeviceShowsLink(page: Page): Promise<string> {
  await page.goto("/");
  await page.getByRole("button", { name: "Get started" }).first().click();
  await walkSetup(page, { stopAt: "invite", name: "Maya" });
  await page.getByRole("button", { name: "Skip for now" }).click();
  await page.getByRole("button", { name: "Settings" }).click();
  await page.getByRole("button", { name: "Devices" }).filter({ hasText: "The phones" }).click();
  await page.getByRole("button", { name: "Add a device" }).click();
  await page.getByRole("button", { name: "Show the code and link" }).click();
  const qr = page.locator("figure[data-kind=device-link]");
  await expect(qr).toBeVisible({ timeout: 60_000 });
  // Visibly not a contact invitation: a text badge and a warning, not only a colour.
  await expect(qr).toContainText("Adds a device to your account");
  await expect(page.getByText("This is not an invitation to chat.")).toBeVisible();
  const link = (await page.locator("code.om-devices__link").textContent())?.trim() ?? "";
  expect(link).toMatch(/^om:link\/[A-Za-z0-9_-]{16,200}$/);
  expect(link.length).toBeLessThanOrEqual(256);
  return link;
}

test("the existing device shows a distinct device-link QR and link; the new device's first run offers both choices and refuses an invitation link", async ({ browser }) => {
  const first = await newDevice(browser, BASE);
  const link = await existingDeviceShowsLink(first);

  const second = await newDevice(browser, BASE);
  await second.goto("/");
  await second.getByRole("button", { name: "Get started" }).first().click();
  await expect(second.getByRole("button", { name: "Start new" })).toBeVisible();
  await second.getByRole("button", { name: "I already use OM" }).click();
  await second.getByRole("button", { name: "Next: Secure this device" }).click();
  await expect(second.getByRole("heading", { name: "Link to your account" })).toBeVisible({ timeout: 60_000 });
  await second.getByLabel("Link from your other device").fill(`${BASE}/#invite/AAAAAAAAAAAAAAAAAAAAAA`);
  await second.getByRole("button", { name: "Continue" }).click();
  await expect(second.getByRole("alert")).toContainText("does not look like an OM device link");
  expect(link).toMatch(/^om:link\//);
});

test("a second device joins with the code and link, after both show the same security code", async ({ browser }) => {
  const first = await newDevice(browser, BASE);
  const link = await existingDeviceShowsLink(first);

  const second = await newDevice(browser, BASE);
  await second.goto("/");
  await second.getByRole("button", { name: "Get started" }).first().click();
  await second.getByRole("button", { name: "I already use OM" }).click();
  await second.getByRole("button", { name: "Next: Secure this device" }).click();
  await second.getByLabel("Link from your other device").fill(link);
  await second.getByRole("button", { name: "Continue" }).click();
  const theirCode = (await second.locator("output").first().textContent())?.trim() ?? "";
  expect(theirCode).toMatch(/^\d{6}$|^\d{3} \d{3}$/);

  await expect(first.getByText(/is asking to join your account/)).toBeVisible({ timeout: 60_000 });
  await expect(first.locator("output").first()).toHaveText(theirCode);
  await first.getByRole("button", { name: "The codes match. Link it" }).click();
  await expect(first.getByText("The new device is linked.").first()).toBeVisible({ timeout: 60_000 });
  await expect(second.getByRole("button", { name: "Settings" })).toBeVisible({ timeout: 60_000 });
  await first.getByRole("button", { name: "Back to devices" }).first().click();
  await expect(first.getByText(/A new device was added to your account/)).toBeVisible({ timeout: 30_000 });
});
