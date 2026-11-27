// SPDX-License-Identifier: MPL-2.0
// Runs the typed client against the real engineering module when it has been built (`pnpm --filter @om/core-wasm build:wasm:scope`);
// the module is not part of any release, so the suite skips when the file is absent.
import { existsSync, readFileSync } from "node:fs";
import type { ProfileExecutionContext } from "@om/platform-api";
import { describe, expect, it } from "vitest";
import { instantiateCore } from "./wasm";
import { createScopeEngines } from "./scope-engines";

const wasm = new URL("../dist/om_wasm_scope.wasm", import.meta.url);
const hex = (b: string, n: number) => b.repeat(n);

const room = JSON.stringify(["profile", hex("99", 32)]);
const head = { serviceId: hex("61", 32), version: 1, hash: hex("01", 32), endpoints: [{ id: hex("10", 16), state: 1 }] };
const bridgeOffer = {
  head, mapping: { incarnation: hex("05", 16), namespace: "test.net", foreignScope: "external group", direction: 3 as const, allowedClasses: [1, 2] },
  approvers: [{ kind: 1, id: hex("01", 32) }], name: "Relay", externalClaim: null,
};

describe.skipIf(!existsSync(wasm))("scope-engines typed client", () => {
  it("refuses unscoped handles, offers, and caller-supplied Room authority", async () => {
    const core = await instantiateCore(readFileSync(wasm));
    const engines = createScopeEngines(core);
    const { bridgeRoom, roomServices } = engines;
    const denied = { refused: "not_authorized" };
    expect(bridgeRoom.open(room, { quorum: 1, sequencingTimeout: 300, freshnessBound: 600 })).toEqual(denied);
    expect(bridgeRoom.propose(room, bridgeOffer)).toEqual(denied);
    expect(bridgeRoom.admit(room, ["room.add_bridge"], { ...bridgeOffer, now: 0 })).toEqual(denied);
    expect(roomServices.open(room, { quorum: null, sequencingTimeout: 300 })).toEqual(denied);
    expect(roomServices.install(room, ["room.install_service"], {
      head, incarnation: hex("05", 16), filterClasses: null, approvers: [],
    })).toEqual(denied);
    expect(engines.spaceBinding.propose("s")).toEqual(denied);
    expect(engines.lanTransfer.openReceiver("t", 10)).toEqual(denied);
  });

  it("isolates scoped handles and removes them on profile close", async () => {
    const core = await instantiateCore(readFileSync(wasm));
    core.call({ cmd: "init", home: { tier: "none", observers: ["no_third_party"], migrationAvailable: false, clientOriginChoice: ["default_origin"] } });
    const open = (profileId: string) => core.call({ cmd: "open_profile", profileId, records: [] }) as ProfileExecutionContext;
    const ctx = open("profile");
    const a = createScopeEngines(core, ctx);
    const b = createScopeEngines(core, open("other"));
    expect(a.spaceBinding.propose("s")).toEqual({ done: true });
    expect(a.spaceBinding.childAccepts("s")).toEqual({ refused: "unverified_child_state" });
    expect(a.spaceBinding.view("s")).toEqual({ official: false, spaceSide: true, childSide: false, bound: false });
    expect(a.lanTransfer.openReceiver("t", 10)).toEqual({ done: true });
    expect(a.lanTransfer.accept("t", 5, 5)).toEqual({ refused: "bad_chunk" });
    expect(b.spaceBinding.view("s")).toEqual({ refused: "unknown_handle" });
    expect(b.lanTransfer.receiverView("t")).toEqual({ refused: "unknown_handle" });

    const offer = { head, incarnation: hex("05", 16), capabilities: [], filterClasses: null, approvers: [], name: "Assistant" };
    expect(a.roomServices.propose(room, offer)).toEqual({ refused: "not_authorized" });
    expect(a.bridgeRoom.propose(room, bridgeOffer)).toEqual({ refused: "not_authorized" });
    expect(b.roomServices.list(room)).toEqual({ refused: "not_authorized" });
    expect(b.bridgeRoom.list(room)).toEqual({ refused: "not_authorized" });

    core.call({ cmd: "close_profile", profileId: ctx.profileId });
    const reopened = createScopeEngines(core, open("profile"));
    expect(() => a.spaceBinding.view("s")).toThrow();
    expect(reopened.spaceBinding.view("s")).toEqual({ refused: "unknown_handle" });
    expect(reopened.lanTransfer.receiverView("t")).toEqual({ refused: "unknown_handle" });
  });
});
