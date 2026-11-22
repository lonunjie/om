// A contact's side of "I lost my devices" (ADR-0190, D131 to D133) on the REAL Core and the local reference Home, in three browser contexts:
// Alice and Bob are contacts, Bob has saved his safety code, and a new device of his asks for the account back. Alice's page then shows the one
// calm banner, keeps a text she writes (never sent to the new device on her own), and "This isn't them" hands it back as not sent. Nothing is
// passed between the contexts except what a person would carry: the invite link and the safety code. The dev-harness spec (peer-recovery.spec.ts)
// covers the same screens with a stand-in Core; this one proves the real Core holds the text.
import { expect, test, type BrowserContext, type Page } from "@playwright/test";
import { noPasskeys, walkSetup } from "./helpers.ts";
import { connectToHome, saveSafetyCode } from "./recovery-steps.ts";

const APP = "http://localhost:4179";
test.use({ baseURL: APP });

/** A first-time person on the setup bundle, connected to the locked Home by name, on the invite screen. */
async function start(context: BrowserContext, name: string): Promise<Page> {
  const page = await context.newPage();
  await noPasskeys(page);
  await connectToHome(page);
  await walkSetup(page, { stopAt: "invite", name });
  return page;
}

test("a text written while a contact is recovering is held on the real Core, never sent on its own, and handed back when declined", async ({ browser }) => {
  test.setTimeout(360_000);
  const contexts: BrowserContext[] = [];
  const fresh = async (): Promise<BrowserContext> => { const c = await browser.newContext({ baseURL: APP, viewport: { width: 1280, height: 800 } }); contexts.push(c); return c; };
  try {
    const alice = await start(await fresh(), "Alice");
    await alice.getByRole("button", { name: "Show my QR" }).click();
    const link = ((await alice.locator("code[translate=no]").first().textContent()) ?? "").trim();
    expect(link).toMatch(/\/#invite\//);
    await alice.getByRole("button", { name: "Skip for now" }).click();

    const bob = await start(await fresh(), "Bob");
    await bob.getByText("Have an invite?").click();
    await bob.getByLabel("Invitation link").fill(new URL(link).href);
    await bob.getByRole("button", { name: "Accept invitation" }).click();
    await expect(bob.getByRole("button", { name: "Settings" })).toBeVisible({ timeout: 60_000 });
    const request = alice.getByRole("region", { name: "Message request" }).filter({ hasText: "Bob" });
    await expect(request).toBeVisible({ timeout: 60_000 });
    await request.getByRole("button", { name: "Accept", exact: true }).click();
    await expect(alice.getByRole("option", { name: /Bob/ })).toBeVisible({ timeout: 60_000 });

    // Bob saves the safety code after Alice is a contact, so the saved account knows her route.
    const groups = await saveSafetyCode(bob);
    await bob.waitForTimeout(10_000); // the Home has the record before a new device looks

    // A new device of Bob's, knowing nothing, asks for the account back.
    const fresh2 = await (await fresh()).newPage();
    await noPasskeys(fresh2);
    await connectToHome(fresh2);
    await fresh2.getByRole("button", { name: "I lost my devices" }).click();
    await fresh2.getByRole("button", { name: "Next: Secure this device" }).click();
    await expect(fresh2.getByRole("heading", { name: "Get your account back" })).toBeVisible({ timeout: 60_000 });
    await fresh2.getByLabel("Your recovery key").fill(groups.join(" "));
    await fresh2.getByRole("button", { name: "Find my account" }).click();
    await expect(fresh2.getByRole("heading", { name: "Your saved account was found" })).toBeVisible({ timeout: 60_000 });
    await fresh2.getByRole("button", { name: "Ask for my account back" }).click();
    await expect(fresh2.getByRole("heading", { name: "Waiting to get your account back" })).toBeVisible({ timeout: 60_000 });

    // Alice sees one calm banner in her chat with Bob.
    await alice.getByRole("option", { name: /Bob/ }).click();
    const banner = alice.getByRole("region", { name: "Bob is recovering their account on a new device" });
    await expect(banner).toBeVisible({ timeout: 120_000 });
    await expect(banner).toContainText("Messages you write will be delivered once the new device is confirmed.");

    // A text is kept here and shown as waiting; it reaches nobody, not even Bob's first device.
    const text = "are you there?";
    await alice.getByRole("textbox", { name: "Message" }).fill(text);
    await alice.getByRole("button", { name: "Send" }).click();
    const log = alice.getByRole("log");
    await expect(log.getByText(text, { exact: true })).toBeVisible();
    await expect(log.getByText("Waiting until their new device is confirmed")).toBeVisible();
    await bob.getByRole("option").first().click({ timeout: 5_000 }).catch(() => undefined); // her chat if it is listed; a text that arrived would show in the list too
    await bob.waitForTimeout(8_000);
    await expect(bob.getByText(text, { exact: true })).toHaveCount(0);

    // Declining ends it for this contact: what waited is handed back as not sent, with one tap to try again, and still never reached Bob.
    await banner.getByRole("button", { name: "This isn't them" }).click();
    await expect(log.getByText("Not sent.")).toBeVisible({ timeout: 60_000 });
    await expect(alice.getByRole("button", { name: "Try sending again" })).toBeVisible();
    await expect(alice.getByRole("region", { name: /Bob is recovering/ })).toHaveCount(0);
    await bob.waitForTimeout(4_000);
    await expect(bob.getByText(text, { exact: true })).toHaveCount(0);
  } finally {
    for (const c of contexts) await c.close().catch(() => undefined);
  }
});
