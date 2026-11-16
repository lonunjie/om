// Shared steps of the real-Core groups specs: each person is a browser context of their own (their own storage, vault, Core and groups
// module), all over the real carriage on the local reference Home (the Worker bundle in workerd). Nothing here reads Core's state: a person
// does only what a person can do on screen. The only thing passed between contexts is the invite link a person would send in any chat app.
import { expect, type Browser, type BrowserContext, type Page } from "@playwright/test";
import { noPasskeys, reopen, walkSetup } from "./helpers.ts";

export const GROUPS_APP = "http://localhost:4180";
export const RELAY = "http://127.0.0.1:8788";

export interface Person {
  readonly name: string;
  readonly page: Page;
  readonly context: BrowserContext;
  /** Every request this person's page made to the Home (so a spec can check what the Home saw, and when). */
  readonly wire: { readonly url: string; readonly method: string; readonly body: Buffer | null; readonly at: number }[];
  /** URLs of every `.wasm` this page fetched. */
  readonly wasm: string[];
}

function watch(name: string, page: Page, context: BrowserContext): Person {
  const wire: Person["wire"][number][] = [];
  const wasm: string[] = [];
  page.on("request", (r) => {
    if (r.url().startsWith(RELAY)) wire.push({ url: r.url(), method: r.method(), body: r.postDataBuffer(), at: Date.now() });
    if (r.url().endsWith(".wasm")) wasm.push(r.url());
  });
  return { name, page, context, wire, wasm };
}

async function newPage(browser: Browser): Promise<{ page: Page; context: BrowserContext }> {
  const context = await browser.newContext({ viewport: { width: 1280, height: 800 }, baseURL: GROUPS_APP });
  const page = await context.newPage();
  await noPasskeys(page);
  return { page, context };
}

/** A first-time person with no contacts, on the invite screen (their own link is one tap away). */
export async function startPerson(browser: Browser, name: string): Promise<Person> {
  const { page, context } = await newPage(browser);
  const p = watch(name, page, context);
  await page.goto("/");
  await page.getByRole("button", { name: "Get started" }).first().click();
  await walkSetup(page, { stopAt: "invite", name });
  await expect(page.getByText("E2E LOCAL RELAY, NOT PRODUCTION")).toBeVisible();
  return p;
}

