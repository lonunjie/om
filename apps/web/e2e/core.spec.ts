// The real Core (om_wasm.wasm) in a real browser: two identities in one browser (the V1 profile switcher, ADR-0148),
// each with its own vault, talking through the TEST loopback provider. Everything user-visible goes through the same
// shell and typed intents as production; only the transport is the loopback.
import { expect, test, type Page } from "@playwright/test";
import { noPasskeys, onboard, walkSetup } from "./helpers.ts";

test.use({ baseURL: "http://localhost:4176" });

// Both identities carry the default label, so the test tracks which one is current: the app lists the first-created
// identity as current whenever the shell (re)mounts, which is Alice after Bob's setup.
type Who = "alice" | "bob";
async function asIdentity(page: Page, state: { who: Who }, who: Who): Promise<void> {
  if (state.who === who) return;
  await switchIdentity(page);
  state.who = who;
}

async function switchIdentity(page: Page): Promise<void> {
  await page.getByRole("button", { name: "Settings" }).waitFor();
  const other = page.getByRole("list", { name: "Identities" }).getByRole("button").and(page.locator(":not([aria-current])"));
  await other.click();
  await expect(page.getByRole("list", { name: "Identities" }).getByRole("button").and(page.locator("[aria-current]"))).toHaveCount(1);
}

test("two identities on the real Core: invite, introduction, messages both ways, sealed on the wire", async ({ page }) => {
  await noPasskeys(page);
  await expect(page.getByText("E2E LOOPBACK PROVIDER")).toHaveCount(0); // before load
  // Alice: first run, then show her invite link.
  await onboard(page, { stopAt: "invite" });
  await expect(page.getByText("E2E LOOPBACK PROVIDER, NOT PRODUCTION")).toBeVisible();
  await page.getByRole("button", { name: "Show as QR code" }).click();
  const link = (await page.locator("code[translate=no]").first().textContent())?.trim() ?? "";
  expect(link).toMatch(/^om:invite\/[A-Za-z0-9_-]+$/);
  await page.getByRole("button", { name: "Skip for now" }).click();
  await page.getByRole("button", { name: "Open my chats" }).click();
  await expect(page.getByRole("button", { name: "Settings" })).toBeVisible();

  // Bob: a second identity in the same browser accepts Alice's invite while setting up.
  await page.getByRole("button", { name: "Add identity" }).click();
  await walkSetup(page, { stopAt: "invite" });
  await page.getByLabel("Invitation link").fill(link);
  await page.getByRole("button", { name: "Accept invitation" }).click();
  await page.getByRole("button", { name: "Open my chats" }).click();
  await expect(page.getByRole("button", { name: "Settings" })).toBeVisible();

  // Alice (current again after Bob's setup) sees the request (the sender is unknown until she accepts) and accepts it.
  const me = { who: "alice" as Who };
  await page.getByRole("region", { name: "Message request" }).getByRole("button", { name: "Accept", exact: true }).click();
  await expect(page.getByRole("option")).toHaveCount(1, { timeout: 30_000 });
  await page.getByRole("option").click();
  await page.getByRole("textbox", { name: /message/i }).fill("hello from alice");
  await page.getByRole("button", { name: /send/i }).click();
  await expect(page.getByText("hello from alice")).toBeVisible();

  // Bob receives it (polling) and replies.
  await asIdentity(page, me, "bob");
  await expect(page.getByRole("option")).toHaveCount(1, { timeout: 30_000 });
  await page.getByRole("option").click();
  await expect(page.getByText("hello from alice")).toBeVisible({ timeout: 30_000 });
  await page.getByRole("textbox", { name: /message/i }).fill("hi alice, this is bob");
  await page.getByRole("button", { name: /send/i }).click();

  // Alice receives the reply.
  await asIdentity(page, me, "alice");
  await page.getByRole("option").click();
  await expect(page.getByText("hi alice, this is bob")).toBeVisible({ timeout: 30_000 });

  // What a relay saw: only sealed wrappers (plus the one MessageRequest), never plaintext.
  const seen = await page.evaluate(() => (globalThis as unknown as { __OM_E2E_LOOPBACK__: { deposited: () => string[] } }).__OM_E2E_LOOPBACK__.deposited());
  expect(seen.length).toBeGreaterThanOrEqual(5);
  const plain = (s: string): string => Array.from(s.match(/../g) ?? [], (h) => String.fromCharCode(parseInt(h, 16))).join("");
  for (const hex of seen) {
    const text = plain(hex);
    expect(text).not.toContain("hello from alice");
    expect(text).not.toContain("hi alice, this is bob");
  }

  // The profile registry and Core state survive a reload: each identity is behind its own vault, and unlocking it
  // brings back its conversation from the encrypted store (the loopback relay is gone, the local history is not).
  await page.reload();
  for (let n = 0; n < 2; n++) {
    await expect(page.getByRole("heading", { name: /unlock/i }).first()).toBeVisible({ timeout: 30_000 });
    await page.getByRole("button", { name: "Unlock", exact: true }).click();
  }
  await expect(page.getByRole("button", { name: "Settings" })).toBeVisible({ timeout: 30_000 });
  await expect(page.getByRole("option")).toHaveCount(1);
  await page.getByRole("option").click();
  await expect(page.getByText("hello from alice")).toBeVisible();
  await expect(page.getByText("hi alice, this is bob")).toBeVisible();
});
