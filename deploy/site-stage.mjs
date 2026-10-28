#!/usr/bin/env node
// Stages a VERIFIED, signed OM release folder into a checkout of the public website repo (lonunjie/om.lonunjie.org)
// on a non-main branch, for a Cloudflare Pages preview. Never commits, pushes or touches main. See CUTOVER.md.
//   node deploy/site-stage.mjs <release-folder> <website-repo> [--allow-unsigned]   (--allow-unsigned: DEV bundles only, never main)
import { spawnSync } from "node:child_process";
import { cpSync, existsSync, mkdirSync, readdirSync, rmSync, copyFileSync } from "node:fs";
import { join, resolve } from "node:path";

const args = process.argv.slice(2).filter((a) => !a.startsWith("--"));
const allowUnsigned = process.argv.includes("--allow-unsigned");
const fail = (m) => { console.error(`site-stage: ${m}`); process.exit(1); };
if (args.length !== 2) fail("usage: site-stage.mjs <release-folder> <website-repo> [--allow-unsigned]");
const [rel, repo] = args.map((p) => resolve(p));
for (const p of [join(rel, "site"), join(rel, "verify.mjs"), join(rel, "wrangler.workers.toml")]) if (!existsSync(p)) fail(`not a release folder (missing ${p})`);
const git = (...a) => spawnSync("git", ["-C", repo, ...a], { encoding: "utf8" });
const branch = git("rev-parse", "--abbrev-ref", "HEAD");
if (branch.status !== 0) fail("the website path is not a git checkout");
const b = branch.stdout.trim();
if (b === "main" || b === "master" || b === "HEAD") fail(`refusing to stage on '${b}': switch to a preview branch (production deploys are the operator's)`);
if (git("status", "--porcelain").stdout.trim() !== "") fail("the website checkout has uncommitted changes; commit or stash them first");
const v = spawnSync("node", [join(rel, "verify.mjs"), rel, ...(allowUnsigned ? [] : ["--require-signed"])], { encoding: "utf8" });
if (v.status !== 0) fail(`release verification failed:\n${v.stdout}${v.stderr}`);

const clear = (d) => { if (existsSync(d)) for (const e of readdirSync(d)) rmSync(join(d, e), { recursive: true, force: true }); else mkdirSync(d, { recursive: true }); };
clear(join(repo, "public"));
cpSync(join(rel, "site"), join(repo, "public"), { recursive: true });
clear(join(repo, "release"));
cpSync(join(rel, "site"), join(repo, "release", "site"), { recursive: true });
copyFileSync(join(rel, "wrangler.workers.toml"), join(repo, "release", "wrangler.toml"));
console.log(`staged ${rel} into ${repo} on branch ${b}: public/ (Pages) and release/ (Deploy-to-Cloudflare source). Review, commit and push this branch for a preview; do not merge to main without the operator.`);
