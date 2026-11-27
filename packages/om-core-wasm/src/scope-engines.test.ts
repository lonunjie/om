// SPDX-License-Identifier: MPL-2.0
// Runs the typed client against the real engineering module when it has been built (`pnpm --filter @om/core-wasm build:wasm:scope`);
// the module is not part of any release, so the suite skips when the file is absent.
import { existsSync, readFileSync } from "node:fs";
import { describe, expect, it } from "vitest";
import { instantiateCore } from "./wasm";
import { createScopeEngines } from "./scope-engines";

const wasm = new URL("../dist/om_wasm_scope.wasm", import.meta.url);
const hex = (b: string, n: number) => b.repeat(n);

describe.skipIf(!existsSync(wasm))("scope-engines typed client", () => {
  it("keeps bridge, room and link state under handles", async () => {
    const e = createScopeEngines(await instantiateCore(readFileSync(wasm)));
    const head = { serviceId: hex("61", 32), version: 1, hash: hex("01", 32), endpoints: [{ id: hex("10", 16), state: 1 }] };
    expect(e.bridgeRoom.open("b", { quorum: 2, sequencingTimeout: 300, freshnessBound: 600 })).toEqual({ done: true });
    const mapping = { incarnation: hex("05", 16), namespace: "test.net", direction: 3 as const, allowedClasses: [1, 2] };
    const mixed = [{ kind: 1, id: hex("01", 32) }, { kind: 3, id: hex("61", 32) }];
    expect(e.bridgeRoom.admit("b", ["room.add_bridge"], { head, mapping, approvers: mixed, now: 0 })).toEqual({ refused: "quorum_not_met" });
    const humans = [{ kind: 1, id: hex("01", 32) }, { kind: 1, id: hex("02", 32) }];
    expect(e.bridgeRoom.admit("b", ["room.add_bridge"], { head, mapping, approvers: humans, now: 0 })).toEqual({ done: true });
    expect(e.bridgeRoom.mayExport("b", { bridge: head.serviceId, endpoint: hex("10", 16), class: 9, now: 1 })).toMatchObject({ allowed: false, blocked: "class_not_allowed" });
    expect(e.bridgeRoom.importAuthorOk("b", 1)).toBe(false);
    const listed = e.bridgeRoom.list("b");
    expect(listed).toMatchObject({ bridges: [{ bridge: head.serviceId, installed: true, mapping: { namespace: "test.net", direction: 3, allowedClasses: [1, 2], active: true } }] });
    expect(e.bridgeRoom.view("b", head.serviceId)).toMatchObject({ installed: true, mapping: { namespace: "test.net", active: true } });
    expect(e.bridgeRoom.view("b", hex("77", 32))).toMatchObject({ installed: false, mapping: null });
    expect(e.spaceBinding.propose("s")).toEqual({ done: true });
    expect(e.spaceBinding.createBinding("s", { spaceId: hex("aa", 32), governance: humans, capabilities: ["room.remove_member"], freshnessBound: 60, acceptsSubdelegation: false })).toEqual({ refused: "link_not_official" });
    expect(e.lanTransfer.openReceiver("t", 10)).toEqual({ done: true });
    expect(e.lanTransfer.accept("t", 5, 5)).toEqual({ refused: "bad_chunk" });
  });
});
