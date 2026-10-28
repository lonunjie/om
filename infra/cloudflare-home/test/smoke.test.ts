// SPDX-License-Identifier: MPL-2.0
import { afterAll, beforeAll, describe, expect, it } from "vitest";
import { startHome, type Home } from "./harness";

let home: Home;
beforeAll(async () => { home = await startHome(); }, 120_000);
afterAll(async () => { await home.dispose(); });

describe("smoke", () => {
  it("serves the capabilities document", async () => {
    const r = await home.fetch("/.well-known/om-provider-capabilities");
    expect(r.status).toBe(200);
    const doc = (await r.json()) as { provider_id: string };
    expect(doc.provider_id).toBe("om-test-home");
  });
});
