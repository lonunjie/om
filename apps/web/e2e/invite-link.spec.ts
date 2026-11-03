// Two people who have never used OM message each other from a shared link, against the local reference Home.
import { expect, test } from "@playwright/test";
import { inviteAndChat, newDevice } from "./invite-flow.ts";
import { walkSetup } from "./helpers.ts";

const BASE = "http://localhost:4177";
test.use({ baseURL: BASE });

test("a shared link is enough: open it, accept, get approved, and chat", async ({ browser }) => { await inviteAndChat(browser, BASE); });

test("an unsent draft is still there after the page is reloaded, and a sent one is not", async ({ browser }) => {
  const { alice } = await inviteAndChat(browser, BASE);
  const box = alice.getByRole("textbox", { name: /message/i });
  await box.fill("not sent yet");
  await alice.waitForTimeout(1_500); // drafts are written a moment after typing stops
  await alice.reload();
  await expect(box).toHaveValue("not sent yet", { timeout: 60_000 });
  await alice.getByRole("button", { name: /send/i }).click();
  await expect(box).toHaveValue("");
  await alice.waitForTimeout(1_500);
  await alice.reload();
  await expect(box).toBeVisible({ timeout: 60_000 });
  await expect(box).toHaveValue("");
});

test("Settings has a Help and feedback page that points at GitHub", async ({ browser }) => {
  const alice = await newDevice(browser, BASE);
  await alice.goto("/");
  await alice.getByRole("button", { name: "Get started" }).first().click();
  await walkSetup(alice, { stopAt: "invite", name: "Alice" });
  await alice.getByRole("button", { name: "Skip for now" }).click();
  await alice.getByRole("button", { name: "Settings" }).click();
  await alice.getByRole("button", { name: /Help and feedback/ }).click();
  await expect(alice.getByRole("heading", { name: "Help and feedback", level: 1 })).toBeVisible();
  const report = alice.getByRole("link", { name: /Report a problem/ });
  await expect(report).toHaveAttribute("href", /^https:\/\/github\.com\/lonunjie\/om\/issues\/new\?template=bug\.yml&version=\d+\.\d+\.\d+/);
  await expect(alice.getByRole("link", { name: /Suggest an idea/ })).toHaveAttribute("target", "_blank");
});

test("someone already set up opens a link, can say not now, and an unusable link says why", async ({ browser }) => {
  const alice = await newDevice(browser, BASE);
  await alice.goto("/");
  await alice.getByRole("button", { name: "Get started" }).first().click();
  await walkSetup(alice, { stopAt: "invite", name: "Alice" });
  await alice.getByRole("button", { name: "Copy link" }).click();
  const link = (await alice.locator("code[translate=no]").first().textContent())?.trim() ?? "";
  await alice.getByRole("button", { name: "Skip for now" }).click();
  await expect(alice.getByRole("button", { name: "Settings" })).toBeVisible();

  // Alice opening her own link: Accept says so in plain words instead of failing silently.
  // The link is opened in a fresh tab of the same browser: the vault is locked, so she unlocks first and the question follows.
  const tab = await alice.context().newPage();
  await tab.goto(link);
  await expect(tab.getByRole("heading", { name: "Accept this invite?" })).toBeVisible({ timeout: 60_000 }); // the key in this browser opens by itself
  await tab.getByRole("button", { name: "Accept invite" }).click();
  await expect(tab.getByRole("alert")).toContainText(/not your own invite|your own invite/i, { timeout: 30_000 });
  await tab.getByRole("button", { name: "Not now" }).click();
  await expect(tab.getByRole("heading", { name: "Accept this invite?" })).toHaveCount(0);
  await expect(tab.getByRole("heading", { name: "Invite someone to chat" })).toBeVisible();
});
