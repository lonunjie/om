// Moving around: every place has a web address, Back/Forward and a reload land where a person expects, and a bad address is
// never an error page. Runs against the production entry connected to the local reference Home.
import { expect, test, type Page } from "@playwright/test";
import { inviteAndChat, newDevice, shot } from "./invite-flow.ts";
import { walkSetup } from "./helpers.ts";

const BASE = "http://localhost:4177";
test.use({ baseURL: BASE });

const path = (p: Page): string => { const u = new URL(p.url()); return u.pathname + u.hash; };

async function unlockIfAsked(page: Page): Promise<void> {
  const unlock = page.getByRole("button", { name: "Unlock", exact: true });
  const asked = await unlock.waitFor({ timeout: 20_000 }).then(() => true, () => false);
  if (asked) await unlock.click();
}

test("settings pages have addresses; Back and Forward walk them; a reload stays put; unknown addresses fall back to the root", async ({ browser }) => {
  const me = await newDevice(browser, BASE);
  await me.goto("/");
  await me.getByRole("button", { name: "Get started" }).first().click();
  await walkSetup(me, { name: "Maya" });
  expect(path(me)).toBe("/");
  await expect(me).toHaveTitle("Chats · OM");

  await me.getByRole("button", { name: "Settings" }).click();
  await expect(me.getByRole("heading", { name: "Settings", level: 1 })).toBeVisible();
  expect(path(me)).toBe("/settings");
  await expect(me).toHaveTitle("Settings · OM");
  await shot(me, "10-settings");

  await me.getByRole("button", { name: /Message delivery/ }).click();
  expect(path(me)).toBe("/settings/delivery");
  await expect(me.getByRole("heading", { name: "Message delivery" }).first()).toBeVisible();
  await shot(me, "11-delivery");
  await me.goBack(); // the phone's Back button: delivery -> the settings menu -> chats
  await expect(me.getByRole("heading", { name: "Settings", level: 1 })).toBeVisible();
  expect(path(me)).toBe("/settings");
  await me.goBack();
  await expect(me.getByRole("heading", { name: "Chats", level: 1 })).toBeVisible();
  expect(path(me)).toBe("/");
  await me.goForward();
  await me.goForward();
  await expect(me.getByRole("heading", { name: "Message delivery" }).first()).toBeVisible();
  expect(path(me)).toBe("/settings/delivery");

  // A reload at a deep address comes back to the same place (after the unlock the locked vault asks for).
  await me.goto("/settings/recovery");
  await unlockIfAsked(me);
  await expect(me.getByRole("heading", { name: /recovery|safety code/i }).first()).toBeVisible({ timeout: 60_000 });
  expect(path(me)).toBe("/settings/recovery");
  await me.reload();
  await unlockIfAsked(me);
  await expect(me.getByRole("heading", { name: /recovery|safety code/i }).first()).toBeVisible({ timeout: 60_000 });

  // Nonsense is the root, not an error.
  for (const bad of ["/nope", "/settings/nope", "/chats#no-such-chat"]) {
    await me.goto(bad);
    await unlockIfAsked(me);
    await expect(me.getByRole("heading", { name: "Chats", level: 1 })).toBeVisible({ timeout: 60_000 });
    await expect.poll(() => path(me)).toBe("/");
  }
});

test("a chat has an address that never reaches the host, and Back leaves it for the list", async ({ browser }) => {
  const { bob } = await inviteAndChat(browser, BASE);
  const seen: string[] = [];
  bob.on("request", (r) => { seen.push(r.url()); });
  // Bob is in the chat now: the fragment names it, the path does not.
  expect(new URL(bob.url()).pathname).toBe("/chats");
  expect(new URL(bob.url()).hash).toMatch(/^#.+/);
  expect(seen.some((u) => u.includes("#"))).toBe(false);
  // Beside the list (a wide window) there is no on-screen Back; the browser's Back does the same.
  if ((bob.viewportSize()?.width ?? 0) >= 900) await bob.goBack();
  else await bob.getByRole("button", { name: "Back to chats" }).click();
  await expect(bob.getByRole("listbox")).toBeVisible();
  await expect.poll(() => path(bob)).toBe("/");
  await bob.goForward();
  await expect(bob.getByRole("textbox", { name: /message/i })).toBeVisible();
  expect(new URL(bob.url()).pathname).toBe("/chats");
  // The phone's Back button does the same as the on-screen one.
  await bob.goBack();
  await expect(bob.getByRole("listbox")).toBeVisible();
  // Opening the chat's own address in a new tab lands in the chat once unlocked.
  await bob.goForward();
  const link = bob.url();
  const tab = await bob.context().newPage();
  await tab.goto(link);
  await unlockIfAsked(tab);
  await expect(tab.getByRole("textbox", { name: /message/i })).toBeVisible({ timeout: 60_000 });
  await expect(tab.getByText("hello bob")).toBeVisible();
});
