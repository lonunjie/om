// ADR-0254 on the real expressive Core and the locked loopback Home.
// Build the expressive Wasm artifact, then build:setup-e2e with VITE_OM_EXPRESSIVE_MEDIA=1.
// OM_EXPRESSIVE_E2E=1 runs this spec against that opt-in setup bundle.
import { readFileSync } from "node:fs";
import { expect, test, type Locator, type Page } from "@playwright/test";
import { E2E_ADMISSION_SECRET, LOCKED_RELAY } from "./admission-fixture.ts";
import { addIdentity, chooseOwnRelay, noPasskeys, walkSetup } from "./helpers.ts";

const APP = "http://localhost:4179";
const vectors = JSON.parse(readFileSync(new URL("../../../verification/vectors/pack-file-vectors.json", import.meta.url), "utf8")) as {
  vectors: { id: string; hex: string }[];
};
const first = vectors.vectors.find(({ id }) => id === "genesis");
const successor = vectors.vectors.find(({ id }) => id === "portable-update");
if (first === undefined || successor === undefined) throw new Error("pack vectors missing");

test.use({ baseURL: APP });
test.skip(process.env["OM_EXPRESSIVE_E2E"] !== "1", "Uses the explicit expressive setup bundle");

async function pick(page: Page, button: Locator, name: string, hex: string): Promise<void> {
  const [chooser] = await Promise.all([page.waitForEvent("filechooser"), button.click()]);
  // The ordinary file composer recognizes the extension without relying on the OS MIME mapping.
  await chooser.setFiles({ name, mimeType: "application/octet-stream", buffer: Buffer.from(hex, "hex") });
}

test("Bob reviews Alice's attached pack, installs it and sends a sticker", async ({ page }) => {
  test.setTimeout(120_000);
  await noPasskeys(page);
  await page.emulateMedia({ reducedMotion: "reduce" });
  await page.addInitScript(() => { Object.defineProperty(window, "showOpenFilePicker", { value: undefined, configurable: true }); });
  await page.goto("/");
  await page.evaluate((hex) => new Promise<void>((resolve, reject) => {
    const open = indexedDB.open("om-provider-secrets", 1);
    open.onupgradeneeded = () => { open.result.createObjectStore("secrets"); };
    open.onerror = () => { reject(new Error("seed failed")); };
    open.onsuccess = () => {
      const tx = open.result.transaction("secrets", "readwrite");
      tx.objectStore("secrets").put(Uint8Array.from(hex.match(/../g) ?? [], (b) => parseInt(b, 16)), "admission-pending-v1");
      tx.oncomplete = () => { open.result.close(); resolve(); };
      tx.onerror = () => { open.result.close(); reject(new Error("seed failed")); };
    };
  }), E2E_ADMISSION_SECRET.toString("hex"));

  await page.getByRole("button", { name: "Get started" }).click();
  await chooseOwnRelay(page);
  await page.getByLabel("Address of your delivery service").fill(LOCKED_RELAY);
  await page.getByRole("button", { name: "Check and continue" }).click();
  await expect(page.getByRole("button", { name: "Start new" })).toBeVisible();
  await walkSetup(page, { stopAt: "invite", name: "Alice" });
  await page.getByRole("button", { name: "Show my QR" }).click();
  const link = (await page.locator("code[translate=no]").first().textContent())?.trim() ?? "";
  expect(link).toMatch(/#invite\/[A-Za-z0-9_-]+$/);
  await page.getByRole("button", { name: "Skip for now" }).click();
  await addIdentity(page);
  await walkSetup(page, { stopAt: "invite", name: "Bob" });
  await page.getByText("Have an invite?").click();
  await page.getByLabel("Invitation link").fill(link);
  await page.getByRole("button", { name: "Accept invitation" }).click();
  await page.getByRole("region", { name: "Message request" }).getByRole("button", { name: "Accept", exact: true }).click();
  await page.getByRole("listbox", { name: "Conversations" }).getByRole("option").click();

  const log = page.getByRole("log", { name: "Messages" });
  await pick(page, page.getByRole("button", { name: "Attach a file" }), "faces.ompack", first.hex);
  await expect(page.getByRole("list", { name: "Ready to send" })).toContainText("faces.ompack");
  await page.getByRole("button", { name: "Send", exact: true }).click();
  await expect(log.getByText("faces.ompack")).toBeVisible();

  await page.getByRole("list", { name: "Identities" }).getByRole("button", { name: "Bob", exact: true }).click();
  await page.getByRole("listbox", { name: "Conversations" }).getByRole("option").click();
  const review = log.getByRole("button", { name: "Review pack" });
  await expect(review).toBeVisible();
  const requests: { url: URL; method: string }[] = [];
  page.on("request", (r) => { requests.push({ url: new URL(r.url()), method: r.method() }); });
  const blobReads = () => requests.filter(({ url, method }) => method === "GET" && url.pathname.startsWith("/v1/blobs/"));
  const thirdParty = () => requests.filter(({ url }) => url.origin !== APP && url.origin !== LOCKED_RELAY);

  await review.click();
  const dialog = page.getByRole("dialog", { name: "Review pack" });
  await expect(dialog.getByText("File sent by")).toHaveText("File sent by Alice");
  await dialog.getByRole("button", { name: "Install Little faces" }).click();
  await expect(dialog.getByText("Pack saved on this device.")).toBeVisible();
  expect(blobReads()).toHaveLength(1);
  expect(thirdParty()).toEqual([]);
  await dialog.getByRole("button", { name: "Close", exact: true }).click();

  const stickers = page.getByRole("button", { name: "Stickers", exact: true });
  const picker = page.getByRole("dialog", { name: "Stickers" });
  await stickers.click();
  await picker.getByRole("button", { name: "Little faces" }).click();
  await picker.getByRole("button", { name: "Send Hello smile" }).click();
  await expect(picker.getByText("Sticker sent.")).toBeVisible();
  await picker.getByRole("button", { name: "Close", exact: true }).click();

  // The same pack is updated from local bytes without another attachment request.
  requests.length = 0;
  await stickers.click();
  await pick(page, picker.getByRole("button", { name: "Choose pack file" }), "faces-update.ompack", successor.hex);
  await picker.getByRole("button", { name: "Update Little faces" }).click();
  await expect(picker.getByText("Pack saved on this device.")).toBeVisible();
  expect(blobReads()).toEqual([]);
  expect(thirdParty()).toEqual([]);
  await picker.getByRole("button", { name: "Close", exact: true }).click();

  await page.getByRole("list", { name: "Identities" }).getByRole("button", { name: "Alice", exact: true }).click();
  await page.getByRole("listbox", { name: "Conversations" }).getByRole("option").click();
  await expect(log.getByText("🙂", { exact: true })).toBeVisible();
  const openImage = log.getByRole("button", { name: "Show image" });
  await openImage.click();
  await expect(log.getByRole("img", { name: "A smiling sticker" })).toBeVisible();
});
