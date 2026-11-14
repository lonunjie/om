// Ordinary groups on the dev harness (the shipped Core has no group engine or wire yet, so the harness plays the other people):
// the Work profile has an invitation waiting; accept it, open the group, send, see a reply, open the people list, add someone,
// leave. Also create a new group and see that the people chosen are only invited.
import { expect, test } from "@playwright/test";
import { noPasskeys, onboard } from "./helpers.ts";
import { shot } from "./invite-flow.ts";

interface DevGroups {
  readonly sim: { list: (ctx: unknown) => readonly { conversationId: string }[]; simulateMessage: (ctx: unknown, id: string, from: string, text: string) => boolean };
  readonly ctx: () => unknown;
}

test("groups: accept an invitation, talk, add and leave; create a group and its people stay invited", async ({ page }) => {
  await noPasskeys(page);
  await onboard(page, { stopAt: "invite" });
  await page.getByRole("button", { name: "Skip for now" }).click();
  await page.getByRole("list", { name: "Identities" }).getByRole("button", { name: "Work" }).click();

  // The invitation says who asked, and what joining means; nothing happens until the person answers.
  const invites = page.getByRole("region", { name: "Group invitations" });
  await expect(invites.getByText("Ana Reyes invited you to Book club")).toBeVisible();
  await expect(invites.getByText(/only what is said after you join/)).toBeVisible();
  await shot(page, "30-group-invitation");
  await invites.getByRole("button", { name: "Join" }).click();
  await expect(page.getByRole("option", { name: /Book club/ })).toBeVisible();
  await page.getByRole("option", { name: /Book club/ }).click();
  expect(new URL(page.url()).hash).toMatch(/^#/);

  await page.getByRole("textbox", { name: "Message the group" }).fill("hello everyone");
  await page.getByRole("button", { name: "Send" }).click();
  await expect(page.getByText("hello everyone", { exact: true })).toBeVisible();
  await page.evaluate(() => {
    const dev = (globalThis as unknown as { __omDevGroups: DevGroups }).__omDevGroups;
    const id = dev.sim.list(dev.ctx())[0]?.conversationId ?? "";
    dev.sim.simulateMessage(dev.ctx(), id, "acct-c-ana", "welcome Maya");
  });
  await expect(page.getByText("welcome Maya", { exact: true })).toBeVisible();
  await shot(page, "31-group-chat");

  await page.getByRole("button", { name: /^People in Book club/ }).click();
  const sheet = page.getByRole("dialog");
  await expect(sheet.getByText("Anyone in a group can show its messages to others as proof of who wrote them.")).toBeVisible();
  await shot(page, "32-group-people");
  await sheet.getByRole("button", { name: /^Invite / }).first().click();
  await expect(sheet.getByText("Invited, no answer yet")).toBeVisible();
  await sheet.getByRole("button", { name: "Leave group" }).click();
  await expect(sheet.getByText("Leave Book club?")).toBeVisible();
  await sheet.getByRole("button", { name: "Stay" }).click();
  await sheet.getByRole("button", { name: "Leave group" }).click();
  await sheet.getByRole("button", { name: "Leave", exact: true }).click();
  await expect(page.getByRole("option", { name: /Book club/ })).toHaveCount(0);

  // Create: only people already chatted with, and the group says they are invited, not added.
  await page.getByRole("button", { name: "New group" }).click();
  const make = page.getByRole("dialog", { name: "New group" });
  await expect(make.getByText("Each person gets an invitation and is only added if they say yes.")).toBeVisible();
  await make.getByLabel("Group name (optional)").fill("Lunch");
  await make.getByRole("checkbox", { name: /Design team/ }).check();
  await shot(page, "33-new-group");
  await make.getByRole("button", { name: /Create group/ }).click();
  await expect(page.getByRole("option", { name: /Lunch/ })).toBeVisible();
});
