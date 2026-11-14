// Groups on the real Core WITH the OpenMLS engine module, in a real browser: two identities in one browser (the profile switcher),
// each its own vault, carried by the group driver over an in-page loopback carriage (the real carriage is not specified yet,
// groups plan Q2; the build refuses to be a production build: VITE_OM_GROUPS needs the dev bundle switch, no product carriage yet, ADR-0187 is accepted).
// Alice invites Bob to a group over their 1:1; Bob joins; a message goes each way; the transport only ever sees what Core made.
import { expect, test, type Page } from "@playwright/test";
import { noPasskeys, onboard, walkSetup, addIdentity } from "./helpers.ts";

test.use({ baseURL: "http://localhost:4180" });

type Who = "alice" | "bob";
async function asIdentity(page: Page, state: { who: Who }, who: Who): Promise<void> {
  if (state.who === who) return;
  await page.getByRole("button", { name: "Settings" }).waitFor();
  const other = page.getByRole("list", { name: "Identities" }).getByRole("button").and(page.locator(":not([aria-current])"));
  await other.click();
  await expect(page.getByRole("list", { name: "Identities" }).getByRole("button").and(page.locator("[aria-current]"))).toHaveCount(1);
  state.who = who;
}

test("groups on the real Core: invite over the 1:1, join, a message each way between two identities", async ({ page }) => {
  await noPasskeys(page);
  await onboard(page, { stopAt: "invite" });
  await page.getByRole("button", { name: "Show my QR" }).click();
  const link = (await page.locator("code[translate=no]").first().textContent())?.trim() ?? "";
  await page.getByRole("button", { name: "Skip for now" }).click();
  await addIdentity(page);
  await walkSetup(page, { stopAt: "invite" });
  await page.getByText("Have an invite?").click();
  await page.getByLabel("Invitation link").fill(link);
  await page.getByRole("button", { name: "Accept invitation" }).click();
  await expect(page.getByRole("button", { name: "Settings" })).toBeVisible();
  const me = { who: "alice" as Who };
  await page.getByRole("region", { name: "Message request" }).getByRole("button", { name: "Accept", exact: true }).click();
  await expect(page.getByRole("option")).toHaveCount(1, { timeout: 30_000 });

  // Alice makes a group with Bob. Bob is shown as invited, not added: he has not said yes.
  await page.getByRole("button", { name: "New group" }).click();
  const make = page.getByRole("dialog", { name: "New group" });
  await make.getByLabel("Group name (optional)").fill("Lunch club");
  await make.getByRole("checkbox").first().check();
  await make.getByRole("button", { name: /Create group/ }).click();
  await expect(page.getByRole("option", { name: /Lunch club/ })).toBeVisible();

  // Bob gets the invitation through the carriage and joins.
  await asIdentity(page, me, "bob");
  const invites = page.getByRole("region", { name: "Group invitations" });
  await expect(invites.getByText(/invited you to Lunch club/)).toBeVisible({ timeout: 30_000 });
  await invites.getByRole("button", { name: "Join" }).click();
  await expect(page.getByRole("option", { name: /Lunch club/ })).toBeVisible();
  await page.getByRole("option", { name: /Lunch club/ }).click();

  // Alice's side commits the Add and sends the Welcome; once Bob can send, he says hello.
  await expect(page.getByRole("textbox", { name: "Message the group" })).toBeEnabled({ timeout: 30_000 });
  await page.getByRole("textbox", { name: "Message the group" }).fill("hello from bob");
  await page.getByRole("button", { name: "Send" }).click();
  await expect(page.getByRole("log", { name: "Group messages" }).getByText("hello from bob", { exact: true })).toBeVisible();

  await asIdentity(page, me, "alice");
  await page.getByRole("option", { name: /Lunch club/ }).click();
  await expect(page.getByRole("log", { name: "Group messages" }).getByText("hello from bob", { exact: true })).toBeVisible({ timeout: 30_000 });
  await page.getByRole("textbox", { name: "Message the group" }).fill("hi bob, alice here");
  await page.getByRole("button", { name: "Send" }).click();

  await asIdentity(page, me, "bob");
  await page.getByRole("option", { name: /Lunch club/ }).click();
  await expect(page.getByRole("log", { name: "Group messages" }).getByText("hi bob, alice here", { exact: true })).toBeVisible({ timeout: 30_000 });
});
