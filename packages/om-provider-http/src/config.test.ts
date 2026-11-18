import { describe, expect, it } from "vitest";
import { checkProviderUrl } from "./config";

describe("checkProviderUrl loopback", () => {
  it("allows plain http to this machine only when the test flag is on, including names under .localhost", () => {
    for (const u of ["http://127.0.0.1:8789", "http://localhost:8789", "http://home.localhost:8789"]) {
      expect(checkProviderUrl(u, { allowLoopbackHttp: true }).ok).toBe(true);
      expect(checkProviderUrl(u).ok).toBe(false);
    }
  });
  it("never allows plain http to another name", () => {
    for (const u of ["http://example.com", "http://notlocalhost.example", "http://localhost.example.com"]) expect(checkProviderUrl(u, { allowLoopbackHttp: true }).ok).toBe(false);
  });
});
