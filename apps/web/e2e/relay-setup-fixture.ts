import type { BrowserContext, Request } from "@playwright/test";
import { E2E_ADMISSION_HASH, LOCKED_RELAY } from "./admission-fixture.ts";

export const SETUP_APP = "http://localhost:4179";
const CLOUDFLARE = "https://deploy.example.test";
export const DEPLOY = `${CLOUDFLARE}/start`;
const GITHUB = "https://github.example.test";
const GITLAB = "https://gitlab.example.test";
export const CLOUDFLARE_SIGNUP = `${CLOUDFLARE}/signup`;
export const GITHUB_SIGNUP = `${GITHUB}/signup`;
export const GITLAB_SIGNUP = `${GITLAB}/signup`;

interface RehearsalOptions {
  readonly gitLabOffered?: boolean;
  readonly failures?: boolean;
}

const html = (heading: string, body: string): string =>
  `<!doctype html><html lang="en"><meta charset="utf-8"><title>${heading}</title><link rel="icon" href="data:,"><main><h1>${heading}</h1>${body}</main></html>`;
const link = (url: string, label: string): string => `<p><a href="${url}">${label}</a></p>`;
const submit = (url: string, label: string, fields = ""): string =>
  `<form method="post" action="${url}">${fields}<button>${label}</button></form>`;

/**
 * Synthetic page states for the walkthrough. The labels, permissions, alternate forge and outcomes are test inputs, not
 * observations of either vendor. The browser can reach the local app and the local locked relay after a fixture deploys it.
 */
export async function rehearseSetup(context: BrowserContext, options: RehearsalOptions = {}) {
  let cloudflareAccount = false;
  let githubAccount = false;
  let gitlabAccount = false;
  let hasRepository = false;
  let access: "none" | "selected" | "all" | "gitlab" = "none";
  let deployed = false;
  let validDeploys = 0;
  const unexpected: string[] = [];

  const deployForm = () => {
    const label = access === "gitlab" ? "OM setup value" : "OM_ADMISSION";
    return html("Configure deployment", submit(`${CLOUDFLARE}/deploy`, "Deploy",
      `<label>${label} <input required name="setup-value"></label>`));
  };
  const tryAgain = link(DEPLOY, "Back to deployment");
  const connectGitHub = link(`${GITHUB}/access`, "Connect GitHub");
  const connectGitLab = options.gitLabOffered ? link(`${GITLAB}/access`, "Connect GitLab") : "";

  const pages = new Map<string, (request: Request) => string>([
    [`GET ${CLOUDFLARE_SIGNUP}`, () => html("Cloudflare sign-up", submit(CLOUDFLARE_SIGNUP, "Create free account"))],
    [`POST ${CLOUDFLARE_SIGNUP}`, () => {
      cloudflareAccount = true;
      return html("Cloudflare dashboard", "<p>Signed in to a test account.</p>");
    }],
    [`GET ${GITHUB_SIGNUP}`, () => html("GitHub sign-up", submit(GITHUB_SIGNUP, "Create account"))],
    [`POST ${GITHUB_SIGNUP}`, () => {
      githubAccount = true;
      return html("GitHub dashboard", submit(`${GITHUB}/repositories`, "Create empty repository"));
    }],
    [`POST ${GITHUB}/repositories`, () => {
      hasRepository = true;
      return html("Empty repository created", "<p>setup-test</p>");
    }],
    [`GET ${GITLAB_SIGNUP}`, () => html("GitLab sign-up", submit(GITLAB_SIGNUP, "Create account"))],
    [`POST ${GITLAB_SIGNUP}`, () => {
      gitlabAccount = true;
      return html("GitLab dashboard", "<p>Signed in to a test account.</p>");
    }],
    [`GET ${DEPLOY}`, () => {
      if (!cloudflareAccount) return html("Sign in to Cloudflare", link(CLOUDFLARE_SIGNUP, "Create account"));
      if (access === "selected") return html("Repository access needs attention", link(`${GITHUB}/access`, "Review GitHub access") + connectGitLab);
      if (access === "all" || access === "gitlab") return deployForm();
      return html("Connect a source account", connectGitHub + connectGitLab);
    }],
    [`GET ${GITHUB}/access`, () => {
      if (!githubAccount) return html("Sign in to GitHub", link(GITHUB_SIGNUP, "Create account"));
      const selected = hasRepository ? submit(`${GITHUB}/selected`, "Allow selected repositories",
        '<label><input type="checkbox" name="repository" value="setup-test" required> setup-test</label>') : "<p>No existing repositories.</p>";
      return html("Choose GitHub repository access", selected + submit(`${GITHUB}/all`, "Allow all repositories"));
    }],
    [`POST ${GITHUB}/selected`, (request) => {
      if (new URLSearchParams(request.postData() ?? "").get("repository") !== "setup-test") return html("Choose a repository", connectGitHub);
      access = "selected";
      return html("Selected repository access saved", "<p>Return to the setup tab to continue.</p>");
    }],
    [`POST ${GITHUB}/all`, () => {
      access = "all";
      return html("GitHub access saved", link(DEPLOY, "Return to Cloudflare"));
    }],
    [`GET ${GITLAB}/access`, () => gitlabAccount
      ? html("Connect your GitLab account", submit(`${GITLAB}/access`, "Allow setup"))
      : html("Sign in to GitLab", link(GITLAB_SIGNUP, "Create account"))],
    [`POST ${GITLAB}/access`, () => {
      access = "gitlab";
      return html("GitLab access saved", link(DEPLOY, "Return to Cloudflare"));
    }],
    [`POST ${CLOUDFLARE}/deploy`, (request) => {
      const value = new URLSearchParams(request.postData() ?? "").get("setup-value");
      if (value !== E2E_ADMISSION_HASH) return html("Setup value rejected", tryAgain);
      validDeploys++;
      if (options.failures && validDeploys === 1) return html("Free plan limit reached", tryAgain);
      if (options.failures && validDeploys === 2) return html("Deployment failed", tryAgain);
      deployed = true;
      return html("Deployment complete", `<p>Address of your delivery service</p><output>${LOCKED_RELAY}</output>`);
    }],
  ]);

  await context.route("**/*", async (route) => {
    const request = route.request();
    const url = new URL(request.url());
    if (url.origin === SETUP_APP || (deployed && url.origin === LOCKED_RELAY)) {
      await route.continue();
      return;
    }
    const state = pages.get(`${request.method()} ${url.href}`);
    if (state !== undefined) {
      await route.fulfill({ contentType: "text/html; charset=utf-8", body: state(request) });
      return;
    }
    unexpected.push(`${request.method()} ${url.origin}${url.pathname}`);
    await route.abort();
  });
  await context.routeWebSocket((url) => !deployed || url.origin !== LOCKED_RELAY.replace(/^http:/, "ws:"), async (route) => {
    unexpected.push(`WebSocket ${new URL(route.url()).origin}`);
    await route.close();
  });
  return { unexpected };
}
