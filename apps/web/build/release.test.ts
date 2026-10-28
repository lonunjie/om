import { spawnSync } from "node:child_process";
import { cpSync, existsSync, mkdtempSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { afterAll, describe, expect, it } from "vitest";
import { verifyRelease } from "./verify-release.mjs";

const webRoot = new URL("..", import.meta.url).pathname;
const tmp = mkdtempSync(join(tmpdir(), "om-release-test-"));
afterAll(() => {
  rmSync(tmp, { recursive: true, force: true });
});
const run = (...a: string[]) => spawnSync("node", ["build/release.mjs", ...a], { cwd: webRoot, encoding: "utf8" });

describe("release script", () => {
  it("refuses an unsigned production release and says why", () => {
    const r = run("--out", tmp);
    expect(r.status).toBe(1);
    expect(r.stderr).toMatch(/UNSIGNED/);
    expect(r.stderr).toMatch(/--dev/);
  });
  it("builds a verifiable dev release; tampering and --require-signed fail", { timeout: 120_000 }, () => {
    const r = run("--dev", "--no-zip", "--out", tmp);
    expect(r.status, r.stderr).toBe(0);
    const dir = join(tmp, "om-web-0.0.0-dev");
    expect(existsSync(join(dir, "SHA256SUMS"))).toBe(true);
    expect(verifyRelease(dir).ok).toBe(true);
    expect(verifyRelease(dir, { requireSigned: true }).ok).toBe(false);
    const cli = spawnSync("node", [join(dir, "verify.mjs"), dir], { encoding: "utf8" });
    expect(cli.status).toBe(0);
    expect(cli.stdout).toMatch(/DEV build/);
    const bad = join(tmp, "tampered");
    cpSync(dir, bad, { recursive: true });
    writeFileSync(join(bad, "site", "robots.txt"), "tampered");
    const v = verifyRelease(bad);
    expect(v.ok).toBe(false);
    expect(v.errors.join("\n")).toMatch(/mismatch/);
  });
});
