// First run (ADR-0186) with synthetic vendor pages and the real local locked Home. The vendor fixtures rehearse the
// handoffs and possible failures. The separate operator run checks actual account, permission and deployment screens.
import { createHash } from "node:crypto";
import { expect, test, type Page } from "@playwright/test";
import { E2E_ADMISSION_HASH, E2E_ADMISSION_SECRET, LOCKED_RELAY } from "./admission-fixture.ts";
import { noPasskeys, walkSetup } from "./helpers.ts";
import { CLOUDFLARE_SIGNUP, DEPLOY, GITHUB_SIGNUP, GITLAB_SIGNUP, SETUP_APP, rehearseSetup } from "./relay-setup-fixture.ts";

test.use({ baseURL: SETUP_APP, serviceWorkers: "block" });

const ADDRESS = "Address of your delivery service";
const HEADING = "Set up your delivery service";
const FORGE = "2. A GitHub or GitLab account";

/** Reads one entry of the adapter's secret store (a tiny IndexedDB) from the page, as hex, or null. */
async function secret(page: Page, name: string): Promise<string | null> {
  return page.evaluate((key) => new Promise<string | null>((resolve) => {
    const open = indexedDB.open("om-provider-secrets", 1);
    open.onerror = () => { resolve(null); };
    open.onsuccess = () => {
      const db = open.result;
      if (!db.objectStoreNames.contains("secrets")) { resolve(null); return; }
      const q = db.transaction("secrets", "readonly").objectStore("secrets").get(key);
      q.onsuccess = () => { const v: unknown = q.result; resolve(v instanceof Uint8Array && v.length > 0 ? Array.from(v, (b) => b.toString(16).padStart(2, "0")).join("") : null); };
      q.onerror = () => { resolve(null); };
    };
  }), name);
}

async function startSetup(page: Page): Promise<void> {
  await noPasskeys(page);
  // Where the browser would refuse a copy without a permission prompt, an in-page clipboard stands in (the value is read back from the page).
  await page.addInitScript(() => {
    try {
      Object.defineProperty(navigator, "clipboard", { value: { writeText: () => Promise.resolve(), readText: () => Promise.resolve("") } });
    } catch { /* the browser's own clipboard stays */ }
  });
  await page.goto("/");
  // As if the device had made its secret earlier: seed the adapter's (empty) secret store, so the setup value is the hash this locked Home was started with.
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
  await expect(page.getByRole("heading", { name: "OM is different" })).toBeVisible();
  await page.getByRole("button", { name: "Set up my own, free" }).click();
  await expect(page.getByRole("heading", { name: "Before you start" })).toBeVisible();
}

const cloudflareConfirm = (page: Page) => page.getByRole("region", { name: "1. A Cloudflare account" }).getByRole("button", { name: "I have it" });
const forgeConfirm = (page: Page) => page.getByRole("region", { name: FORGE }).getByRole("button", { name: "I have it" });
const proceed = (page: Page) => page.getByRole("button", { name: "Continue", exact: true });

async function createTestAccount(page: Page, url: string, action: string): Promise<Page> {
  const tab = await page.context().newPage();
  await tab.goto(url);
  await tab.getByRole("button", { name: action, exact: true }).click();
  return tab;
}

async function confirmAndCopy(page: Page): Promise<string> {
  await forgeConfirm(page).click();
  await proceed(page).click();
  await expect(page.getByRole("heading", { name: HEADING })).toBeVisible();
  await page.getByRole("button", { name: "Copy setup value" }).click();
  await expect(page.getByText("Setup value copied", { exact: true })).toBeVisible();
  return (await page.getByText(/^[0-9a-f]{64}$/).textContent()) ?? "";
}

async function openDeployment(page: Page): Promise<Page> {
  const [tab] = await Promise.all([
    page.context().waitForEvent("page"),
    page.getByRole("link", { name: /Open Cloudflare setup/ }).click(),
  ]);
  await expect(tab).toHaveURL(DEPLOY);
  await tab.waitForLoadState();
  expect(await tab.evaluate(() => window.opener === null)).toBe(true);
  return tab;
}

async function submitDeployment(tab: Page, value: string, heading: string, label = "OM_ADMISSION"): Promise<void> {
  await tab.getByLabel(label, { exact: true }).fill(value);
  await tab.getByRole("button", { name: "Deploy", exact: true }).click();
  await expect(tab.getByRole("heading", { name: heading, exact: true })).toBeVisible();
}

