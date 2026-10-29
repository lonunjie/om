// Landing prerender, manifest and icon validity, install affordance. Chromium project (matches *.spec.ts, not cross.spec.ts).
import { expect, test } from "@playwright/test";
import { BUNDLE_URL } from "./helpers.ts";

test("index.html carries the landing text and metadata without running any JavaScript", async ({ request }) => {
  const res = await request.get(BUNDLE_URL + "/");
  expect(res.ok()).toBe(true);
  const html = await res.text();
  const root = /<div id="root">([\s\S]*)<\/body>/.exec(html)?.[1] ?? "";
  for (const text of [
    "The messaging app you own", "no single company, server, or app store is indispensable", "Why make another app?", "What’s in a name?",
    "Beta", "Security &amp; privacy", "Add to Home Screen", "Install app", "Run your own copy", "https://github.com/lonunjie/om", "Get started",
  ]) expect(root, text).toContain(text);
  expect(html).toMatch(/<meta name="description" content="[^"]{40,}"/);
  expect(html).toMatch(/<meta property="og:image" content="[^"]+og-image\.png"/);
  expect(html).toMatch(/<meta name="twitter:card"/);
  expect(html).toMatch(/<meta name="theme-color" content="#F7F8FA" media="\(prefers-color-scheme: light\)"/);
  expect(html).toMatch(/<meta name="theme-color" content="#0F1217" media="\(prefers-color-scheme: dark\)"/);
  expect(html).toMatch(/rel="apple-touch-icon"/);
  expect(html).not.toMatch(/<script[^>]+src="https?:/); // nothing third-party
});

test("with JavaScript the app replaces the landing page only after Get started", async ({ page }) => {
  await page.goto(BUNDLE_URL + "/");
  await expect(page.getByRole("heading", { level: 1 })).toHaveText("The messaging app you own");
  await page.getByRole("button", { name: "Get started" }).click();
  // The real Core starts, and the first run continues into device setup (no provider is configured in this build).
  await expect(page.getByRole("button", { name: "Lock this device" })).toBeVisible();
  await expect(page.getByRole("heading", { name: "Run your own copy" })).toHaveCount(0);
});

test("the dev harness shows the landing page first and Get started opens onboarding", async ({ page }) => {
  await page.goto("/");
  await expect(page.getByRole("heading", { name: "Run your own copy" })).toBeVisible();
  await page.getByRole("button", { name: "Get started" }).click();
  await expect(page.getByRole("heading", { name: "Lock your data on this device" })).toBeVisible();
});

test("the manifest is complete and every icon it names is a valid PNG or SVG of the declared size", async ({ request }) => {
  const m = (await (await request.get(BUNDLE_URL + "/manifest.webmanifest")).json()) as {
    id: string; name: string; short_name: string; lang: string; description: string; categories: string[]; display: string; orientation: string;
    start_url: string; scope: string; background_color: string; theme_color: string;
    icons: { src: string; sizes: string; type: string; purpose: string }[];
  };
  for (const k of ["id", "name", "short_name", "lang", "description", "display", "orientation", "start_url", "scope", "background_color", "theme_color"] as const) expect(m[k], k).toBeTruthy();
  expect(m.categories.length).toBeGreaterThan(0);
  const has = (size: string, purpose: string) => m.icons.some((i) => i.sizes === size && i.purpose === purpose && i.type === "image/png");
  expect(has("192x192", "any")).toBe(true);
  expect(has("512x512", "any")).toBe(true);
  expect(has("512x512", "maskable")).toBe(true);
  for (const icon of [...m.icons, { src: "/icons/apple-touch-icon.png", sizes: "180x180", type: "image/png", purpose: "any" }, { src: "/og-image.png", sizes: "1200x630", type: "image/png", purpose: "any" }]) {
    const res = await request.get(BUNDLE_URL + icon.src);
    expect(res.ok(), icon.src).toBe(true);
    expect(res.headers()["content-type"], icon.src).toContain(icon.type);
    const body = await res.body();
    if (icon.type === "image/png") {
      expect(body.subarray(0, 8).toString("hex"), icon.src).toBe("89504e470d0a1a0a");
      const [w, h] = icon.sizes.split("x").map(Number);
      expect([body.readUInt32BE(16), body.readUInt32BE(20)], icon.src).toEqual([w, h]);
    } else expect(body.toString("utf8")).toContain("<svg");
  }
});
