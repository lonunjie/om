// Two contacts who show the same name (GROUPS_AND_SPACES_UX section 10) on the dev harness (`?dupnames` adds the second Ana):
// the invitation, the new-group pick list and the people sheet each say so in one calm line, with a pattern and "1 of 2", and no dialog stops the person.
import { expect, test } from "@playwright/test";
import { noPasskeys, walkSetup } from "./helpers.ts";

test("groups: people who share a name are told apart without a modal", async ({ page }) => {
  await noPasskeys(page);
  await page.goto("/?dupnames");
  await page.getByRole("button", { name: "Get started" }).click();
  await walkSetup(page, { stopAt: "invite" });
  await page.getByRole("button", { name: "Skip for now" }).click();
  await page.getByRole("list", { name: "Identities" }).getByRole("button", { name: "Work" }).click();

  const invites = page.getByRole("region", { name: "Group invitations" });
  await expect(invites.getByText("2 of your contacts are named Ana Reyes.")).toBeVisible();
  await expect(invites.getByText(/Ana Reyes \(\d of 2\) invited you to Book club/)).toBeVisible();
  await expect(invites.locator("svg.om-avatar--pattern")).toHaveCount(1);
  await expect(invites.locator("svg.om-avatar--pattern")).toHaveAttribute("aria-hidden", "true");

  await page.getByRole("button", { name: "New group" }).click();
  const make = page.getByRole("dialog", { name: "New group" });
  await expect(make.getByText("2 of your contacts are named Ana Reyes.")).toBeVisible();
  await expect(make.getByRole("checkbox", { name: /Ana Reyes \(\d of 2\)/ })).toBeVisible();
  await expect(make.getByRole("checkbox", { name: /ana reyes \(\d of 2\)/ })).toBeVisible();
  await expect(make.locator("svg.om-avatar--pattern")).toHaveCount(2);
  await page.keyboard.press("Escape");

  await invites.getByRole("button", { name: "Join" }).click();
  await page.getByRole("option", { name: /Book club/ }).click();
  await page.getByRole("button", { name: /^People in Book club/ }).click();
  const sheet = page.getByRole("dialog");
  await expect(sheet.getByRole("button", { name: /^Invite .* \(\d of 2\)$/ })).toBeVisible();
});
