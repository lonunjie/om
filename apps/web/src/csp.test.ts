import { readFileSync } from "node:fs";
import { describe, expect, it } from "vitest";
import { CSP, SECURITY_HEADERS, cspHeader, headersFile } from "../csp.ts";

describe("production CSP (UW-0)", () => {
  it("never allows inline or eval script, remote script, or a base/form escape", () => {
    expect(CSP["script-src"]).not.toMatch(/unsafe-inline|(?<!wasm-)unsafe-eval|https?:|\*/);
    expect(CSP["default-src"]).toBe("'none'");
    expect(CSP["base-uri"]).toBe("'none'");
    expect(CSP["form-action"]).toBe("'none'");
    expect(CSP["object-src"]).toBe("'none'");
    expect(CSP["frame-ancestors"]).toBe("'none'");
  });
  it("requires Trusted Types for script sinks", () => {
    expect(CSP["require-trusted-types-for"]).toBe("'script'");
    expect(cspHeader()).toContain("trusted-types om-bundle");
  });
  it("the Pages _headers file carries the same policy", () => {
    expect(headersFile()).toContain(SECURITY_HEADERS["Content-Security-Policy"]);
  });
  it("index.html has no inline script or remote subresource", () => {
    const html = readFileSync(new URL("../index.html", import.meta.url), "utf8");
    expect(html).not.toMatch(/<script(?![^>]*\bsrc=)[^>]*>\s*\S/);
    expect(html).not.toMatch(/(?:src|href)=["']?https?:/);
    expect(html).not.toMatch(/\son[a-z]+=/i);
  });
});
