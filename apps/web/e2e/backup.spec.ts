// A chat backup file, end to end in a real browser on the real Core: two people talk, one saves a backup file, a brand-new browser
// (a fresh install with its own identity) restores it and can read the history but not reply. Wrong passwords, a changed file and a
// file that is not a backup are refused in plain words and import nothing. The real password cost is used (64 MiB, 3 passes).
import { mkdtempSync, readFileSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { expect, test, type Page } from "@playwright/test";
import { addIdentity, noPasskeys, onboard, reopen, walkSetup } from "./helpers.ts";

const CORE = "http://localhost:4176";
test.use({ baseURL: CORE });
test.setTimeout(300_000);
const SLOW = { timeout: 120_000 };
const PASSWORD = "correct horse battery staple";

async function switchIdentity(page: Page): Promise<void> {
  await page.getByRole("button", { name: "Settings" }).waitFor();
  const other = page.getByRole("list", { name: "Identities" }).getByRole("button").and(page.locator(":not([aria-current])"));
  await other.click();
  await expect(page.getByRole("list", { name: "Identities" }).getByRole("button").and(page.locator("[aria-current]"))).toHaveCount(1);
}

/** Alice and Bob in one browser, with a short conversation in both directions. Leaves Alice as the current identity. */
async function talk(page: Page): Promise<void> {
  await noPasskeys(page);
  await onboard(page, { stopAt: "invite", name: "Alice" });
  await page.getByRole("button", { name: "Show my QR" }).click();
  const link = (await page.locator("code[translate=no]").first().textContent())?.trim() ?? "";
  await page.getByRole("button", { name: "Skip for now" }).click();
  await expect(page.getByRole("button", { name: "Settings" })).toBeVisible();
  await addIdentity(page);
  await walkSetup(page, { stopAt: "invite", name: "Bob" });
  await page.getByText("Have an invite?").click();
  await page.getByLabel("Invitation link").fill(link);
  await page.getByRole("button", { name: "Accept invitation" }).click();
  await expect(page.getByRole("button", { name: "Settings" })).toBeVisible();

  await page.getByRole("region", { name: "Message request" }).getByRole("button", { name: "Accept", exact: true }).click();
  await expect(page.getByRole("option")).toHaveCount(1, { timeout: 30_000 });
  await page.getByRole("option").click();
  await page.getByRole("textbox", { name: /message/i }).fill("hello from alice");
  await page.getByRole("button", { name: /send/i }).click();
  await expect(page.getByRole("log").getByText("hello from alice")).toBeVisible();

  await switchIdentity(page); // Bob
  await expect(page.getByRole("option")).toHaveCount(1, { timeout: 30_000 });
  await page.getByRole("option").click();
  await expect(page.getByRole("log").getByText("hello from alice")).toBeVisible({ timeout: 30_000 });
  await page.getByRole("textbox", { name: /message/i }).fill("hi alice, this is bob");
  await page.getByRole("button", { name: /send/i }).click();
  await switchIdentity(page); // Alice
  await page.getByRole("option").click();
  await expect(page.getByRole("log").getByText("hi alice, this is bob")).toBeVisible({ timeout: 30_000 });
}

async function openBackupPage(page: Page): Promise<void> {
  await page.getByRole("button", { name: "Settings" }).click();
  await page.getByRole("button", { name: /^Backup file/ }).click();
  await expect(page.getByRole("heading", { name: "Backup file", level: 1 })).toBeVisible();
}

async function restoreWith(page: Page, file: string, password: string): Promise<void> {
  await page.getByLabel("Choose the backup file").setInputFiles(file);
  await page.getByLabel("Password for this file").fill(password);
  await page.getByRole("button", { name: "Restore chats" }).click();
}

test("save a backup file, restore it on a fresh install, read the history, cannot reply; refusals import nothing", async ({ page, browser }) => {
  await talk(page);

  // The page says what a backup holds before it asks for anything.
  await openBackupPage(page);
  await expect(page.getByText(/Not your account, your keys or your safety code/)).toBeVisible();

  // Too short, then a mismatch: nothing is made. Then a good password gives exactly one file.
  await page.getByLabel("Choose a password").fill("short");
  await page.getByLabel("Type it again").fill("short");
  await page.getByRole("button", { name: "Save backup file" }).click();
  await expect(page.getByRole("alert")).toContainText("at least 12 characters");
  await page.getByLabel("Choose a password").fill(PASSWORD);
  await page.getByLabel("Type it again").fill(PASSWORD + "!");
  await page.getByRole("button", { name: "Save backup file" }).click();
  await expect(page.getByRole("alert")).toContainText("do not match");
  await page.getByLabel("Type it again").fill(PASSWORD);
  const [download] = await Promise.all([page.waitForEvent("download", SLOW), page.getByRole("button", { name: "Save backup file" }).click()]);
  expect(download.suggestedFilename()).toMatch(/^om-backup-\d{4}-\d{2}-\d{2}\.ombackup$/);
  await expect(page.getByText(/^Saved om-backup-/)).toBeVisible();
  const dir = mkdtempSync(join(tmpdir(), "om-backup-e2e-"));
  const file = join(dir, "chats.ombackup");
  await download.saveAs(file);
  const bytes = readFileSync(file);
  expect(bytes.subarray(0, 8).toString("latin1")).toBe("OMBACKUP");
  expect(bytes.toString("latin1")).not.toContain("hello from alice"); // nothing readable in the file

  // A brand-new browser: no data, its own identity.
  const fresh = await browser.newContext({ baseURL: CORE });
  const page2 = await fresh.newPage();
  await noPasskeys(page2);
  await onboard(page2, { name: "Alice again" });
  await expect(page2.getByRole("option")).toHaveCount(0);
  await openBackupPage(page2);

  // A file that is not a backup is said so at once; a wrong password, a changed file and a cut-short file are each named.
  const junk = join(dir, "photo.ombackup");
  writeFileSync(junk, "just some text, not a backup");
  await page2.getByLabel("Choose the backup file").setInputFiles(junk);
  await expect(page2.getByRole("alert")).toContainText("does not look like an OM backup file");
  await restoreWith(page2, file, "not the right password!");
  await expect(page2.getByRole("alert")).toContainText("does not match this backup", SLOW);
  const changed = Buffer.from(bytes);
  changed[changed.length - 1] = (changed[changed.length - 1] ?? 0) ^ 1;
  const changedFile = join(dir, "changed.ombackup");
  writeFileSync(changedFile, changed);
  await restoreWith(page2, changedFile, PASSWORD);
  await expect(page2.getByRole("alert")).toContainText("changed or damaged", SLOW);
  const cutFile = join(dir, "cut.ombackup");
  writeFileSync(cutFile, bytes.subarray(0, bytes.length - 10));
  await restoreWith(page2, cutFile, PASSWORD);
  await expect(page2.getByRole("alert")).toContainText("incomplete");
  await page2.getByRole("button", { name: /^.*, Settings/ }).click(); // back to Chats: nothing was imported by any of the above
  await expect(page2.getByRole("option")).toHaveCount(0);

  // The right file and password: the chat appears as a saved copy.
  await openBackupPage(page2);
  await restoreWith(page2, file, PASSWORD);
  await expect(page2.getByText("Restored 1 chat with 2 messages.")).toBeVisible(SLOW);
  await page2.getByRole("button", { name: /Settings/ }).click();
  await expect(page2.getByRole("option")).toHaveCount(1);
  await page2.getByRole("option").click();
  await expect(page2.getByRole("log").getByText("hello from alice")).toBeVisible();
  await expect(page2.getByRole("log").getByText("hi alice, this is bob")).toBeVisible();
  await expect(page2.getByText(/Restored from a backup file/).first()).toBeVisible();
  await expect(page2.getByRole("textbox", { name: /message/i })).toHaveCount(0); // history only: nothing to reply with

  // Restoring the same file again adds nothing; the history is still there after a reload.
  await openBackupPage(page2);
  await restoreWith(page2, file, PASSWORD);
  await expect(page2.getByText(/nothing new for this device/)).toBeVisible(SLOW);
  await page2.getByRole("button", { name: /Settings/ }).click(); // back to Chats, so the reload opens the list
  await page2.reload();
  await reopen(page2, 30_000);
  await expect(page2.getByRole("option")).toHaveCount(1);
  await page2.getByRole("option").click();
  await expect(page2.getByRole("log").getByText("hello from alice")).toBeVisible();
  await fresh.close();
});
