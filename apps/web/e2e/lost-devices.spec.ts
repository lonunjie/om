// "I lost my devices" (ADR-0200, ADR-0202, ADR-0203): the first run offers it quietly beside the two equal choices, the device is
// protected first, and the key screen asks for the recovery key in plain words. The real Core runs against the local reference Home.
// This covers the entry path in a real browser. The full owner recovery (record found, timer, approval by a surviving device) is
// covered by the om-core loopback tests (crates/om-core/tests/recovery_approval_flow.rs); the local reference Home in this
// harness does not implement the recovery store, so the ceremony is not driven end to end here.
import { expect, test } from "@playwright/test";
import { newDevice } from "./invite-flow.ts";
import { injectAxe } from "./helpers.ts";

test.use({ baseURL: "http://localhost:4177" });
const BASE = "http://localhost:4177";

test("a person who lost every device reaches the key screen by keyboard, after the device is protected", async ({ browser }) => {
  const page = await newDevice(browser, BASE);
  await page.goto("/");
  await page.getByRole("button", { name: "Get started" }).first().click();
  await expect(page.getByRole("button", { name: "Start new" })).toBeVisible();
  const lost = page.getByRole("button", { name: "I lost my devices" });
  await lost.focus();
  await page.keyboard.press("Enter");
  await page.getByRole("button", { name: "Next: Secure this device" }).click();
  await expect(page.getByRole("heading", { name: "Get your account back" })).toBeVisible({ timeout: 60_000 });
  await expect(page.getByLabel("Your recovery key")).toBeVisible();
  const text = (await page.getByRole("heading", { name: "Get your account back" }).locator("xpath=..").innerText()).toLowerCase();
  for (const word of ["epoch", "pin", "cbor", "relay", "dossier"]) expect(text).not.toMatch(new RegExp(`\\b${word}\\b`));
  await injectAxe(page);
  const violations = await page.evaluate(async () => {
    const axe = (globalThis as unknown as { axe: { run: (c: Element, o: object) => Promise<{ violations: { id: string }[] }> } }).axe;
    return (await axe.run(document.body, { rules: { "color-contrast": { enabled: false }, region: { enabled: false } } })).violations.map((v) => v.id);
  });
  expect(violations).toEqual([]);
  // Nothing is sent for an empty key.
  await page.getByRole("button", { name: "Find my account" }).click();
  await expect(page.getByText("No saved account was found for this key.")).toBeVisible();
  await page.getByRole("button", { name: "Back" }).click();
  await expect(page.getByRole("heading", { name: "Get your account back" })).toBeHidden();
});
