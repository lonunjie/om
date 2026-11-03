// The production-shaped bundle (dev-signed, real Core, service worker): it carries no delivery service (ADR-0186), so first run
// stops at the page where a person sets up their own and goes no further until one has answered. Nothing is queued, created or
// guessed in the meantime. (The setup that does continue is relay-setup.spec.ts; a connected app is relay.spec.ts.)
import { expect, test } from "@playwright/test";
import { BUNDLE_URL, noPasskeys } from "./helpers.ts";

test.use({ baseURL: BUNDLE_URL });

test("without a delivery service first run stops at the setup page, and says so rather than pretending", async ({ page }) => {
  await noPasskeys(page);
  await page.goto("/");
  await page.getByRole("button", { name: "Get started" }).click();
  await expect(page.getByRole("heading", { name: "OM is different" })).toBeVisible();
  await expect(page.getByText("You own your data, not us.")).toBeVisible();
  await expect(page.getByRole("button", { name: "Set up my own, free" })).toBeVisible();
  // The other ways are listed, not offered: no control for them exists.
  await expect(page.getByText("Coming later")).toBeVisible();
  await expect(page.getByRole("button", { name: /container|install script|tunnel|open ports/i })).toHaveCount(0);
  // The vendor is named where the choice is made, and only there.
  await expect(page.getByText("Uses Cloudflare. You will create or sign in to a free Cloudflare account, and to a GitHub or GitLab account, which Cloudflare needs to copy the setup into.")).toBeVisible();
  // No test banner: this is not the loopback build, and the next step (the person's name) is not reachable.
  await expect(page.getByText("LOOPBACK")).toHaveCount(0);
  await expect(page.getByLabel("Your name")).toHaveCount(0);
  await page.getByRole("button", { name: "Set up my own, free" }).click();
  await expect(page.getByRole("heading", { name: "Set up your delivery service" })).toBeVisible();
  await expect(page.getByText(/^[0-9a-f]{64}$/)).toBeVisible();
  // The production build carries the one-click setup link (the person's own template deployment), which opens in a new tab.
  const link = page.getByRole("link", { name: /Open Cloudflare setup/ });
  await expect(link).toHaveAttribute("href", "https://deploy.workers.cloudflare.com/?url=https://github.com/lonunjie/om-relay");
  await expect(link).toHaveAttribute("target", "_blank");
  // An address that is not a delivery service is explained and changes nothing.
  await page.getByLabel("Address of your delivery service").fill("http://not-secure.example");
  await page.getByRole("button", { name: "Check and continue" }).click();
  await expect(page.getByRole("alert")).toContainText("must start with https://");
  await expect(page.getByLabel("Your name")).toHaveCount(0);
  // After a reload first run is still waiting for a delivery service, and shows the same setup value (it was kept for the deployment in progress).
  const value = (await page.getByText(/^[0-9a-f]{64}$/).textContent()) ?? "";
  await page.reload();
  await page.getByRole("button", { name: "Get started" }).click();
  await page.getByRole("button", { name: "Set up my own, free" }).click();
  await expect(page.getByText(value)).toBeVisible();
});