async function useDeployedAddress(page: Page, tab: Page): Promise<void> {
  const address = await tab.locator("output").innerText();
  expect(address).toBe(LOCKED_RELAY);
  await tab.close();
  await page.getByLabel(ADDRESS).fill(address);
  await page.getByRole("button", { name: "Check and continue" }).click();
  await expect(page.getByRole("button", { name: "Start new" })).toBeVisible();
}

test("selected repositories stall in the rehearsal, then full access on a test account permits setup and first run", async ({ page, context }) => {
  const { unexpected } = await rehearseSetup(context, { failures: true });
  const wire: string[] = [];
  const refused: string[] = [];
  page.on("request", (r) => { if (new URL(r.url()).origin === LOCKED_RELAY) wire.push(`${r.method()} ${new URL(r.url()).pathname}`); });
  page.on("response", (r) => { if (new URL(r.url()).origin === LOCKED_RELAY && (r.status() === 401 || r.status() === 403)) refused.push(`${String(r.status())} ${new URL(r.url()).pathname}`); });
  await startSetup(page);

  await expect(cloudflareConfirm(page)).toHaveAttribute("aria-pressed", "false");
  await expect(forgeConfirm(page)).toHaveAttribute("aria-pressed", "false");
  await expect(proceed(page)).toBeDisabled();
  await (await createTestAccount(page, CLOUDFLARE_SIGNUP, "Create free account")).close();
  await cloudflareConfirm(page).click();
  await expect(proceed(page)).toBeDisabled();
  await cloudflareConfirm(page).click();
  await expect(cloudflareConfirm(page)).toHaveAttribute("aria-pressed", "false");
  await cloudflareConfirm(page).click();
  const github = await createTestAccount(page, GITHUB_SIGNUP, "Create account");
  await github.getByRole("button", { name: "Create empty repository" }).click();
  await expect(github.getByRole("heading", { name: "Empty repository created" })).toBeVisible();
  await github.close();

  const shown = await confirmAndCopy(page);
  expect(shown).toBe(E2E_ADMISSION_HASH);
  const pending = await secret(page, "admission-pending-v1");
  expect(pending).not.toBeNull();
  expect(createHash("sha256").update("OM-RELAY-ADMISSION-v1").update(Buffer.from(pending ?? "", "hex")).digest("hex")).toBe(shown);

  const selected = await openDeployment(page);
  await selected.getByRole("link", { name: "Connect GitHub" }).click();
  await selected.getByRole("checkbox", { name: "setup-test" }).check();
  await selected.getByRole("button", { name: "Allow selected repositories" }).click();
  await expect(selected.getByRole("heading", { name: "Selected repository access saved" })).toBeVisible();
  await selected.close();

  await expect(page.getByText(/If you land on GitHub or setup stops/)).toBeVisible();
  const tab = await openDeployment(page);
  await expect(tab.getByRole("heading", { name: "Repository access needs attention" })).toBeVisible();
  await page.getByText("If setup stops", { exact: true }).click();
  await expect(page.getByText(/If setup stops after you allow only selected repositories/)).toBeVisible();
  await tab.getByRole("link", { name: "Review GitHub access" }).click();
  await tab.getByRole("button", { name: "Allow all repositories" }).click();
  await tab.getByRole("link", { name: "Return to Cloudflare" }).click();
  await expect(tab.getByRole("heading", { name: "Configure deployment" })).toBeVisible();

  await submitDeployment(tab, LOCKED_RELAY, "Setup value rejected");
  await expect(page.getByText(/If setup rejects the value, copy it again/)).toBeVisible();
  await tab.getByRole("link", { name: "Back to deployment" }).click();
  await submitDeployment(tab, shown, "Free plan limit reached");
  await expect(page.getByText(/If Cloudflare says you reached a free plan limit/)).toBeVisible();
  await tab.getByRole("link", { name: "Back to deployment" }).click();
  await submitDeployment(tab, shown, "Deployment failed");
  await expect(page.getByText(/If deployment fails, read the last error/)).toBeVisible();
  await tab.getByRole("link", { name: "Back to deployment" }).click();
  await submitDeployment(tab, shown, "Deployment complete");

  await page.getByLabel(ADDRESS).fill(shown);
  await page.getByRole("button", { name: "Check and continue" }).click();
  await expect(page.getByRole("alert")).toHaveText(/That looks like a setup value/);
  await page.getByLabel(ADDRESS).fill("http://bad.example.test");
  await page.getByRole("button", { name: "Check and continue" }).click();
  await expect(page.getByRole("alert")).toBeVisible();
  expect(wire).toEqual([]);
  await useDeployedAddress(page, tab);
  expect(wire).toContain("GET /.well-known/om-provider-capabilities");

  // Stored for this address only, and the pending secret is retired.
  expect(await secret(page, `admission-v1:${LOCKED_RELAY}`)).toBe(pending);
  expect(await secret(page, "admission-pending-v1")).toBeNull();

  await walkSetup(page, { stopAt: "invite" });
  await expect(page.getByText("Not connected to a delivery service")).toHaveCount(0);
  // Making an invite creates a mailbox at the locked Home: it only works with the admission secret kept for this address.
  await page.getByRole("button", { name: "Show my QR" }).click();
  await expect(page.locator("code[translate=no]").first()).toContainText("/#invite/");
  expect(refused).toEqual([]);
  expect(wire.some((w) => w.startsWith("POST /v1/"))).toBe(true);
  await page.getByRole("button", { name: "Skip for now" }).click();
  await expect(page.getByRole("button", { name: "Settings" })).toBeVisible();
  await page.reload();
  await expect(page.getByRole("button", { name: "Settings" })).toBeVisible({ timeout: 30_000 });
  await expect(page.getByText("Not connected to a delivery service")).toHaveCount(0);
  expect(unexpected).toEqual([]);
});

