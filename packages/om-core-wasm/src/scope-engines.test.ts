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
  it("holds a proposed bridge as an offer bound to its review target", async () => {
    const e = createScopeEngines(await instantiateCore(readFileSync(wasm)));
    const head = { serviceId: hex("62", 32), version: 1, hash: hex("01", 32), endpoints: [{ id: hex("10", 16), state: 1 }] };
    const humans = [{ kind: 1, id: hex("01", 32) }, { kind: 1, id: hex("02", 32) }];
    const mapping = { incarnation: hex("05", 16), namespace: "test.net", direction: 3 as const, allowedClasses: [1, 2] };
    e.bridgeRoom.open("o", { quorum: 2, sequencingTimeout: 300, freshnessBound: 600 });
    const p = e.bridgeRoom.propose("o", { head, mapping, approvers: humans, name: "Relay\u202E", externalClaim: null }) as { target: string };
    expect(p.target).toMatch(/^[0-9a-f]{64}$/);
    expect(e.bridgeRoom.list("o")).toMatchObject({ bridges: [{ bridge: head.serviceId, membership: "proposed", target: p.target, name: "Relay", installed: false }] });
    expect(e.bridgeRoom.admitOffer("o", ["room.add_bridge"], { bridge: head.serviceId, target: hex("00", 32), now: 0 })).toEqual({ refused: "stale_target" });
    expect(e.bridgeRoom.admitOffer("o", ["room.add_bridge"], { bridge: head.serviceId, target: p.target, now: 0 })).toEqual({ done: true });
    expect(e.bridgeRoom.list("o")).toMatchObject({ bridges: [{ membership: "member", target: p.target, installed: true }] });
    expect(e.bridgeRoom.removeMember("o", ["room.add_bridge"], { bridge: head.serviceId, target: p.target })).toMatchObject({ removedEndpoints: 1 });
    expect(e.bridgeRoom.list("o")).toMatchObject({ bridges: [{ membership: "removed" }] });
  });
  it("holds a Room service offer until the exact reviewed target is installed", async () => {
    const { roomServices: services } = createScopeEngines(await instantiateCore(readFileSync(wasm)));
    const head = { serviceId: hex("63", 32), version: 1, hash: hex("01", 32), endpoints: [{ id: hex("10", 16), state: 1 }] };
    const offer = { head, incarnation: hex("05", 16), filterClasses: [1, 2], capabilities: ["message.create"],
      name: "Assistant\u202e", approvers: [{ kind: 1, id: hex("01", 32) }] };
    expect(services.open("service-room", { quorum: null, sequencingTimeout: 300 })).toEqual({ done: true });
    const first = services.propose("service-room", offer);
    if ("refused" in first) throw new Error(first.refused);
    expect(first.target).toMatch(/^[0-9a-f]{64}$/);
    expect(services.view("service-room", head.serviceId)).toEqual({ installed: false });
    expect(services.list("service-room")).toMatchObject({ services: [{
      service: head.serviceId, membership: "proposed", target: first.target, name: "Assistant",
      capabilities: ["message.create"], filterClasses: [1, 2], admissionBasis: head, approversReportedAtProposal: 1,
    }] });
    const second = services.propose("service-room", { ...offer, filterClasses: [2] });
    if ("refused" in second) throw new Error(second.refused);
    const cap = ["room.install_service"];
    expect(services.installOffer("service-room", cap, { service: head.serviceId, target: first.target })).toEqual({ refused: "stale_target" });
    const reviewed = { service: head.serviceId, target: second.target };
    expect(services.installOffer("service-room", [], reviewed)).toEqual({ refused: "not_authorized" });
    expect(services.installOffer("service-room", cap, reviewed)).toEqual({ done: true });
    expect(services.list("service-room")).toMatchObject({ services: [{
      membership: "installed", target: second.target, installed: true, canReadAllContent: true, filterClasses: [2],
    }] });
    expect(services.removeMember("service-room", cap, { ...reviewed, target: first.target })).toEqual({ refused: "stale_target" });
    expect(services.removeMember("service-room", cap, reviewed)).toEqual({ removedEndpoints: 1 });
    expect(services.list("service-room")).toMatchObject({ services: [{ membership: "removed", target: second.target }] });
    expect(services.withdrawOffer("service-room", head.serviceId)).toEqual({ done: false });
  });
  it("refuses an unsigned admission basis in an initialized Core", async () => {
    const core = await instantiateCore(readFileSync(wasm));
    const { roomServices: services } = createScopeEngines(core);
    const room = JSON.stringify(["profile", "room"]);
    const head = { serviceId: hex("63", 32), version: 1, hash: hex("01", 32), endpoints: [{ id: hex("10", 16), state: 1 }] };
    const offer = { head, incarnation: hex("05", 16), capabilities: [], filterClasses: null, approvers: [], name: "Assistant" };
    services.open(room, { quorum: null, sequencingTimeout: 300 });
    const proposed = services.propose(room, offer);
    if ("refused" in proposed) throw new Error(proposed.refused);

    core.call({ cmd: "init", home: { tier: "none", observers: ["no_third_party"], migrationAvailable: false, clientOriginChoice: ["default_origin"] } });
    const ctx = core.call({ cmd: "open_profile", profileId: "profile", records: [] }) as ProfileExecutionContext;
    const reviewed = { service: head.serviceId, target: proposed.target };
    expect(services.installOffer(room, ["room.install_service"], reviewed)).toEqual({ refused: "unverified_head" });
    expect(services.installOffer(room, ["room.install_service"], reviewed, ctx)).toEqual({ refused: "unverified_head" });
    expect(services.propose(room, offer, ctx)).toEqual({ refused: "unverified_head" });
    expect(services.list(room)).toMatchObject({ services: [{ membership: "proposed", installed: false }] });
  });
});
