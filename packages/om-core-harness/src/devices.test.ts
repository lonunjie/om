import { describe, expect, it } from "vitest";
import { asDeviceId, asOperationId, asProfileId } from "@om/platform-api";
import type { ConfirmationToken, CoreIntent, OperationId, OutcomeClass, ProfileExecutionContext } from "@om/platform-api";
import { createHarness, type Harness, type SeedDevices } from "./harness";

const OLD = asProfileId("existing");
const NEW = asProfileId("new-device");
const token = Object.freeze({}) as ConfirmationToken; // the harness is not the confirmation authority; the registry is
let n = 0;
const op = (): OperationId => asOperationId(`dop-${String(++n)}`);

const ACCOUNT: SeedDevices = {
  devices: [
    { id: "d-this", label: "This laptop", role: "control", current: true },
    { id: "d-phone", label: "Phone", role: "control" },
    { id: "d-tablet", label: "Tablet", role: "messaging", lastSeen: "long_ago", historySync: false },
  ],
  recovery: { custodians: ["synced_passkey", "recovery_key"], activationDelayMs: 1000, incoming: [{ requestId: "rr-in", scope: "identity", custodian: "recovery_key" }] },
};

function setup(account: SeedDevices = ACCOUNT, clock = (): number => 5000) {
  const h = createHarness({ clock });
  const old = h.seedProfile({ profileId: OLD, label: "Me", account });
  const fresh = h.seedProfile({ profileId: NEW, label: "New" });
  return { h, old, fresh };
}
async function run(h: Harness, intent: CoreIntent): Promise<OutcomeClass> {
  const handle = await h.dispatch(intent);
  const seen: OutcomeClass[] = [];
  h.observe(intent.ctx, handle.operationId, (o) => { seen.push(o); })();
  const last = seen.at(-1);
  if (!last) throw new Error("no outcome");
  return last;
}
const device = async (h: Harness, ctx: ProfileExecutionContext, id: string) => (await h.query({ kind: "GetDevices", ctx })).devices.find((d) => d.id === id);

describe("devices via harness (DEV ONLY)", () => {
  it("reports Core-decided actions and cautions: two controllers cannot suspend (would freeze), messaging devices are only removable", async () => {
    const { h, old } = setup();
    const v = await h.query({ kind: "GetDevices", ctx: old });
    expect(v.controlState).toBe("normal");
    expect(v.devices.find((d) => d.isCurrent)).toMatchObject({ id: "d-this", actions: [] });
    expect(v.devices.find((d) => d.id === "d-phone")).toMatchObject({ actions: ["revoke"], cautions: ["suspend_would_freeze"] });
    expect(v.devices.find((d) => d.id === "d-tablet")).toMatchObject({ actions: ["revoke"], cautions: ["long_idle", "history_sync_unavailable"] });
    expect(await run(h, { kind: "SuspendDevice", ctx: old, operationId: op(), deviceId: asDeviceId("d-phone"), confirmation: token })).toEqual({ kind: "Rejected", reason: "policy_denied" });
  });

  it("suspend is quarantine; revoking a controller stays pending until authorized; a messaging device is removed at once", async () => {
    const { h, old } = setup({ devices: [...(ACCOUNT.devices ?? []), { id: "d-desk", label: "Desktop", role: "control" }] });
    expect((await device(h, old, "d-phone"))?.actions).toEqual(["suspend", "revoke"]);
    expect(await run(h, { kind: "SuspendDevice", ctx: old, operationId: op(), deviceId: asDeviceId("d-phone"), confirmation: token })).toEqual({ kind: "Committed" });
    expect((await device(h, old, "d-phone"))?.state).toBe("quarantined");
    const revokeOp = op();
    expect(await run(h, { kind: "RevokeDevice", ctx: old, operationId: revokeOp, deviceId: asDeviceId("d-phone"), confirmation: token })).toEqual({ kind: "PendingAuthorization" });
    expect((await device(h, old, "d-phone"))?.state).toBe("revocation_pending");
    h.resolveAuthorization(revokeOp, { kind: "Committed" });
    expect((await device(h, old, "d-phone"))?.state).toBe("revoked");
    expect(await run(h, { kind: "RevokeDevice", ctx: old, operationId: op(), deviceId: asDeviceId("d-tablet"), confirmation: token })).toEqual({ kind: "Committed" });
    expect((await device(h, old, "d-tablet"))?.state).toBe("revoked");
  });

  it("Safety Freeze: Core offers no device changes", async () => {
    const { h, old } = setup({ ...ACCOUNT, controlState: "safety_freeze" });
    const v = await h.query({ kind: "GetDevices", ctx: old });
    expect(v.controlState).toBe("safety_freeze");
    expect(v.devices.flatMap((d) => d.actions)).toEqual([]);
  });
});

