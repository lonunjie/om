// Files, voice notes, location cards and local search, on the real Core in the production entry against a LOCKED local reference Home
// (the Worker bundle in workerd, holding only the hash of the owner's admission secret, which is what offers the blob spool). Two
// identities in one browser: Alice sends a small file, a voice note recorded by Chromium's fake microphone and a one-shot location
// card; Bob opens each one himself (nothing is fetched until he asks). An oversize file is refused in plain words before anything is
// uploaded. Bob then finds a message and a file name from the Omnibar. The Home only ever sees sealed bytes.
import { readFileSync } from "node:fs";
import { expect, test, type Page } from "@playwright/test";
import { E2E_ADMISSION_SECRET, LOCKED_RELAY } from "./admission-fixture.ts";
import { addIdentity, noPasskeys, walkSetup } from "./helpers.ts";

// Permissions are granted by name only where the engine knows the name: Playwright's Firefox rejects "microphone" at context creation.
// Geolocation is granted everywhere; the microphone, and the voice-note leg that needs it, only on Chromium (fake device flags in the config).
test.use({ baseURL: "http://localhost:4179", permissions: ["geolocation"], geolocation: { latitude: 51.5007, longitude: -0.1246, accuracy: 20 } });

const MARKER = "SEED-FILE-CONTENT-7f3a-do-not-leak";

type Who = "alice" | "bob";
async function asIdentity(page: Page, state: { who: Who }, who: Who): Promise<void> {
  if (state.who === who) return;
  await page.getByRole("button", { name: "Settings" }).waitFor();
  const other = page.getByRole("list", { name: "Identities" }).getByRole("button").and(page.locator(":not([aria-current])"));
  await other.click();
  await expect(page.getByRole("list", { name: "Identities" }).getByRole("button").and(page.locator("[aria-current]"))).toHaveCount(1);
  state.who = who;
}

