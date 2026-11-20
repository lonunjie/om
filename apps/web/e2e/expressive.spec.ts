// Keyboard-only flow on the opt-in loopback harness. The dev entry supplies local fixture bytes.
import { expect, test, type Locator, type Page } from "@playwright/test";
import { noPasskeys } from "./helpers.ts";

// Firefox does not wrap Tab past the last control: focus leaves the page for the browser's own UI and
// `document.activeElement` stays put. When Tab stops moving focus, walk back with Shift+Tab instead.
async function tabTo(page: Page, target: Locator): Promise<void> {
  await expect(target).toBeVisible();
  const active = () => page.evaluate(() => {
    const el = document.activeElement;
    return el === null || el === document.body ? null : Array.prototype.indexOf.call(document.querySelectorAll("*"), el);
  });
  let key = "Tab";
  for (let n = 0; n < 100; n++) {
    if (await target.evaluate((el) => el === document.activeElement)) return;
    const before = await active();
    await page.keyboard.press(key);
    if (key === "Tab" && before !== null && await active() === before) key = "Shift+Tab";
  }
  await expect(target).toBeFocused();
}

async function activate(page: Page, target: Locator): Promise<void> {
  await tabTo(page, target);
  await page.keyboard.press("Enter");
}

test("keyboard can send a sticker, add a custom reaction and remove the pack", async ({ page }) => {
  await noPasskeys(page);
  await page.emulateMedia({ reducedMotion: "reduce" });
  await page.goto("/?expressive");
  await activate(page, page.getByRole("button", { name: "Get started" }));
  await tabTo(page, page.getByRole("textbox", { name: "Your name" }));
  await page.keyboard.type("Tester");
  await activate(page, page.getByRole("button", { name: "Next: Secure your identity" }));
  await activate(page, page.getByRole("button", { name: "Skip for now" }));
  await activate(page, page.getByRole("list", { name: "Identities" }).getByRole("button", { name: "Work" }));
  const chats = page.getByRole("listbox", { name: "Conversations" });
  const ana = chats.getByRole("option", { name: /Ana Reyes/ });
  await tabTo(page, chats);
  await expect(ana).toBeVisible();
  const targetId = await ana.getAttribute("id");
  if (targetId === null) throw new Error("conversation option has no focus target");
  await page.keyboard.press("Home");
  const count = await chats.getByRole("option").count();
  for (let i = 0; i < count && await chats.getAttribute("aria-activedescendant") !== targetId; i++) {
    await page.keyboard.press("ArrowDown");
  }
  await expect(chats).toHaveAttribute("aria-activedescendant", targetId);
  await page.keyboard.press("Enter");

  const stickers = page.getByRole("button", { name: "Stickers", exact: true });
  const log = page.getByRole("log", { name: "Messages" });
  await activate(page, stickers);
  const picker = page.getByRole("dialog", { name: "Stickers" });
  await activate(page, picker.getByRole("button", { name: "Review pack" }));
  await expect(picker.getByText("Signed by", { exact: true })).toBeVisible();
  await expect(picker.getByText("License stated by the pack", { exact: false })).toBeVisible();
  await activate(page, picker.getByRole("button", { name: "Install Little faces" }));
  await activate(page, picker.getByRole("button", { name: "Little faces" }));
  await activate(page, picker.getByRole("button", { name: "Send Hello smile" }));
  await expect(picker.getByText("Sticker sent.")).toBeVisible();
  await page.keyboard.press("Escape");
  await expect(stickers).toBeFocused();
  const sticker = log.getByRole("img", { name: "A smiling sticker" });
  await expect(sticker).toBeVisible();
  // `has` is matched inside each list item, so it must not be scoped to the log again.
  const message = log.getByRole("listitem").filter({ has: page.getByRole("img", { name: "A smiling sticker" }) });
  await activate(page, message.getByRole("button", { name: /^React:/ }));
  await activate(page, message.getByRole("button", { name: "Custom reactions" }));
  const reactions = page.getByRole("dialog", { name: "Custom reactions" });
  await activate(page, reactions.getByRole("button", { name: "Little faces" }));
  await activate(page, reactions.getByRole("button", { name: "React with Cheer" }));
  await expect(reactions.getByText("Reaction added.")).toBeVisible();
  await page.keyboard.press("Escape");
  await expect(message.getByRole("img", { name: "A cheering reaction" })).toBeVisible();

  await activate(page, stickers);
  await activate(page, picker.getByRole("button", { name: "Little faces" }));
  await activate(page, picker.getByRole("button", { name: "About Little faces" }));
  await activate(page, picker.getByRole("button", { name: "Remove Little faces" }));
  await expect(picker.getByText("Remove Little faces from this device?")).toBeVisible();
  await activate(page, picker.getByRole("button", { name: "Remove Little faces" }));
  await expect(picker.getByText("Pack removed from this device.")).toBeVisible();
  await page.keyboard.press("Escape");
  await expect(sticker).toBeVisible();
});
