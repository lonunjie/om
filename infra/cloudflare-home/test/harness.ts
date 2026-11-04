// SPDX-License-Identifier: MPL-2.0
// Boots the real Worker bundle (the same `wrangler deploy --dry-run` output a deploy would upload) in Miniflare/workerd with
// SQLite Durable Objects. No Cloudflare account, token or network is involved.
import { execFileSync } from "node:child_process";
import { mkdtempSync, readFileSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { Miniflare } from "miniflare";

const ROOT = new URL("..", import.meta.url).pathname;
let bundled: string | undefined;

/** `OM_HOME_BUNDLE=<path to a released relay/worker.js>` runs the whole suite against that artifact instead of a fresh build of the source. */
function bundle(): string {
  if (process.env["OM_HOME_BUNDLE"]) return process.env["OM_HOME_BUNDLE"];
  if (bundled) return bundled;
  const out = mkdtempSync(join(tmpdir(), "om-home-bundle-"));
  execFileSync("pnpm", ["exec", "wrangler", "deploy", "--dry-run", "--config", "wrangler.jsonc", "--outdir", out], { cwd: ROOT, stdio: "pipe", env: { ...process.env, WRANGLER_SEND_METRICS: "false", WRANGLER_LOG_PATH: join(out, "logs"), CI: "1" } });
  bundled = join(out, "index.js");
  return bundled;
}

export interface Home {
  readonly url: string;
  readonly mf: Miniflare;
  fetch(path: string, init?: RequestInit): Promise<Response>;
  dispose(): Promise<void>;
}

export async function startHome(opts: { persist?: string; port?: number; vars?: Record<string, string>; bundle?: string; blobs?: false } = {}): Promise<Home> {
  const code = readFileSync(opts.bundle ?? bundle(), "utf8");
  const mf = new Miniflare({
    modules: [{ type: "ESModule", path: "index.js", contents: code }],
    compatibilityDate: "2026-06-01",
    host: "127.0.0.1",
    port: opts.port ?? 0,
    bindings: { OM_PROVIDER_ID: "om-test-home", OM_HARD_FREE_TIER: "1", ...(opts.vars ?? {}) },
    durableObjects: {
      QUEUES: { className: "MailboxQueue", useSQLite: true },
      KEYS: { className: "KeyPool", useSQLite: true },
      DESCRIPTORS: { className: "DescriptorStore", useSQLite: true },
      INTRO: { className: "IntroGate", useSQLite: true },
      // A bundle released before the blob spool (OM_HOME_BUNDLE=<older worker.js>) has no such class; the Home then runs without it.
      ...(opts.blobs !== false && /\bBlobSpool = class\b|\bclass BlobSpool\b/.test(code) ? { BLOBS: { className: "BlobSpool", useSQLite: true } } : {}),
    },
    ...(opts.persist ? { durableObjectsPersist: opts.persist } : {}),
  });
  const url = (await mf.ready).toString().replace(/\/$/, "");
  return {
    url, mf,
    fetch: (path, init) => fetch(url + path, init),
    dispose: () => mf.dispose(),
  };
}

export const scratch = (): { dir: string; remove(): void } => {
  const dir = mkdtempSync(join(tmpdir(), "om-home-state-"));
  return { dir, remove: () => { rmSync(dir, { recursive: true, force: true }); } };
};
