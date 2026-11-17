// The same-name and lookalike cues on one-to-one surfaces (ADR-0197, relay status D139) on the dev harness: a message request whose name looks like a
// contact's says so calmly before accept (`?requestcue`), and two contacts with the same name are told apart in the list and the header (`?dupnames`). No dialog stops anyone.
import { expect, test, type Page } from "@playwright/test";
import { noPasskeys, walkSetup } from "./helpers.ts";

async function open(page: Page, query: string): Promise<void> {
  await noPasskeys(page);
  await page.goto(`/?${query}`);
  await page.getByRole("button", { name: "Get started" }).click();
  await walkSetup(page, { stopAt: "invite" });
  await page.getByRole("button", { name: "Skip for now" }).click();
  await page.getByRole("list", { name: "Identities" }).getByRole("button", { name: "Work" }).click();
}

test("requests: a name that looks like a contact's gets a calm line, a pattern, and accept stays one tap", async ({ page }) => {
  await open(page, "requestcue");
  const requests = page.getByRole("region", { name: "Message request" });
  const similar = requests.filter({ hasText: "Maria G" }).filter({ hasText: "look similar to" });
  await expect(similar).toHaveCount(1);
  await expect(similar.getByText("2 people here have names that look similar to Maria Gomez. A name does not show who someone is.")).toBeVisible();
  await expect(similar.getByText(/Maria G.mez \(\d of 2\)/)).toBeVisible();
  await expect(similar.locator("svg.om-avatar--pattern")).toHaveAttribute("aria-hidden", "true");
  await expect(similar.getByRole("button", { name: "Accept", exact: true })).toBeEnabled();
  // The other request (an unrelated name) has no cue.
  await expect(requests.filter({ hasText: "Sam Okafor" }).locator("svg.om-avatar--pattern")).toHaveCount(0);
  await expect(page.getByRole("dialog")).toHaveCount(0);
});

test("chats: two contacts with the same name get a pattern, 'n of 2', one line, and the header says it too", async ({ page }) => {
  await open(page, "dupnames");
  await expect(page.locator(".om-conv-list__samename").getByText("2 of your contacts are named Ana Reyes.")).toBeVisible();
  const rows = page.getByRole("option", { name: /Ana Reyes \(\d of 2\)/i });
  await expect(rows).toHaveCount(2);
  await rows.first().click();
  await expect(page.getByRole("heading", { name: /Ana Reyes \(\d of 2\)/i })).toBeVisible();
  await expect(page.locator(".om-conv-view__samename")).toBeVisible();
  // Omnibar: both contacts carry their place (a message hit from either may also show it, so at least two).
  await page.keyboard.press("Control+k");
  await page.getByRole("combobox").fill("ana");
  await expect.poll(async () => page.locator(".om-omni").getByRole("option", { name: /Ana Reyes \(\d of 2\)/i }).count()).toBeGreaterThanOrEqual(2);
});
