// The human path for two people who have never used OM, shared by the hermetic spec (local Home) and the opt-in live spec
// (the deployed Home). Two separate browser contexts (two devices, two storages); the only thing passed between them is the
// link a person would send through any chat app.
import { expect, type Browser, type Page } from "@playwright/test";
import { noPasskeys, walkSetup } from "./helpers.ts";

const PHONE = { width: 390, height: 844 };
const DESKTOP = { width: 1280, height: 800 };
/** `OM_SHOTS=<dir>` saves a screenshot of each step (`OM_VIEW=desktop` for 1280x800) so the screens can be looked at. */
const SHOTS = process.env["OM_SHOTS"];
const size = process.env["OM_VIEW"] === "desktop" ? DESKTOP : PHONE;
export async function shot(page: Page, name: string): Promise<void> {
  if (SHOTS === undefined) return;
  await page.waitForTimeout(500); // let the arrival animation finish
  await page.screenshot({ path: `${SHOTS}/${name}-${String(size.width)}.png`, fullPage: false });
}

export async function newDevice(browser: Browser, base: string): Promise<Page> {
  // Only Chromium has clipboard permissions to grant; elsewhere the copy still runs, it is just not read back.
  const permissions = browser.browserType().name() === "chromium" ? ["clipboard-read", "clipboard-write"] : [];
  const ctx = await browser.newContext({ viewport: size, baseURL: base, permissions });
  const page = await ctx.newPage();
  await noPasskeys(page);
  return page;
}

export async function inviteAndChat(browser: Browser, base: string): Promise<{ readonly alice: Page; readonly bob: Page }> {
  const alice = await newDevice(browser, base);
  await alice.goto("/");
  await alice.getByRole("button", { name: "Get started" }).first().click();
  await walkSetup(alice, { stopAt: "invite", name: "Alice" });

  // Alice copies her link with one tap and gets told what to do with it.
  await shot(alice, "01-alice-ready");
  await alice.getByRole("button", { name: "Copy link" }).click();
  await expect(alice.getByText(/Link copied/)).toBeAttached(); // a green tick for the eye, the words for a screen reader
  await alice.getByRole("button", { name: "Show as QR code" }).click();
  await shot(alice, "02-alice-link");
  const link = (await alice.locator("code[translate=no]").first().textContent())?.trim() ?? "";
  expect(link).toMatch(new RegExp(`^${base.replace(/[.*+?^${}()|[\]\\]/g, "\\$&")}/#invite/[A-Za-z0-9_-]{22,}$`));
  if (browser.browserType().name() === "chromium") expect(await alice.evaluate(() => navigator.clipboard.readText())).toBe(link);
  await alice.getByRole("button", { name: "Skip for now" }).click();
  await expect(alice.getByRole("button", { name: "Settings" })).toBeVisible();

  // Bob opens that link on his own device. He goes straight to "What should we call you?", with no token left in the address.
  const bob = await newDevice(browser, base);
  await bob.goto(link);
  await expect(bob.getByRole("heading", { name: "What should we call you?" })).toBeVisible();
  expect(new URL(bob.url()).hash).toBe("");
  await shot(bob, "04-bob-name");
  // Pressing the button without a name says why, instead of doing nothing.
  await bob.getByRole("button", { name: "Next: Secure your identity" }).click();
  await expect(bob.getByRole("alert")).toContainText("Type your name first");
  await bob.getByLabel("Your name").fill("Bob");
  await bob.getByRole("button", { name: "Next: Secure your identity" }).click();

  // After setup his one decision is "Accept this invite?"; nothing was sent before this.
  await expect(bob.getByRole("heading", { name: "Accept this invite?" })).toBeVisible({ timeout: 60_000 });
  await shot(bob, "05-bob-accept");
  await bob.getByRole("button", { name: "Accept invite" }).click();
  // Bob lands in the new chat, which says plainly that it is waiting for Alice. He has nothing else to do.
  await expect(bob.getByRole("heading", { name: "Alice" })).toBeVisible({ timeout: 60_000 });
  await expect(bob.getByText(/Waiting for them to accept/)).toBeVisible();
  await shot(bob, "06-bob-waiting");

  // Alice sees the request arrive by itself and approves it.
  const request = alice.getByRole("region", { name: "Message request" });
  await expect(request).toBeVisible({ timeout: 60_000 });
  await expect(request).toContainText("Bob");
  await shot(alice, "07-alice-request");
  await request.getByRole("button", { name: "Accept", exact: true }).click();
  await expect(alice.getByRole("option")).toHaveCount(1, { timeout: 60_000 });

  // Messages flow both ways. The first one waits for the secure connection to finish, as a person would.
  await alice.getByRole("option").click();
  const box = alice.getByRole("textbox", { name: /message/i });
  await expect(async () => {
    await box.fill("hello bob");
    await alice.getByRole("button", { name: /send/i }).click();
    await expect(alice.getByText("hello bob")).toBeVisible({ timeout: 3_000 });
  }).toPass({ timeout: 60_000 });
  await expect(bob.getByText("hello bob")).toBeVisible({ timeout: 60_000 });
  await bob.getByRole("textbox", { name: /message/i }).fill("hi alice");
  await bob.getByRole("button", { name: /send/i }).click();
  await expect(alice.getByText("hi alice")).toBeVisible({ timeout: 60_000 });
  await shot(bob, "08-bob-chat");
  // With OM_SHOTS set, also look at the places a person reaches from Settings (addresses are in route/place.ts).
  if (SHOTS !== undefined) {
    for (const [n, path] of [["09-settings", "/settings"], ["10-delivery", "/settings/delivery"], ["11-devices", "/settings/devices"], ["12-recovery", "/settings/recovery"], ["13-lock", "/settings/lock"]] as const) {
      await alice.goto(path);
      await shot(alice, n);
    }
  }
  return { alice, bob };
}
