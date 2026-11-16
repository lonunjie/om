// A contact recovering their account (ADR-0190) on the dev harness, at phone and desktop widths: the one calm banner, "This isn't them",
// the Hardened-or-Maximum "Yes, it's them" with its consent words, and the honest send refusal. The harness plays Core's side of it.
import { createRequire } from "node:module";
import { expect, test, type Page } from "@playwright/test";
import { noPasskeys, onboard } from "./helpers.ts";
import { shot } from "./invite-flow.ts";

const axeSource = createRequire(import.meta.url).resolve("axe-core/axe.min.js");
test.use({ bypassCSP: true }); // only so the test can inject axe; the app's own policy is covered elsewhere

const SIZES = [{ name: "phone", width: 390, height: 780 }, { name: "desktop", width: 1280, height: 800 }] as const;

async function openChat(page: Page, name: RegExp): Promise<void> {
  await noPasskeys(page);
  await onboard(page, { stopAt: "invite" });
  await page.getByRole("button", { name: "Skip for now" }).click();
  await page.getByRole("list", { name: "Identities" }).getByRole("button", { name: "Work" }).click();
  await page.getByRole("option", { name }).click();
}

/** axe over the conversation, and colour contrast (which only a real browser computes) over the banner area. The message time stamps' own contrast is outside this change. */
async function noViolations(page: Page): Promise<void> {
  await page.addScriptTag({ path: axeSource });
  const violations = await page.evaluate(async () => {
    const axe = (globalThis as unknown as { axe: { run: (el: Element, o: unknown) => Promise<{ violations: { id: string; help: string; nodes: { target: unknown; any: { data: unknown }[] }[] }[] }> } }).axe;
    const el = document.querySelector(".om-conv-view");
    const notes = document.querySelector(".om-conv-view__notes");
    if (el === null) return ["missing"];
    const whole = await axe.run(el, { rules: { region: { enabled: false }, "color-contrast": { enabled: false } } });
    const r = notes === null ? whole : { violations: [...whole.violations, ...(await axe.run(notes, { rules: { region: { enabled: false } } })).violations] };
    return r.violations.map((v) => `${v.id}: ${v.help} ${JSON.stringify(v.nodes.map((n) => [n.target, n.any[0]?.data]))}`);
  });
  expect(violations).toEqual([]);
}

for (const size of SIZES) {
  test.describe(size.name, () => {
    test.use({ viewport: { width: size.width, height: size.height } });

    test("waiting: one calm banner, and sending is refused with the person's name", async ({ page }) => {
      await openChat(page, /Maria Gomez/);
      const banner = page.getByRole("region", { name: "Maria Gomez is recovering their account on a new device" });
      await expect(banner).toBeVisible();
      await expect(banner).toContainText("OM is waiting about 3 days to be sure it is really them");
      await expect(banner).not.toContainText(/\b(certain|guarantee|verified|secure)\b/i);
      await expect(banner.getByRole("button", { name: "Yes, it's them" })).toHaveCount(0);
      await banner.getByRole("button", { name: "Check with Maria Gomez" }).click();
      await expect(banner.getByText(/Their other devices can stop this/)).toBeVisible();
      await noViolations(page);
      await shot(page, `40-peer-recovery-waiting-${size.name}`);

      const box = page.getByRole("textbox", { name: "Message" });
      await box.fill("are you there?");
      await page.getByRole("button", { name: "Send" }).click();
      await expect(page.getByText("Not sent. Maria Gomez is recovering their account; you can message them once it finishes.")).toBeVisible();
      await expect(box).toHaveValue("are you there?");
      await expect(page.getByRole("log").getByText("are you there?", { exact: true })).toHaveCount(0); // not in the conversation: nothing was sent or queued
    });

    test("decline: 'This isn't them' ends it for this contact, leaves one quiet line, and sending works again", async ({ page }) => {
      await openChat(page, /Maria Gomez/);
      await page.getByRole("region", { name: /Maria Gomez is recovering/ }).getByRole("button", { name: "This isn't them" }).click();
      await expect(page.getByText("You turned this down. Your chat with Maria Gomez is unchanged. They can ask again.")).toBeVisible();
      await expect(page.getByRole("region", { name: /Maria Gomez is recovering/ })).toHaveCount(0);
      await noViolations(page);
      await shot(page, `41-peer-recovery-declined-${size.name}`);
      await page.getByRole("textbox", { name: "Message" }).fill("hello again");
      await page.getByRole("button", { name: "Send" }).click();
      await expect(page.getByText("hello again", { exact: true })).toBeVisible();
    });

    test("confirm: after the wait the person says yes with the consent words in view, and one line says what happened", async ({ page }) => {
      await openChat(page, /Tomas Reid/);
      const ask = page.getByRole("region", { name: "The wait is over. Is this really Tomas Reid?" });
      await expect(ask).toBeVisible();
      await expect(ask).toContainText("Say yes only if you checked with Tomas Reid another way.");
      await expect(ask).toContainText("their safety code will change");
      await expect(ask.getByRole("button", { name: "This isn't them" })).toBeVisible();
      await noViolations(page);
      await shot(page, `42-peer-recovery-ready-${size.name}`);
      await ask.getByRole("button", { name: "Yes, it's them" }).click();
      await expect(page.getByText("Tomas Reid recovered their account on a new device. Your chat and earlier messages are unchanged. Their safety code changed.")).toBeVisible();
      await expect(ask).toHaveCount(0);
      await noViolations(page);
      await page.getByRole("textbox", { name: "Message" }).fill("welcome back");
      await page.getByRole("button", { name: "Send" }).click();
      await expect(page.getByText("welcome back", { exact: true })).toBeVisible();
    });
  });
}
