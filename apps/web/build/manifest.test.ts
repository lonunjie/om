import { describe, expect, it } from "vitest";
import { sha256Hex } from "../src/update/hash";
import { canonicalJson, parseManifest } from "../src/update/manifest";
import { buildManifest, serializeManifest } from "./manifest";

const enc = (s: string) => new TextEncoder().encode(s);
const input = {
  version: "1.2.3",
  files: [
    { path: "sw.js", data: enc("sw") },
    { path: "index.html", data: enc("<html>") },
    { path: "assets/a.js", data: enc("a") },
    { path: "_headers", data: enc("h") },
    { path: "bundle-manifest.json", data: enc("old") },
  ],
};

describe("bundle manifest generation (UW-4)", () => {
  it("orders files stably, hashes correctly, excludes itself and _headers", async () => {
    const m = await buildManifest(input);
    expect(m.files.map((f) => f.path)).toEqual(["assets/a.js", "index.html", "sw.js"]);
    expect(m.files[1]).toEqual({ path: "index.html", sha256: await sha256Hex(enc("<html>")), bytes: 6 });
    expect(m.signature).toBeNull();
    expect(m.format).toBe("om.web-bundle/1");
  });
  it("is independent of input order and carries no timestamps", async () => {
    const a = serializeManifest(await buildManifest(input));
    const b = serializeManifest(await buildManifest({ ...input, files: input.files.toReversed() }));
    expect(a).toBe(b);
    expect(a).not.toMatch(/\d{4}-\d{2}-\d{2}|time|date/i);
    expect(parseManifest(a).version).toBe("1.2.3");
  });
  it("marks dev builds and records the commit only when known", async () => {
    expect((await buildManifest({ ...input, dev: true })).dev).toBe(true);
    expect((await buildManifest(input)).createdFrom).toEqual({});
    expect((await buildManifest({ ...input, gitCommit: "abc" })).createdFrom).toEqual({ gitCommit: "abc" });
  });
  it("parseManifest rejects malformed manifests", async () => {
    const good = JSON.parse(serializeManifest(await buildManifest(input))) as Record<string, unknown>;
    for (const bad of [
      { ...good, format: "x" },
      { ...good, version: "1.2" },
      { ...good, entry: "nope.html" },
      { ...good, files: [{ path: "../x", sha256: "0".repeat(64), bytes: 1 }] },
      { ...good, signature: { alg: "ed25519" } },
    ]) expect(() => parseManifest(JSON.stringify(bad))).toThrow();
    expect(canonicalJson({ b: 1, a: [2, { d: 1, c: 2 }] })).toBe('{"a":[2,{"c":2,"d":1}],"b":1}');
  });
});