/** The person's own invite link, from the first-run invite screen (and leaves that screen). */
export async function firstLink(p: Person): Promise<string> {
  await p.page.getByRole("button", { name: "Show my QR" }).click();
  const link = (await p.page.locator("code[translate=no]").first().textContent())?.trim() ?? "";
  expect(link).toMatch(/\/#invite\//);
  await p.page.getByRole("button", { name: "Skip for now" }).click();
  await expect(p.page.getByRole("button", { name: "Settings" })).toBeVisible();
  return link;
}

/** A further invite link from the shell: New conversation, Show my QR. */
export async function nextLink(p: Person): Promise<string> {
  await p.page.getByRole("button", { name: "New conversation" }).click();
  const sheet = p.page.getByRole("dialog", { name: "New conversation" });
  await sheet.getByRole("button", { name: "Show my QR" }).click();
  await expect(sheet.getByRole("figure", { name: "QR code for your invite" })).toBeVisible();
  const link = (await sheet.locator("code[translate=no]").first().textContent())?.trim() ?? "";
  expect(link).toMatch(/\/#invite\//);
  await p.page.keyboard.press("Escape");
  await expect(sheet).toBeHidden();
  return link;
}

/** A new person opens `link`, names themself, accepts; the inviter accepts the request. Both end with each other as an active chat. */
export async function joinByLink(browser: Browser, name: string, link: string, inviter: Person): Promise<Person> {
  const { page, context } = await newPage(browser);
  const p = watch(name, page, context);
  await page.goto(link);
  await expect(page.getByRole("heading", { name: "What should we call you?" })).toBeVisible();
  await page.getByLabel("Your name").fill(name);
  await page.getByRole("button", { name: "Next: Secure your identity" }).click();
  await expect(page.getByRole("heading", { name: "Accept this invite?" })).toBeVisible({ timeout: 60_000 });
  await page.getByRole("button", { name: "Accept invite" }).click();
  await expect(page.getByRole("heading", { name: inviter.name })).toBeVisible({ timeout: 60_000 });
  const request = inviter.page.getByRole("region", { name: "Message request" }).filter({ hasText: name });
  await expect(request).toBeVisible({ timeout: 60_000 });
  await request.getByRole("button", { name: "Accept", exact: true }).click();
  await expect(inviter.page.getByRole("option", { name: new RegExp(name) })).toBeVisible({ timeout: 60_000 });
  return p;
}

/** Two people who already use OM become contacts: `accepter` opens `owner`'s link in their running app and accepts; `owner` accepts the request. */
export async function meet(accepter: Person, owner: Person, link: string): Promise<void> {
  await accepter.page.goto(link);
  await reopen(accepter.page);
  const card = accepter.page.getByRole("heading", { name: "Accept this invite?" });
  await expect(card).toBeVisible({ timeout: 60_000 });
  await accepter.page.getByRole("button", { name: "Accept invite" }).click();
  const request = owner.page.getByRole("region", { name: "Message request" }).filter({ hasText: accepter.name });
  await expect(request).toBeVisible({ timeout: 60_000 });
  await request.getByRole("button", { name: "Accept", exact: true }).click();
  await expect(owner.page.getByRole("option", { name: new RegExp(accepter.name) })).toBeVisible({ timeout: 60_000 });
}

/** First use of groups on this device: the module is fetched, checked and kept, the app restarts onto it, and the New group sheet opens (then closed). */
export async function readyGroups(p: Person): Promise<void> {
  await p.page.getByRole("button", { name: "New group" }).click();
  const sheet = p.page.getByRole("dialog", { name: "New group" });
  await expect(sheet).toBeVisible({ timeout: 60_000 });
  await p.page.keyboard.press("Escape");
  await expect(sheet).toBeHidden();
}

export async function createGroup(p: Person, group: string, others: readonly string[], level?: "Hardened" | "Maximum Privacy"): Promise<void> {
  await p.page.getByRole("button", { name: "New group" }).click();
  const make = p.page.getByRole("dialog", { name: "New group" });
  await expect(make).toBeVisible({ timeout: 60_000 });
  await make.getByLabel("Group name (optional)").fill(group);
  for (const o of others) await make.getByRole("checkbox", { name: new RegExp(o) }).check();
  if (level !== undefined) {
    await make.getByText("Protection: Recommended").click();
    await make.getByRole("radio", { name: new RegExp(`^${level}`) }).check();
  }
  await make.getByRole("button", { name: /Create group/ }).click();
  await expect(p.page.getByRole("option", { name: new RegExp(group) })).toBeVisible();
}

/** The invitation card for `group` on this person's screen, joined with one tap; then the Room is open and it is possible to send. */
export async function joinGroup(p: Person, group: string, inviter: string): Promise<void> {
  const card = p.page.getByRole("region", { name: "Group invitations", exact: true }).locator(".om-group-invite").filter({ hasText: group });
  await expect(card.getByText(`${inviter} invited you to ${group}`)).toBeVisible({ timeout: 60_000 });
  await card.getByRole("button", { name: "Join" }).click();
  await openGroup(p, group);
  await expect(p.page.getByRole("textbox", { name: "Message the group" })).toBeEnabled({ timeout: 60_000 });
}

export async function openGroup(p: Person, group: string): Promise<void> {
  await p.page.getByRole("option", { name: new RegExp(group) }).click();
  await expect(p.page.getByRole("log", { name: "Group messages" })).toBeVisible();
}

export const roomLog = (p: Person) => p.page.getByRole("log", { name: "Group messages" });

/** Says `text` in the open Room and waits until it shows in this person's own log. */
export async function say(p: Person, text: string): Promise<void> {
  const box = p.page.getByRole("textbox", { name: "Message the group" });
  await expect(box).toBeEnabled({ timeout: 60_000 });
  await box.fill(text);
  await p.page.getByRole("button", { name: "Send" }).click();
  await expect(roomLog(p).getByText(text, { exact: true })).toBeVisible();
}

export async function hears(p: Person, text: string): Promise<void> {
  await expect(roomLog(p).getByText(text, { exact: true })).toBeVisible({ timeout: 60_000 });
}

/** What the Home saw is sealed objects only: no plaintext of the run, no capability or secret in a URL. */
export function expectSealedOnly(p: Person, plaintexts: readonly RegExp[]): void {
  for (const w of p.wire) {
    expect(w.url).not.toMatch(/[?#]/);
    const body = w.body?.toString("latin1") ?? "";
    for (const re of plaintexts) expect(body).not.toMatch(re);
  }
}

export async function closeAll(people: readonly Person[]): Promise<void> {
  for (const p of people) await p.context.close().catch(() => undefined);
}
