// Chromium only: CDP virtual authenticators (PRF passkeys) and a fake camera that plays a QR clip.
import { expect, test, type Page } from "@playwright/test";
import { BUNDLE_URL, noPasskeys, onboard } from "./helpers.ts";

interface AuthOptions { readonly prf: boolean; readonly backupEligible: boolean }

async function virtualAuthenticator(page: Page, o: AuthOptions): Promise<void> {
  const cdp = await page.context().newCDPSession(page);
  await cdp.send("WebAuthn.enable");
  await cdp.send("WebAuthn.addVirtualAuthenticator", {
    options: {
      protocol: "ctap2", ctap2Version: "ctap2_1", transport: "internal", hasResidentKey: true, hasUserVerification: true,
      isUserVerified: true, automaticPresenceSimulation: true, hasPrf: o.prf,
      defaultBackupEligibility: o.backupEligible, defaultBackupState: o.backupEligible,
    },
  });
}

test.describe("passkey wrapping", () => {
  test("a device-bound PRF passkey wraps the vault with no reduced-protection notice", async ({ page }) => {
    await virtualAuthenticator(page, { prf: true, backupEligible: false });
    await onboard(page, { stopAt: "protected" });
    await expect(page.getByText("Your data is locked with a passkey from this device or your password manager.")).toBeVisible();
    await expect(page.getByText(/synced between your devices/)).toHaveCount(0);
  });

  test("a synced passkey wraps the vault and the reduced at-rest property is stated", async ({ page }) => {
    await virtualAuthenticator(page, { prf: true, backupEligible: true });
    await onboard(page, { stopAt: "protected" });
    await expect(page.getByText("Your data is locked with a passkey from this device or your password manager.")).toBeVisible();
    await expect(page.getByText(/synced between your devices/)).toBeVisible();
  });

  test("an authenticator without PRF falls back to a browser-held key and says so", async ({ page }) => {
    await virtualAuthenticator(page, { prf: false, backupEligible: false });
    await onboard(page, { stopAt: "protected" });
    await expect(page.getByText("Your data is locked with a key kept in this browser.")).toBeVisible();
    await expect(page.getByText(/can open your local data/)).toBeVisible();
  });

  test("passphrase layered over a PRF passkey completes onboarding", async ({ page }, info) => {
    await virtualAuthenticator(page, { prf: true, backupEligible: false });
    const { lockMs } = await onboard(page, { passphrase: "orbit lantern velvet coast", stopAt: "invite" });
    info.annotations.push({ type: "argon2id-lock-ms", description: `chromium+prf: ${String(lockMs)}` });
  });
});

test("the camera scanner decodes a QR code from a real video stream and accepts the invitation", async ({ page, context }) => {
  await context.grantPermissions(["camera"]);
  await noPasskeys(page);
  await onboard(page, { stopAt: "invite" });
  await page.getByRole("button", { name: "Scan a QR code" }).click();
  await expect(page.getByRole("dialog")).toBeVisible();
  // The dialog closes by itself once a frame decodes; the accepted link moves the flow on.
  await expect(page.getByRole("heading", { name: "You are set up" })).toBeVisible({ timeout: 30_000 });
  await expect(page.getByRole("dialog")).toHaveCount(0);
});

test.describe("service worker on the dev-signed production bundle", () => {
  test("installs, controls the page after reload, and serves the shell offline", async ({ page, context }) => {
    await page.goto(BUNDLE_URL + "/");
    await expect(page.getByRole("heading", { name: "Private messaging that nobody operates for you" }).first()).toBeVisible();
    await page.evaluate(async () => { await navigator.serviceWorker.ready; });
    await page.reload();
    await expect.poll(() => page.evaluate(() => navigator.serviceWorker.controller !== null)).toBe(true);
    await context.setOffline(true);
    await page.reload();
    await expect(page.getByRole("heading", { name: "Private messaging that nobody operates for you" }).first()).toBeVisible();
    await context.setOffline(false);
  });

  test("security headers: strict CSP, Trusted Types, camera allowed for self only", async ({ request }) => {
    const res = await request.get(BUNDLE_URL + "/");
    const csp = res.headers()["content-security-policy"] ?? "";
    expect(csp).toContain("default-src 'none'");
    expect(csp).toContain("require-trusted-types-for 'script'");
    expect(res.headers()["permissions-policy"]).toContain("camera=(self)");
    expect(res.headers()["permissions-policy"]).toContain("microphone=()");
  });
});