describe("device linking via harness: NoSilentLink (DEV ONLY)", () => {
  it("offer -> open on existing device -> same code on both -> explicit approval -> enrollment notice", async () => {
    const { h, old, fresh } = setup();
    const offerOp = op();
    expect(await run(h, { kind: "OfferDeviceLink", ctx: fresh, operationId: offerOp })).toEqual({ kind: "Committed" });
    const offer = await h.query({ kind: "GetLinkOffer", ctx: fresh, operationId: offerOp });
    expect(offer).toMatchObject({ state: "waiting_for_scan", verificationCode: null });
    if (!offer) throw new Error("no offer");
    expect(await h.query({ kind: "GetLinkOffer", ctx: old, operationId: offerOp })).toBeNull(); // not the other profile's offer

    const reqOp = op();
    expect(await run(h, { kind: "RequestDeviceLink", ctx: old, operationId: reqOp, link: offer.link })).toEqual({ kind: "Committed" });
    const request = await h.query({ kind: "GetLinkRequest", ctx: old, operationId: reqOp });
    const shown = await h.query({ kind: "GetLinkOffer", ctx: fresh, operationId: offerOp });
    expect(request?.state).toBe("awaiting_approval");
    expect(shown?.verificationCode).toMatch(/^\d{3} \d{3}$/);
    expect(request?.verificationCode).toBe(shown?.verificationCode);
    // Opening the offer linked nothing.
    expect((await h.query({ kind: "GetDevices", ctx: old })).devices).toHaveLength(3);
    if (!request) throw new Error("no request");

    expect(await run(h, { kind: "ApproveDeviceLink", ctx: old, operationId: op(), requestId: request.requestId, confirmation: token })).toEqual({ kind: "Committed" });
    const after = await h.query({ kind: "GetDevices", ctx: old });
    expect(after.devices).toHaveLength(4);
    expect(after.notices).toEqual([expect.objectContaining({ code: "device_linked", label: "New web browser" })]);
    expect((await h.query({ kind: "GetLinkOffer", ctx: fresh, operationId: offerOp }))?.state).toBe("linked");
    // An offer is single use.
    expect(await run(h, { kind: "RequestDeviceLink", ctx: old, operationId: op(), link: offer.link })).toEqual({ kind: "Rejected", reason: "conflict" });
  });

  it("rejecting links nothing and tells the new device", async () => {
    const { h, old } = setup();
    const offer = h.simulateNewDeviceOffer("Stranger's laptop");
    const reqOp = op();
    await run(h, { kind: "RequestDeviceLink", ctx: old, operationId: reqOp, link: offer.link });
    const request = await h.query({ kind: "GetLinkRequest", ctx: old, operationId: reqOp });
    if (!request) throw new Error("no request");
    expect(request).toMatchObject({ deviceLabel: "Stranger's laptop", verificationCode: offer.verificationCode });
    expect(await run(h, { kind: "RejectDeviceLink", ctx: old, operationId: op(), requestId: request.requestId })).toEqual({ kind: "Committed" });
    const v = await h.query({ kind: "GetDevices", ctx: old });
    expect(v.devices).toHaveLength(3);
    expect(v.notices).toEqual([]);
    expect(await run(h, { kind: "ApproveDeviceLink", ctx: old, operationId: op(), requestId: request.requestId, confirmation: token })).toEqual({ kind: "Rejected", reason: "conflict" });
  });

  it("uncertain network: the approval stays PendingReconciliation and links only once reconciled", async () => {
    const { h, old } = setup();
    const reqOp = op();
    await run(h, { kind: "RequestDeviceLink", ctx: old, operationId: reqOp, link: h.simulateNewDeviceOffer("Laptop").link });
    const request = await h.query({ kind: "GetLinkRequest", ctx: old, operationId: reqOp });
    if (!request) throw new Error("no request");
    const approveOp = op();
    h.injectNetworkTimeoutAfterSend(approveOp);
    const approve: CoreIntent = { kind: "ApproveDeviceLink", ctx: old, operationId: approveOp, requestId: request.requestId, confirmation: token };
    expect(await run(h, approve)).toEqual({ kind: "PendingReconciliation" });
    h.simulateRestart();
    expect((await h.pendingOperations(old)).find((o) => o.operationId === approveOp)?.outcome).toEqual({ kind: "PendingReconciliation" });
    expect((await h.dispatch(approve)).operationId).toBe(approveOp); // same logical operation, no second one
    expect((await h.query({ kind: "GetDevices", ctx: old })).devices).toHaveLength(3);
    h.resolveReconciliation(approveOp, { kind: "Committed" });
    expect((await h.query({ kind: "GetDevices", ctx: old })).notices).toHaveLength(1);
  });

  it("Hardened: approval is PendingAuthorization; NeedsUserAction changes nothing; expired and malformed offers are refused", async () => {
    let now = 5000;
    const { h, old } = setup({ ...ACCOUNT, linkNeedsAuthorization: true }, () => now);
    const reqOp = op();
    await run(h, { kind: "RequestDeviceLink", ctx: old, operationId: reqOp, link: h.simulateNewDeviceOffer("Laptop").link });
    const request = await h.query({ kind: "GetLinkRequest", ctx: old, operationId: reqOp });
    if (!request) throw new Error("no request");
    const askOp = op();
    h.injectUserActionRequired(askOp, "reauthenticate");
    expect(await run(h, { kind: "ApproveDeviceLink", ctx: old, operationId: askOp, requestId: request.requestId, confirmation: token })).toEqual({ kind: "NeedsUserAction", action: "reauthenticate" });
    const approveOp = op();
    expect(await run(h, { kind: "ApproveDeviceLink", ctx: old, operationId: approveOp, requestId: request.requestId, confirmation: token })).toEqual({ kind: "PendingAuthorization" });
    expect((await h.query({ kind: "GetLinkRequest", ctx: old, operationId: reqOp }))?.state).toBe("pending_authorization");
    h.resolveAuthorization(approveOp, { kind: "Committed" });
    expect((await h.query({ kind: "GetDevices", ctx: old })).devices).toHaveLength(4);

    const late = h.simulateNewDeviceOffer("Late");
    now += 11 * 60 * 1000;
    expect(await run(h, { kind: "RequestDeviceLink", ctx: old, operationId: op(), link: late.link })).toEqual({ kind: "Rejected", reason: "expired" });
    expect(await run(h, { kind: "RequestDeviceLink", ctx: old, operationId: op(), link: "om:invite/AAAAAAAAAAAAAAAAAAAAAA" })).toEqual({ kind: "Rejected", reason: "invalid" });
  });

  it("a messaging-only device cannot link another device", async () => {
    const { h, old } = setup({ devices: [{ id: "d-this", label: "Tablet", role: "messaging", current: true }, { id: "d-phone", label: "Phone", role: "control" }] });
    expect(await run(h, { kind: "RequestDeviceLink", ctx: old, operationId: op(), link: h.simulateNewDeviceOffer("X").link })).toEqual({ kind: "Rejected", reason: "unauthorized" });
  });
});

