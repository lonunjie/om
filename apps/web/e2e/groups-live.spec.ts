// Groups on the real Core WITH the OpenMLS engine module, in a real browser: two identities in one browser (the profile switcher),
// each its own vault, over the REAL carriage (ADR-0189) on the local reference Home (the Worker bundle in workerd, the dev relay
// harness): Invite and Accept travel as events on the 1:1, the Welcome and every message as sealed frames deposited at per-Room routes
// and polled from per-Room mailboxes. The groups module is a lazily loaded signed asset (ADR-0187): the page does not fetch it at first load, fetches
// and verifies it when a person first uses groups, then restarts onto it. The bundle is dev-marked only because the local Home needs the loopback flags. The Home only ever sees sealed objects; the test checks that on the wire.
import { expect, test, type Page } from "@playwright/test";
import { noPasskeys, onboard, walkSetup, addIdentity } from "./helpers.ts";

test.use({ baseURL: "http://localhost:4180" });

const RELAY = "http://127.0.0.1:8788";

type Who = "alice" | "bob";
async function asIdentity(page: Page, state: { who: Who }, who: Who): Promise<void> {
  if (state.who === who) return;
  await page.getByRole("button", { name: "Settings" }).waitFor();
  const other = page.getByRole("list", { name: "Identities" }).getByRole("button").and(page.locator(":not([aria-current])"));
  await other.click();
  await expect(page.getByRole("list", { name: "Identities" }).getByRole("button").and(page.locator("[aria-current]"))).toHaveCount(1);
  state.who = who;
}

test("groups on the real Core: invite over the 1:1, join, a message each way between two identities, through the real carriage", async ({ page }) => {
  const wire: { url: string; body: Buffer | null }[] = [];
  page.on("request", (r) => { if (r.url().startsWith(RELAY)) wire.push({ url: r.url(), body: r.postDataBuffer() }); });
  const wasmFetched: string[] = [];
  page.on("request", (r) => { if (r.url().endsWith(".wasm")) wasmFetched.push(r.url()); });
  await noPasskeys(page);
  await onboard(page, { stopAt: "invite" });
  await expect(page.getByText("E2E LOCAL RELAY, NOT PRODUCTION")).toBeVisible();
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
  expect(wasmFetched.filter((u) => /om_wasm_groups/.test(u))).toEqual([]); // first load costs the base module only
  expect(wasmFetched.filter((u) => /om_wasm-/.test(u)).length).toBeGreaterThan(0);
  await page.getByRole("button", { name: "New group" }).click();
  // First use on this device: the module is fetched, checked against the release digest, kept, and the app restarts onto it; the sheet then opens.
  await expect(page.getByRole("dialog", { name: "New group" })).toBeVisible({ timeout: 60_000 });
  expect(wasmFetched.filter((u) => /om_wasm_groups/.test(u)).length).toBeGreaterThan(0);
  const make = page.getByRole("dialog", { name: "New group" });
  await make.getByLabel("Group name (optional)").fill("Lunch club");
  await make.getByRole("checkbox").first().check();
  await make.getByRole("button", { name: /Create group/ }).click();
  await expect(page.getByRole("option", { name: /Lunch club/ })).toBeVisible();

  // Bob gets the invitation over the 1:1 and joins.
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

  // The Home saw deposits and polls only: no plaintext, and no capability in a URL.
  const deposits = wire.filter((w) => w.url.endsWith("/v1/relay/deposit"));
  expect(deposits.length).toBeGreaterThan(4);
  for (const w of wire) {
    expect(w.url).not.toMatch(/[?#]/);
    expect(w.body?.toString("latin1") ?? "").not.toMatch(/hello from bob|hi bob, alice here|Lunch club/);
  }
});