test("files, a voice note and a location card cross a locked Home; oversize is refused plainly; search finds a message and a file", async ({ page, browserName }) => {
  const voice = browserName === "chromium";
  if (voice) await page.context().grantPermissions(["geolocation", "microphone"]);
  else test.info().annotations.push({ type: "skipped part", description: `voice note: ${browserName} cannot pre-grant a microphone or supply a fake media stream in this setup (Chromium only)` });
  await noPasskeys(page);
  // The browser's own file dialog cannot be driven; without it OM uses an ordinary file input, which Playwright can.
  await page.addInitScript(() => { Object.defineProperty(window, "showOpenFilePicker", { value: undefined, configurable: true }); });
  const toHome: { url: string; method: string; digest: string | undefined; body: Buffer | null }[] = [];
  page.on("request", (r) => { if (r.url().startsWith(LOCKED_RELAY)) toHome.push({ url: r.url(), method: r.method(), digest: r.headers()["om-ciphertext-digest"], body: r.postDataBuffer() }); });

  // First run: the person's own (locked) delivery service, with the owner secret this browser made earlier.
  await page.goto("/");
  await page.evaluate((hex) => new Promise<void>((resolve, reject) => {
    const open = indexedDB.open("om-provider-secrets", 1);
    open.onupgradeneeded = () => { open.result.createObjectStore("secrets"); };
    open.onerror = () => { reject(new Error("seed failed")); };
    open.onsuccess = () => {
      const tx = open.result.transaction("secrets", "readwrite");
      tx.objectStore("secrets").put(Uint8Array.from(hex.match(/../g) ?? [], (b) => parseInt(b, 16)), "admission-pending-v1");
      tx.oncomplete = () => { open.result.close(); resolve(); };
    };
  }), E2E_ADMISSION_SECRET.toString("hex"));
  await page.getByRole("button", { name: "Get started" }).click();
  await page.getByRole("button", { name: "Set up my own, free" }).click();
  await page.getByLabel("Address of your delivery service").fill(LOCKED_RELAY);
  await page.getByRole("button", { name: "Check and continue" }).click();
  await expect(page.getByRole("button", { name: "Start new" })).toBeVisible({ timeout: 30_000 });

  await walkSetup(page, { stopAt: "invite", name: "Alice" });
  await page.getByRole("button", { name: "Show my QR" }).click();
  const link = (await page.locator("code[translate=no]").first().textContent())?.trim() ?? "";
  expect(link).toMatch(/^https?:\/\/[^/]+\/#invite\/[A-Za-z0-9_-]+$/);
  await page.getByRole("button", { name: "Skip for now" }).click();
  await expect(page.getByRole("button", { name: "Settings" })).toBeVisible();
  await addIdentity(page);
  await walkSetup(page, { stopAt: "invite", name: "Bob" });
  await page.getByText("Have an invite?").click();
  await page.getByLabel("Invitation link").fill(link);
  await page.getByRole("button", { name: "Accept invitation" }).click();
  await expect(page.getByRole("button", { name: "Settings" })).toBeVisible();

  const me = { who: "alice" as Who };
  await page.getByRole("region", { name: "Message request" }).getByRole("button", { name: "Accept", exact: true }).click();
  await expect(page.getByRole("option")).toHaveCount(1, { timeout: 30_000 });
  const afterFetch = (): boolean => {
    const at = toHome.findIndex((w) => w.url.endsWith("/v1/keys/fetch"));
    return at >= 0 && toHome.slice(at).some((w) => w.url.endsWith("/v1/relay/deposit"));
  };
  await expect.poll(afterFetch, { timeout: 30_000 }).toBe(true);
  await page.waitForTimeout(1500);
  await page.getByRole("option").click();
  const composer = page.getByRole("textbox", { name: /message/i });
  const log = page.getByRole("log");
  await composer.fill("hello from alice");
  await page.getByRole("button", { name: /send/i }).click();
  await expect(log.getByText("hello from alice")).toBeVisible();

  // ---- a small file, with words ----
  const [chooser] = await Promise.all([page.waitForEvent("filechooser"), page.getByRole("button", { name: "Attach a file" }).click()]);
  await chooser.setFiles({ name: "note.txt", mimeType: "text/plain", buffer: Buffer.from(MARKER) });
  const tray = page.getByRole("list", { name: "Ready to send" });
  await expect(tray).toContainText("note.txt");
  await composer.fill("quarterly budget attached");
  await page.getByRole("button", { name: /send/i }).click();
  await expect(log.getByText("note.txt")).toBeVisible({ timeout: 30_000 });
  await expect(tray).toBeHidden();

  // ---- a voice note: record, listen, then send ----
  if (voice) {
    await page.getByRole("button", { name: "Record a voice note" }).click();
    await expect(page.getByRole("group", { name: "Recording" })).toBeVisible();
    await page.waitForTimeout(1500);
    await page.getByRole("button", { name: "Stop recording" }).click();
    await expect(tray).toContainText("Listen before you send");
    await expect(tray.locator("audio")).toBeVisible();
    await page.getByRole("button", { name: /send/i }).click();
    await expect(log.getByText("Voice note")).toBeVisible({ timeout: 30_000 });
  }

  // ---- a location card: nothing is sent until confirmed ----
  await page.getByRole("button", { name: "Share your location" }).click();
  const confirm = page.getByRole("group", { name: "Share your location?" });
  await expect(confirm).toContainText("51.5007, -0.1246");
  await expect(confirm).toContainText("once");
  await expect(log.getByRole("group", { name: "Location" })).toHaveCount(0);
  await confirm.getByRole("button", { name: "Send location" }).click();
  await expect(log.getByRole("group", { name: "Location" })).toBeVisible({ timeout: 30_000 });

  // ---- oversize: said in plain words, before any upload, with nothing to retry ----
  const uploadsBefore = toHome.filter((w) => w.url.endsWith("/v1/blobs")).length;
  const [chooser2] = await Promise.all([page.waitForEvent("filechooser"), page.getByRole("button", { name: "Attach a file" }).click()]);
  await chooser2.setFiles({ name: "huge.bin", mimeType: "application/octet-stream", buffer: Buffer.alloc(5 * 1024 * 1024, 1) });
  await expect(page.getByRole("alert")).toContainText("This file is too big to send right now.");
  await expect(tray).toBeHidden();
  await expect(page.getByRole("button", { name: /retry/i })).toHaveCount(0);
  expect(toHome.filter((w) => w.url.endsWith("/v1/blobs")).length).toBe(uploadsBefore);

  // ---- Bob opens each one himself ----
  await asIdentity(page, me, "bob");
  await page.getByRole("option").click();
  await expect(log.getByText("note.txt")).toBeVisible({ timeout: 60_000 });
  await expect(log.getByText("quarterly budget attached")).toBeVisible();
  await expect(page.locator("audio")).toHaveCount(0); // nothing was fetched or played on arrival
  const [download] = await Promise.all([page.waitForEvent("download"), log.getByRole("button", { name: "Download note.txt" }).click()]);
  expect(download.suggestedFilename()).toBe("note.txt");
  expect(readFileSync(await download.path(), "utf8")).toBe(MARKER);
  if (voice) {
    await log.getByRole("button", { name: "Play voice note" }).click();
    await expect(log.locator("audio")).toBeVisible({ timeout: 30_000 });
  }
  const card = log.getByRole("group", { name: "Location" });
  await expect(card).toBeVisible({ timeout: 30_000 });
  await expect(card).toContainText("51.5007, -0.1246");
  await expect(card.locator("img, iframe")).toHaveCount(0); // no map is fetched

  // ---- local search from the Omnibar: a sent line and a file name, found on this device ----
  await page.keyboard.press("Control+K");
  await page.keyboard.type("budget");
  const line = page.getByRole("option", { name: /quarterly budget attached/ });
  await expect(line).toBeVisible();
  await expect(line).toContainText("Message");
  await page.keyboard.press("Escape");
  await page.keyboard.press("Control+K");
  await page.keyboard.type("note.txt");
  const file = page.getByRole("option", { name: /note\.txt/ });
  await expect(file).toContainText("File");
  await file.click();
  await expect(log.getByText("note.txt")).toBeVisible();
  await expect(log.locator('li[data-found="true"]')).toContainText("note.txt"); // the hit opened at that exact message

  // ---- what the Home saw: sealed bytes with a digest header, no readable content, no capability in any URL ----
  const uploads = toHome.filter((w) => w.url.endsWith("/v1/blobs") && w.method === "POST");
  expect(uploads.length).toBeGreaterThanOrEqual(voice ? 2 : 1);
  for (const u of uploads) expect(u.digest ?? "").toMatch(/\S{20,}/);
  for (const w of toHome) {
    expect(w.url).not.toMatch(/[?#]/);
    const body = w.body?.toString("latin1") ?? "";
    expect(body).not.toContain(MARKER);
    expect(body).not.toContain("quarterly budget");
    expect(body).not.toContain("note.txt");
  }
});