describe("recovery via harness (DEV ONLY)", () => {
  it("reports status with Core-supplied timers; scopes stay distinct", async () => {
    const { h, old } = setup();
    const s = await h.query({ kind: "GetRecoveryStatus", ctx: old });
    expect(s).toMatchObject({ policy: "on", activationDelayMs: 1000, recoveryKeySaved: true, scopes: ["identity", "history", "provider"] });
    expect(s.custodians).toEqual([{ code: "synced_passkey", distributionBound: true }, { code: "recovery_key", distributionBound: false }]);
    expect(s.requests).toEqual([expect.objectContaining({ requestId: "rr-in", direction: "incoming", activatesAt: 6000, state: "waiting", canStop: true })]);

    const idOp = op();
    expect(await run(h, { kind: "StartRecovery", ctx: old, operationId: idOp, scope: "identity", custodian: "recovery_key", confirmation: token })).toEqual({ kind: "PendingAuthorization" });
    expect(await run(h, { kind: "StartRecovery", ctx: old, operationId: op(), scope: "history", custodian: "recovery_key", confirmation: token })).toEqual({ kind: "Committed" });
    expect(await run(h, { kind: "StartRecovery", ctx: old, operationId: op(), scope: "identity", custodian: "recovery_key", confirmation: token })).toEqual({ kind: "Rejected", reason: "conflict" });
    expect(await run(h, { kind: "StartRecovery", ctx: old, operationId: op(), scope: "provider", custodian: "trusted_contacts", confirmation: token })).toEqual({ kind: "Rejected", reason: "not_found" });
    const mine = (await h.query({ kind: "GetRecoveryStatus", ctx: old })).requests.filter((r) => r.direction === "outgoing");
    expect(mine.map((r) => [r.scope, r.state, r.activatesAt])).toEqual([["identity", "waiting", 6000], ["history", "completed", null]]);
  });

  it("veto stops an incoming recovery; cancelling our own ends its StartRecovery operation", async () => {
    const { h, old } = setup();
    expect(await run(h, { kind: "VetoRecovery", ctx: old, operationId: op(), requestId: "rr-in", confirmation: token })).toEqual({ kind: "Committed" });
    expect((await h.query({ kind: "GetRecoveryStatus", ctx: old })).requests[0]).toMatchObject({ state: "vetoed", canStop: false });

    const startOp = op();
    await run(h, { kind: "StartRecovery", ctx: old, operationId: startOp, scope: "identity", custodian: "recovery_key", confirmation: token });
    const outgoing = (await h.query({ kind: "GetRecoveryStatus", ctx: old })).requests.find((r) => r.operationId === startOp);
    if (!outgoing) throw new Error("no request");
    expect(await run(h, { kind: "VetoRecovery", ctx: old, operationId: op(), requestId: outgoing.requestId, confirmation: token })).toEqual({ kind: "Committed" });
    expect((await h.pendingOperations(old)).find((o) => o.operationId === startOp)?.outcome).toEqual({ kind: "Rejected", reason: "revoked" });
    expect(() => { h.resolveAuthorization(startOp, { kind: "Committed" }); }).toThrow();
  });

  it("an explicitly-off policy cannot start recovery", async () => {
    const { h, old } = setup({ recovery: { policy: "off_explicit" } });
    expect(await run(h, { kind: "StartRecovery", ctx: old, operationId: op(), scope: "identity", custodian: "recovery_key", confirmation: token })).toEqual({ kind: "Rejected", reason: "unsupported" });
  });
});