test("a setup-only GitHub account starts with no repositories", async ({ page, context }) => {
  const { unexpected } = await rehearseSetup(context);
  await startSetup(page);
  await (await createTestAccount(page, CLOUDFLARE_SIGNUP, "Create free account")).close();
  await cloudflareConfirm(page).click();
  await (await createTestAccount(page, GITHUB_SIGNUP, "Create account")).close();
  const value = await confirmAndCopy(page);
  const tab = await openDeployment(page);
  await tab.getByRole("link", { name: "Connect GitHub" }).click();
  await expect(tab.getByText("No existing repositories.")).toBeVisible();
  await tab.getByRole("button", { name: "Allow all repositories" }).click();
  await tab.getByRole("link", { name: "Return to Cloudflare" }).click();
  await submitDeployment(tab, value, "Deployment complete");
  await useDeployedAddress(page, tab);
  expect(unexpected).toEqual([]);
});

test("someone without GitHub can continue in the rehearsal if GitLab is offered", async ({ page, context }) => {
  const { unexpected } = await rehearseSetup(context, { gitLabOffered: true });
  await startSetup(page);
  await expect(page.getByText(/try GitLab if Cloudflare offers it/)).toBeVisible();
  await (await createTestAccount(page, CLOUDFLARE_SIGNUP, "Create free account")).close();
  await cloudflareConfirm(page).click();
  await (await createTestAccount(page, GITLAB_SIGNUP, "Create account")).close();
  const value = await confirmAndCopy(page);
  const tab = await openDeployment(page);
  await tab.getByRole("link", { name: "Connect GitLab" }).click();
  await tab.getByRole("button", { name: "Allow setup" }).click();
  await tab.getByRole("link", { name: "Return to Cloudflare" }).click();
  await submitDeployment(tab, value, "Deployment complete", "OM setup value");
  await useDeployedAddress(page, tab);
  expect(unexpected).toEqual([]);
});

test("someone without GitHub can return to OM when the setup page offers no GitLab choice", async ({ page, context }) => {
  const { unexpected } = await rehearseSetup(context);
  await startSetup(page);
  await (await createTestAccount(page, CLOUDFLARE_SIGNUP, "Create free account")).close();
  await cloudflareConfirm(page).click();
  await expect(proceed(page)).toBeDisabled();
  await expect(page.getByText(/If neither option is available to you/)).toBeVisible();
  await (await createTestAccount(page, GITLAB_SIGNUP, "Create account")).close();
  const value = await confirmAndCopy(page);
  const tab = await openDeployment(page);
  await expect(tab.getByRole("link", { name: "Connect GitHub" })).toBeVisible();
  await expect(tab.getByRole("link", { name: "Connect GitLab" })).toHaveCount(0);
  await expect(tab.getByLabel("OM_ADMISSION")).toHaveCount(0);
  await tab.close();
  await page.getByText("If setup stops", { exact: true }).click();
  await expect(page.getByText(/If you use GitLab and do not see it as a choice/)).toBeVisible();
  await page.getByRole("button", { name: "Back", exact: true }).click();
  await expect(forgeConfirm(page)).toHaveAttribute("aria-pressed", "true");
  await forgeConfirm(page).click();
  await expect(proceed(page)).toBeDisabled();
  expect(await secret(page, `admission-v1:${LOCKED_RELAY}`)).toBeNull();
  expect(await secret(page, "admission-pending-v1")).toBe(E2E_ADMISSION_SECRET.toString("hex"));
  await expect(page.getByText(value, { exact: true })).toHaveCount(0);
  expect(unexpected).toEqual([]);
});
