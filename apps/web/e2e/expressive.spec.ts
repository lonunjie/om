// Keyboard-only flow on the opt-in loopback harness. The dev entry supplies local fixture bytes.
import { readFileSync } from "node:fs";
import { expect, test, type Locator, type Page } from "@playwright/test";
import { noPasskeys } from "./helpers.ts";
import { GROUPS_APP, closeAll, createGroup, firstLink, hears, joinByLink, joinGroup, openGroup, readyGroups, roomLog, say, startLockedPerson, type Person } from "./groups-live-people.ts";

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

test.describe("Room attachments on the opt-in Core", () => {
  test.skip(process.env["OM_EXPRESSIVE_E2E"] !== "1", "Uses the expressive relay bundle");
  test.setTimeout(240_000);
  const everyone: Person[] = [];
  test.afterEach(async () => { await closeAll(everyone.splice(0)); });

  for (const recipient of ["expressive", "plain"] as const) {
    test(`a Room sticker with recipient mode ${recipient}`, async ({ browser }) => {
      // Files need a Home that holds them: the locked Home, reached through the first-run setup bundle. A Home without the blob
      // spool makes Core refuse the upload (no_blob_store), by design.
      const alice = await startLockedPerson(browser, "Alice");
      everyone.push(alice);
      await alice.page.emulateMedia({ reducedMotion: "reduce" });
      const invite = new URL(await firstLink(alice));
      let bob: Person;
      if (recipient === "expressive") {
        bob = await startLockedPerson(browser, "Bob");
        everyone.push(bob);
        await bob.page.getByText("Have an invite?").click();
        await bob.page.getByLabel("Invitation link").fill(invite.href);
        await bob.page.getByRole("button", { name: "Accept invitation" }).click();
        await expect(bob.page.getByRole("button", { name: "Settings" })).toBeVisible({ timeout: 60_000 });
        const request = alice.page.getByRole("region", { name: "Message request" }).filter({ hasText: "Bob" });
        await expect(request).toBeVisible({ timeout: 60_000 });
        await request.getByRole("button", { name: "Accept", exact: true }).click();
        await expect(alice.page.getByRole("option", { name: /Bob/ })).toBeVisible({ timeout: 60_000 });
      } else {
        bob = await joinByLink(browser, "Bob", `${GROUPS_APP}/${invite.hash}`, alice);
        everyone.push(bob);
      }
      await bob.page.emulateMedia({ reducedMotion: "reduce" });
      if (recipient === "plain") await readyGroups(bob);

      const vectors = JSON.parse(readFileSync(new URL("../../../verification/vectors/pack-file-vectors.json", import.meta.url), "utf8")) as {
        vectors: { id: string; hex: string }[];
      };
      const genesis = vectors.vectors.find(({ id }) => id === "genesis");
      if (genesis === undefined) throw new Error("pack fixture unavailable");
      const packBytes = [...Buffer.from(genesis.hex, "hex")];
      const dropped = await alice.page.evaluateHandle((bytes) => {
        const data = new DataTransfer();
        data.items.add(new File([Uint8Array.from(bytes)], "faces.ompack"));
        return data;
      }, packBytes);
      const dropTarget = alice.page.getByText("Drop a pack file here.", { exact: true });
      await dropTarget.dispatchEvent("drop", { dataTransfer: dropped });
      const review = alice.page.getByRole("dialog", { name: "Review pack" });
      await expect(review.getByRole("heading", { name: "Little faces" })).toBeFocused();
      await expect(review.getByText("Signed by", { exact: true })).toBeVisible();
      await alice.page.keyboard.press("Escape");
      await expect(review).toBeHidden();
      await expect(alice.page.getByRole("button", { name: "Choose pack file" })).toBeFocused();
      await dropTarget.dispatchEvent("drop", { dataTransfer: dropped });
      await review.getByRole("button", { name: "Install Little faces" }).click();
      await expect(review.getByText("Pack saved on this device.")).toBeVisible();
      await review.getByRole("button", { name: "Close", exact: true }).click();
      await dropped.dispose();

      await createGroup(alice, "Sticker group", ["Bob"]);
      await joinGroup(bob, "Sticker group", "Alice");
      await openGroup(alice, "Sticker group");
      await say(bob, "Ready for a sticker");
      await hears(alice, "Ready for a sticker");
      const draft = alice.page.getByRole("textbox", { name: "Message the group" });
      await draft.fill("A separate draft");
      const stickers = alice.page.getByRole("button", { name: "Stickers", exact: true });
      await activate(alice.page, stickers);
      const picker = alice.page.getByRole("dialog", { name: "Stickers" });
      await activate(alice.page, picker.getByRole("button", { name: "Little faces" }));
      await expect(picker.getByRole("checkbox", { name: "Allow reuse of this upload" })).not.toBeChecked();
      await activate(alice.page, picker.getByRole("button", { name: "Send Hello smile" }));
      await expect(picker.getByText("Sticker sent.")).toBeVisible();
      await alice.page.keyboard.press("Escape");
      await expect(stickers).toBeFocused();
      await expect(draft).toHaveValue("A separate draft");
      await hears(bob, "🙂");
      const fallback = roomLog(alice).getByText("Sent as an image with text.");
      if (recipient === "plain") {
        await expect(fallback).toBeVisible();
        await expect(roomLog(bob).getByRole("button", { name: /^Download/ })).toBeVisible();
      } else {
        await expect(fallback).toHaveCount(0);
        await roomLog(bob).getByRole("button", { name: "Show image" }).click();
        await expect(roomLog(bob).getByRole("img", { name: "A smiling sticker" })).toBeVisible();
      }
    });
  }
});
