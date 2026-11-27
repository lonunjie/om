// SPDX-License-Identifier: MPL-2.0
// Room control observation through the typed client on the real engineering module (ADR-0269). The module is not part
// of any release, so the suite skips when it has not been built.
import { existsSync } from "node:fs";
import { describe, expect, it } from "vitest";
import { SCOPE_WASM, connectPair } from "./test-support-lan-signal";

describe.skipIf(!existsSync(SCOPE_WASM))("room control observation", () => {
  it("shows nothing before anything is observed and refuses what it cannot verify", async () => {
    const { alice } = await connectPair();
    expect(alice.engines.roomControl.view(alice.ctx, "room-1")).toEqual({ entries: [] });
    // Not hex, then hex that is not an envelope: both are refused and nothing is kept.
    expect(alice.engines.roomControl.observe(alice.ctx, "room-1", "service", "zz")).toEqual({ refused: "malformed" });
    expect(alice.engines.roomControl.observe(alice.ctx, "room-1", "service", "ff")).toEqual({ refused: "malformed" });
    expect(alice.engines.roomControl.observe(alice.ctx, "room-1", "bridge", "a0")).toEqual({ refused: "malformed" });
    expect(alice.engines.roomControl.view(alice.ctx, "room-1")).toEqual({ entries: [] });
  });
});
