// Group protection levels on the dev harness (the harness plays the other people): create a group with Hardened, then tighten and loosen
// its settings, see the notices and the honest states, at phone and desktop width, with axe checks. The shipped Core's own behaviour is
// covered by the Rust tests; this is the screens.
import { expect, test, type Page } from "@playwright/test";
import { injectAxe, noPasskeys, onboard } from "./helpers.ts";
import { shot } from "./invite-flow.ts";

test.use({ bypassCSP: true }); // only so the test can inject axe; the app's own policy is covered elsewhere

interface DevGroups {
  readonly sim: {
    list: (ctx: unknown) => readonly { conversationId: string }[];
    get: (ctx: unknown, id: string) => { protection: { profile: number; membersMayInvite: boolean } } | null;
    simulateEvent: (ctx: unknown, e: unknown) => void;
    simulateSettingChange: (ctx: unknown, id: string, from: string, setting: unknown) => boolean;
    simulateQuorum: (ctx: unknown, id: string) => boolean;
  };
  readonly ctx: () => unknown;
}

async function axeClean(page: Page, selector: string): Promise<void> {
  await injectAxe(page);
  const violations = await page.evaluate(async (sel) => {
    const axe = (globalThis as unknown as { axe: { run: (el: Element, o: unknown) => Promise<{ violations: { id: string; help: string }[] }> } }).axe;
    const el = document.querySelector(sel);
    if (el === null) return ["missing"];
    const r = await axe.run(el, { rules: { "color-contrast": { enabled: false }, region: { enabled: false } } });
    return r.violations.map((v) => `${v.id}: ${v.help}`);
  }, selector);
  expect(violations).toEqual([]);
}

const state = (page: Page, id: string, fn: "profile" | "mayInvite"): Promise<number | boolean | null> =>
  page.evaluate(([gid, what]) => {
    const dev = (globalThis as unknown as { __omDevGroups: DevGroups }).__omDevGroups;
    const p = dev.sim.get(dev.ctx(), gid)?.protection;
    return p === undefined ? null : what === "profile" ? p.profile : p.membersMayInvite;
  }, [id, fn] as const);

