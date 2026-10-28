// The production entry (real Core, vault storage, the HTTP provider adapter) connected to the local reference Home: the
// Worker bundle a deploy would upload, running in workerd. Two identities in one browser invite, introduce and message
// each other through it, and a reload does not lose the install's right to its own mailboxes. Everything the Home sees
// is captured on the wire and checked for plaintext and for capabilities in URLs.
import { expect, test, type Page } from "@playwright/test";
import { noPasskeys, onboard, walkSetup } from "./helpers.ts";

test.use({ baseURL: "http://localhost:4177" });

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

test("two identities talk through the configured reference Home; the Home sees only sealed objects", async ({ page }) => {
  const wire: { url: string; method: string; body: Buffer | null }[] = [];
  page.on("request", (r) => { if (r.url().startsWith(RELAY)) wire.push({ url: r.url(), method: r.method(), body: r.postDataBuffer() }); });
  await noPasskeys(page);

  await onboard(page, { stopAt: "invite" });
  await expect(page.getByText("E2E LOCAL RELAY, NOT PRODUCTION")).toBeVisible();
  await expect(page.getByText("Not connected to a provider")).toHaveCount(0);
  await page.getByRole("button", { name: "Show as QR code" }).click();
  const link = (await page.locator("code[translate=no]").first().textContent())?.trim() ?? "";
  expect(link).toMatch(/^om:invite\/[A-Za-z0-9_-]+$/);
  await page.getByRole("button", { name: "Skip for now" }).click();
  await page.getByRole("button", { name: "Open my chats" }).click();
  await expect(page.getByRole("button", { name: "Settings" })).toBeVisible();

  await page.getByRole("button", { name: "Add identity" }).click();
  await walkSetup(page, { stopAt: "invite" });
  await page.getByLabel("Invitation link").fill(link);
  await page.getByRole("button", { name: "Accept invitation" }).click();
  await page.getByRole("button", { name: "Open my chats" }).click();
  await expect(page.getByRole("button", { name: "Settings" })).toBeVisible();

  const me = { who: "alice" as Who };
  await page.getByRole("region", { name: "Message request" }).getByRole("button", { name: "Accept", exact: true }).click();
  await expect(page.getByRole("option")).toHaveCount(1, { timeout: 30_000 });
  // A real network is slower than a loopback: the first message can only be queued once the introduction has reached the
  // Home (Alice has fetched Bob's keys and deposited her reply), so wait for that instead of racing it.
  const afterFetch = (): boolean => {
    const at = wire.findIndex((w) => w.url.endsWith("/v1/keys/fetch"));
    return at >= 0 && wire.slice(at).some((w) => w.url.endsWith("/v1/relay/deposit"));
  };
  await expect.poll(afterFetch, { timeout: 30_000 }).toBe(true);
  await page.waitForTimeout(1500);
  await page.getByRole("option").click();
  await page.getByRole("textbox", { name: /message/i }).fill("hello from alice");
  await page.getByRole("button", { name: /send/i }).click();
  await expect(page.getByText("hello from alice")).toBeVisible();

  await asIdentity(page, me, "bob");
  await expect(page.getByRole("option")).toHaveCount(1, { timeout: 30_000 });
  await page.getByRole("option").click();
  await expect(page.getByText("hello from alice")).toBeVisible({ timeout: 30_000 });
  await page.getByRole("textbox", { name: /message/i }).fill("hi alice, this is bob");
  await page.getByRole("button", { name: /send/i }).click();

  await asIdentity(page, me, "alice");
  await page.getByRole("option").click();
  await expect(page.getByText("hi alice, this is bob")).toBeVisible({ timeout: 30_000 });

  // Reload: unlock both identities; the install keeps its right to its own mailboxes, so a new message still arrives.
  await page.reload();
  for (let n = 0; n < 2; n++) {
    await expect(page.getByRole("heading", { name: /unlock/i }).first()).toBeVisible({ timeout: 30_000 });
    await page.getByRole("button", { name: "Unlock", exact: true }).click();
  }
  await expect(page.getByRole("button", { name: "Settings" })).toBeVisible({ timeout: 30_000 });
  const after = { who: "alice" as Who };
  await page.getByRole("option").click();
  await page.getByRole("textbox", { name: /message/i }).fill("after the reload");
  await page.getByRole("button", { name: /send/i }).click();
  await asIdentity(page, after, "bob");
  await page.getByRole("option").click();
  await expect(page.getByText("after the reload")).toBeVisible({ timeout: 30_000 });

  // What the Home saw: requests to API paths only, no capability or query in any URL, and no plaintext in any body.
  expect(wire.length).toBeGreaterThan(15);
  for (const w of wire) {
    expect(w.url).not.toMatch(/[?#]/);
    const body = w.body?.toString("latin1") ?? "";
    for (const secret of ["hello from alice", "hi alice, this is bob", "after the reload"]) expect(body).not.toContain(secret);
  }
  const paths = new Set(wire.filter((w) => w.method !== "OPTIONS").map((w) => new URL(w.url).pathname.replace(/[0-9a-f]{32}/g, ":id")));
  expect(paths).toContain("/v1/relay/deposit");
  expect(paths).toContain("/v1/relay/drain");
  expect(paths).toContain("/v1/relay/ack");
  expect(paths).toContain("/v1/relay/queues");
  expect(paths).toContain("/v1/keys/fetch");
  expect(paths).toContain("/v1/keys/bundles/:id");
  expect(paths).toContain("/v1/discovery/descriptors/:id");
});

test("Settings shows the Home in use with what it says about itself, and refuses an address that is not https", async ({ page }) => {
  await noPasskeys(page);
  await onboard(page, { stopAt: "invite" });
  await page.getByRole("button", { name: "Skip for now" }).click();
  await page.getByRole("button", { name: "Open my chats" }).click();
  await page.getByRole("button", { name: "Settings" }).click();
  await page.getByRole("button", { name: "Your Home" }).click();
  await expect(page.getByRole("heading", { level: 1, name: "Your Home" })).toBeVisible();
  await expect(page.getByText(`Address: ${RELAY}`)).toBeVisible();
  await expect(page.getByText("This address answered and calls itself")).toBeVisible();
  await expect(page.getByText("It reports that it is working normally.")).toBeVisible();
  await page.getByLabel("Home address").fill("http://home.example");
  await page.getByRole("button", { name: "Check this Home" }).click();
  await expect(page.getByRole("alert")).toContainText("must start with https://");
  await expect(page.getByRole("button", { name: /Use this Home|Use it anyway/ })).toHaveCount(0);
});
