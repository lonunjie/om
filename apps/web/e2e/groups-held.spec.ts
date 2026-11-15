// An invitation that arrives before this device has the group files (D70, D71), on the dev harness (the harness plays the other people and
// the module): the person sees a calm entry, nothing is accepted, tapping it gets the groups ready, and the ordinary invitation card then
// offers Join. Also the plain notice when some invitations could not be kept.
import { expect, test, type Page } from "@playwright/test";
import { noPasskeys, onboard } from "./helpers.ts";
import { shot } from "./invite-flow.ts";

interface DevGroups {
  readonly sim: {
    setModuleLoaded: (loaded: boolean) => void;
    simulateInvite: (ctx: unknown, relationshipId: string, name: string) => string;
    simulateHeldOverflow: (ctx: unknown, invites: number, replies?: number) => void;
    list: (ctx: unknown) => readonly { conversationId: string }[];
  };
  readonly ctx: () => unknown;
}

async function openWork(page: Page): Promise<void> {
  await noPasskeys(page);
  await onboard(page, { stopAt: "invite" });
  await page.getByRole("button", { name: "Skip for now" }).click();
  await page.getByRole("list", { name: "Identities" }).getByRole("button", { name: "Work" }).click();
}

test("a held group invitation: calm entry, tap gets groups ready, then Join is asked; nothing is accepted before", async ({ page }) => {
  await openWork(page);
  await page.evaluate(() => {
    const dev = (globalThis as unknown as { __omDevGroups: DevGroups }).__omDevGroups;
    dev.sim.setModuleLoaded(false); // this device has not loaded the groups module
    dev.sim.simulateInvite(dev.ctx(), "c-team", "Chess club"); // arrives over the 1:1 with Design team: held, not shown as a group invitation
  });
  const held = page.getByRole("region", { name: "Group invitations waiting" });
  await expect(held.getByText("You have a group invitation")).toBeVisible();
  await expect(held.getByText("Design team invited you to Chess club")).toBeVisible();
  await expect(held.getByText(/nobody there has been told anything/)).toBeVisible();
  await expect(page.getByRole("button", { name: "Join" })).toHaveCount(0); // no accept here
  await shot(page, "50-held-group-invitation");

  await held.getByRole("button", { name: "Open invitation" }).click();
  // The loader sheet runs, the groups become ready, and the entry gives way to the ordinary invitation card.
  const invites = page.getByRole("region", { name: "Group invitations", exact: true });
  const chess = invites.locator(".om-group-invite").filter({ hasText: "Chess club" });
  await expect(chess.getByText("Design team invited you to Chess club")).toBeVisible();
  await expect(page.getByRole("dialog")).toHaveCount(0);
  await expect(held).toHaveCount(0);
  expect(await page.evaluate(() => (globalThis as unknown as { __omDevGroups: DevGroups }).__omDevGroups.sim.list((globalThis as unknown as { __omDevGroups: DevGroups }).__omDevGroups.ctx()).length)).toBe(0); // loading joined nothing
  await shot(page, "51-held-became-invitation");

  await chess.getByRole("button", { name: "Join" }).click();
  await expect(page.getByRole("option", { name: /Chess club/ })).toBeVisible();
});

test("held group invitations that could not be kept: a plain notice that stays until it is read", async ({ page }) => {
  await page.setViewportSize({ width: 390, height: 844 });
  await openWork(page);
  await page.evaluate(() => {
    const dev = (globalThis as unknown as { __omDevGroups: DevGroups }).__omDevGroups;
    dev.sim.setModuleLoaded(false);
    dev.sim.simulateHeldOverflow(dev.ctx(), 3);
  });
  const held = page.getByRole("region", { name: "Group invitations waiting" });
  await expect(held.getByText("Some group invitations couldn't be kept")).toBeVisible();
  await expect(held.getByText("Ask them to invite you again.")).toBeVisible();
  await shot(page, "52-held-overflow");
  await held.getByRole("button", { name: "Got it" }).click();
  await expect(held).toHaveCount(0);
});