for (const [label, size] of [["phone", { width: 390, height: 844 }], ["desktop", { width: 1280, height: 800 }]] as const) {
  test(`group protection at ${label} width: create with Hardened, tighten, loosen with confirmation, states`, async ({ page }) => {
    await page.setViewportSize(size);
    await noPasskeys(page);
    await onboard(page, { stopAt: "invite" });
    await page.getByRole("button", { name: "Skip for now" }).click();
    await page.getByRole("list", { name: "Identities" }).getByRole("button", { name: "Work" }).click();

    // Create with Hardened: Recommended is pre-selected, the other levels say what they mean in one line each.
    await page.getByRole("button", { name: "New group" }).click();
    const make = page.getByRole("dialog", { name: "New group" });
    await make.getByLabel("Group name (optional)").fill("Planning");
    await make.getByRole("checkbox", { name: /Design team/ }).check();
    await make.getByText("Protection: Recommended").click();
    await expect(make.getByRole("radio", { name: /Recommended/ })).toBeChecked();
    await expect(make.getByText(/Only admins can add people, and an admin approves each new person/)).toBeVisible();
    await make.getByRole("radio", { name: /^Hardened/ }).check();
    await expect(make.getByText("Protection: Hardened")).toBeVisible();
    await shot(page, `40-new-group-levels-${label}`);
    await axeClean(page, "[role=dialog]");
    await make.getByRole("button", { name: /Create group/ }).click();
    const row = page.getByRole("option", { name: /Planning/ });
    await expect(row).toBeVisible();
    const gid = await page.evaluate(() => { const d = (globalThis as unknown as { __omDevGroups: DevGroups }).__omDevGroups; return d.sim.list(d.ctx()).slice(-1)[0]?.conversationId ?? ""; });
    expect(await state(page, gid, "profile")).toBe(2);
    await row.click();

    // Room settings, from the people sheet. Tighten applies at once.
    await page.getByRole("button", { name: /^People in Planning/ }).click();
    await page.getByRole("dialog").getByRole("button", { name: "Room settings" }).click();
    const sheet = page.getByRole("dialog", { name: "Settings for Planning" });
    await expect(sheet.getByText("Only admins can add people.")).toBeVisible();
    await sheet.getByRole("button", { name: "Use Maximum Privacy" }).click();
    await expect.poll(() => state(page, gid, "profile")).toBe(3);
    await shot(page, `41-room-settings-${label}`);
    await axeClean(page, "[role=dialog]");

    // Loosen: asks first, names what changes, then applies and is announced.
    await sheet.getByRole("button", { name: "Use Recommended" }).click();
    const ask = sheet.getByRole("group", { name: "Make this group less strict?" });
    await expect(ask.getByText("Anyone in the group will be able to add people.")).toBeVisible();
    await expect(ask.getByText("New people will join without an admin's OK.")).toBeVisible();
    await expect(ask.getByText("Everyone in the group will see this change and who made it.")).toBeVisible();
    expect(await state(page, gid, "profile")).toBe(3);
    await shot(page, `42-loosen-confirm-${label}`);
    await axeClean(page, "[role=dialog]");
    await ask.getByRole("button", { name: "Keep it as it is" }).click();
    expect(await state(page, gid, "profile")).toBe(3);
    await sheet.getByRole("button", { name: "Use Recommended" }).click();
    await sheet.getByRole("group", { name: "Make this group less strict?" }).getByRole("button", { name: "Make less strict" }).click();
    await expect.poll(() => state(page, gid, "profile")).toBe(1);
    await sheet.getByRole("button", { name: "Close" }).click();
    await expect(page.getByText(/changed the protection level: now Recommended\./).first()).toBeVisible();
    await expect(page.getByText("Less strict").first()).toBeVisible();
    await shot(page, `43-notices-${label}`);

    // Someone else tightens: a named system line. A group that needs more agreement refuses an unconfirmed loosening honestly.
    await page.evaluate((id) => {
      const d = (globalThis as unknown as { __omDevGroups: DevGroups }).__omDevGroups;
      d.sim.simulateSettingChange(d.ctx(), id, "acct-c-team", { members_may_invite: false });
      d.sim.simulateQuorum(d.ctx(), id);
    }, gid);
    await expect(page.getByText(/changed who can add people: now only admins\./).first()).toBeVisible();
    await page.getByRole("button", { name: /^People in Planning/ }).click();
    await page.getByRole("dialog").getByRole("button", { name: "Room settings" }).click();
    await page.getByRole("dialog", { name: "Settings for Planning" }).getByRole("button", { name: "Let anyone in the group add people" }).click();
    await page.getByRole("group", { name: "Make this group less strict?" }).getByRole("button", { name: "Make less strict" }).click();
    await expect(page.getByRole("alert").filter({ hasText: /more than one admin to agree/ })).toBeVisible();
    await shot(page, `44-quorum-${label}`);
    await page.getByRole("dialog").getByRole("button", { name: "Close" }).click();

    // Honest states: held, undeliverable. (A superseded Commit has no screen.)
    await page.evaluate(() => {
      const d = (globalThis as unknown as { __omDevGroups: DevGroups }).__omDevGroups;
      d.sim.simulateEvent(d.ctx(), { kind: "commit_superseded", intentId: "x" });
      d.sim.simulateEvent(d.ctx(), { kind: "held", reason: "account_head_missing" });
      d.sim.simulateEvent(d.ctx(), { kind: "undeliverable", count: 1 });
    });
    const status = page.getByRole("region", { name: "Group status" }).locator("visible=true").first();
    await expect(status.getByText("Waiting for a group update")).toBeVisible();
    await expect(status.getByText("1 group message could not be delivered")).toBeVisible();
    await shot(page, `45-states-${label}`);
    await axeClean(page, "[aria-label='Group status']");
  });
}
