// The Omnibar (find or do) and the new-conversation action, on the real Core against the local reference Home. The default
// size is a phone, so this is the compact shell: the Omnibar trigger sits in the composer inside a chat and in the lower zone on the list.
import { expect, test } from "@playwright/test";
import { inviteAndChat, shot } from "./invite-flow.ts";

const BASE = "http://localhost:4177";
test.use({ baseURL: BASE });

test("Omnibar: open from a chat, find a setting, Ctrl+K from anywhere; new conversation stays reachable after the first chat", async ({ browser }) => {
  const { alice } = await inviteAndChat(browser, BASE);

  // Inside a chat the trigger sits beside Send; opening it takes over the lower zone and the draft is kept.
  const box = alice.getByRole("textbox", { name: /message/i });
  await box.fill("draft stays");
  await alice.getByRole("button", { name: "Open Omnibar" }).click();
  const omni = alice.getByRole("combobox", { name: "Omnibar" });
  await expect(omni).toBeFocused();
  await expect(alice.getByRole("textbox", { name: /message/i })).toBeHidden(); // one text entry at a time
  await shot(alice, "20-omnibar-open");
  await alice.keyboard.type("deliv");
  await expect(alice.getByRole("option", { name: /Message delivery/ })).toBeVisible();
  await alice.keyboard.press("Enter");
  await expect(alice.getByRole("heading", { name: "Message delivery" }).first()).toBeVisible();
  expect(new URL(alice.url()).pathname).toBe("/settings/delivery");

  // Escape closes it and gives the focus back; the draft is exactly as it was.
  await alice.keyboard.press("Control+K");
  await expect(omni).toBeFocused();
  await alice.keyboard.press("Escape");
  await expect(omni).toBeHidden();
  await alice.keyboard.press("Control+K");
  await alice.keyboard.type("bob");
  await alice.keyboard.press("Enter");
  await expect(alice.getByRole("textbox", { name: /message/i })).toHaveValue("draft stays");

  // Back on the list there is a new-conversation action even though a chat exists, and it offers the three audited choices.
  await alice.keyboard.press("Control+K");
  await alice.keyboard.type("chats");
  await alice.keyboard.press("Enter");
  await expect(alice.getByRole("listbox")).toBeVisible();
  await shot(alice, "21-list-with-actions");
  await alice.getByRole("button", { name: "New conversation" }).click();
  const sheet = alice.getByRole("dialog", { name: "New conversation" });
  await expect(sheet.getByRole("button", { name: "Share invite link" })).toBeVisible();
  await expect(sheet.getByRole("button", { name: "Show my QR" })).toBeVisible();
  await sheet.getByRole("button", { name: "Show my QR" }).click();
  await expect(sheet.getByRole("figure", { name: "QR code for your invite" })).toBeVisible();
  await shot(alice, "22-new-conversation");
  await alice.keyboard.press("Escape");
  await expect(sheet).toBeHidden();

  // The Omnibar reaches the same flow ("Scan QR code" from the list).
  await alice.getByRole("button", { name: /Omnibar/ }).click();
  await alice.keyboard.type("scan");
  await alice.keyboard.press("Enter");
  await expect(alice.getByRole("dialog", { name: "New conversation" })).toBeVisible();
});
