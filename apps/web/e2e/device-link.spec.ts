// Linking a second device (deviation D24): the existing device shows the link, the new device types it, both show the same
// code, the existing device approves, the new device is in and the old one tells the person. Two browser contexts (two
// devices, two storages) on the real Core connected to the local reference Home; the only thing passed between them is the
// link a person would scan or type.
//
// KNOWN GAP (the second test is fixme): the reference Home's deposit validator (`parseDeposit` in
// packages/om-home-wire/src/sealed.ts, used by infra/cloudflare-home/src/queue.ts) accepts only a SealedDeliveryEnvelope
// (type 23) or a MessageRequest (type 19). Core's ceremony messages are neither, so the Home answers `invalid_request` to the
// new device's deposit and the person sees "Could not start". The Home (infra/, om-home-wire) needs a ceremony-message type
// before the full ceremony can run against a real Home. The ceremony itself is covered end to end in the Rust Core tests
// (crates/om-core/tests/device_link.rs) and at the services boundary here by the harness tests in packages/om-ui.
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

test.fixme("a second device joins with the code and link, after both show the same security code (needs a Home that accepts ceremony messages)", async ({ browser }) => {
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
  await first.getByRole("button", { name: "Back to devices" }).click();
  await expect(first.getByText(/A new device was added to your account/)).toBeVisible({ timeout: 30_000 });
});
