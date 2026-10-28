// @vitest-environment jsdom
import { cleanup, render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import axe from "axe-core";
import { afterEach, describe, expect, it } from "vitest";
import { asOperationId, asProfileId } from "@om/platform-api";
import type { ConfirmationToken } from "@om/platform-api";
import { createHarness, type SeedDevices } from "@om/core-harness";
import { createActionRegistry } from "../actions/registry";
import { testRecoveryCopy } from "../devices/__fixtures__/copy";
import type { SettingsServices } from "../devices/operation";
import { RecoveryScreen } from "./index";

afterEach(cleanup);
const axeViolations = async (el: Element): Promise<string[]> => {
  const r = await axe.run(el, { rules: { "color-contrast": { enabled: false }, region: { enabled: false } } });
  return r.violations.map((v) => `${v.id}: ${v.help}`);
};

const ME = asProfileId("me");
const ACCOUNT: SeedDevices = {
  devices: [{ id: "d-this", label: "Laptop", role: "control", current: true }, { id: "d-phone", label: "Phone", role: "control" }],
  recovery: { custodians: ["synced_passkey", "recovery_key"], activationDelayMs: 500, incoming: [{ requestId: "rr-in", scope: "identity", custodian: "recovery_key", startedAt: 900, activatesAt: 1400 }] },
};

function setup(account: SeedDevices = ACCOUNT, opIds: string[] = []) {
  const h = createHarness({ env: {}, clock: () => 1000 });
  h.seedProfile({ profileId: ME, label: "Me", account });
  let n = 0;
  const registry = createActionRegistry({ newOperationId: () => asOperationId(opIds[n++] ?? `op-${String(n)}`) });
  const services: SettingsServices = { client: h, registry, ctx: h.contextFor(ME) };
  return { h, services };
}
const kinds = async (h: ReturnType<typeof setup>["h"], s: SettingsServices) => (await h.pendingOperations(s.ctx)).map((o) => o.intentKind);

describe("Recovery screen", () => {
  it("shows Core's status, custodians, scopes and timers; the recovery key is never shown again", async () => {
    const { services } = setup();
    const { container } = render(<RecoveryScreen services={services} copy={testRecoveryCopy} />);
    expect(await screen.findByText("Recovery is on")).toBeTruthy();
    expect(screen.getByText("Your passkeys (this app only)")).toBeTruthy();
    expect(screen.getByText("Delay 500")).toBeTruthy();
    expect(screen.getByText("Completes 1400")).toBeTruthy(); // Core's time, not computed in the UI
    expect(screen.getByText("The key cannot be shown again")).toBeTruthy();
    expect(screen.queryByRole("button", { name: /show.*key/i })).toBeNull();
    expect(await axeViolations(container)).toEqual([]);
  });

  it("an incoming recovery is alerted on the reserved surface and can be stopped only from its own step", async () => {
    const { h, services } = setup();
    const { container } = render(<RecoveryScreen services={services} copy={testRecoveryCopy} />);
    const banner = (await screen.findByText("Someone is recovering your account")).closest(".om-security-notice");
    expect(banner?.getAttribute("data-tone")).toBe("danger");
    await userEvent.click(within(banner as HTMLElement).getByRole("button", { name: "Review" }));
    expect(await screen.findByRole("heading", { name: "Stop this recovery?" })).toBe(document.activeElement);
    expect(await kinds(h, services)).toEqual([]);
    expect(await axeViolations(container)).toEqual([]);
    await userEvent.click(screen.getByRole("button", { name: "Yes, stop it" }));
    expect(await screen.findByText("Stopped", { selector: ".om-settings__status" })).toBeTruthy();
    expect(await kinds(h, services)).toEqual(["IssueConfirmation", "VetoRecovery"]);
    expect(screen.queryByText("Someone is recovering your account")).toBeNull();
  });

  it("start: scopes stay distinct, consequences shown before confirming, identity recovery stays pending (never shown as done)", async () => {
    const { h, services } = setup(ACCOUNT, ["op-ic", "op-start"]);
    const { container } = render(<RecoveryScreen services={services} copy={testRecoveryCopy} />);
    await userEvent.click(await screen.findByRole("button", { name: "Start a recovery" }));
    const confirm = screen.getByRole("button", { name: "Start recovery" });
    expect(confirm).toHaveProperty("disabled", true);
    await userEvent.click(screen.getByRole("radio", { name: /History/ }));
    expect(screen.getByText("Restores messages only")).toBeTruthy();
    await userEvent.click(screen.getByRole("radio", { name: /Account/ }));
    expect(screen.queryByText("Restores messages only")).toBeNull();
    expect(screen.getByText("Your devices can stop it")).toBeTruthy();
    await userEvent.click(screen.getByRole("radio", { name: "Your recovery key" }));
    expect(await axeViolations(container)).toEqual([]);
    expect(await kinds(h, services)).toEqual([]);
    await userEvent.click(confirm);
    expect(await screen.findByText("Waiting for approval")).toBeTruthy();
    expect(screen.queryByText("Recovery done")).toBeNull();
    expect(screen.queryByRole("button", { name: "Start recovery" })).toBeNull();
    const ops = await h.pendingOperations(services.ctx);
    expect(ops.map((o) => [o.intentKind, o.outcome.kind])).toEqual([["IssueConfirmation", "Committed"], ["StartRecovery", "PendingAuthorization"]]);
    expect(ops[1]?.operationId).toBe("op-start");
  });

  it("cancelling our own pending recovery ends it; restart shows pending operations from the durable log", async () => {
    const { h, services } = setup(ACCOUNT, ["op-start"]);
    await h.dispatch({ kind: "IssueConfirmation", ctx: services.ctx, operationId: asOperationId("op-nonce"), intentKind: "StartRecovery", target: "identity:recovery_key" });
    const issued = await h.query({ kind: "GetConfirmation", ctx: services.ctx, operationId: asOperationId("op-nonce") });
    await h.dispatch({ kind: "StartRecovery", ctx: services.ctx, operationId: asOperationId("op-pre"), scope: "identity", custodian: "recovery_key", confirmation: Object.freeze({ nonce: issued?.nonce ?? "" }) as ConfirmationToken });
    h.simulateRestart();
    render(<RecoveryScreen services={{ ...services, ctx: h.contextFor(ME) }} copy={testRecoveryCopy} />);
    expect(await screen.findByText("Starting recovery: Waiting for approval")).toBeTruthy();
    await userEvent.click(screen.getByRole("button", { name: "Cancel my recovery" }));
    await userEvent.click(await screen.findByRole("button", { name: "Yes, cancel it" }));
    expect(await screen.findByText("Stopped", { selector: ".om-settings__status" })).toBeTruthy();
    const start = (await h.pendingOperations(services.ctx)).find((o) => o.operationId === "op-pre");
    expect(start?.outcome).toEqual({ kind: "Rejected", reason: "revoked" });
  });

  it("explicitly-off recovery offers no start path", async () => {
    const { services } = setup({ recovery: { policy: "off_explicit" } });
    render(<RecoveryScreen services={services} copy={testRecoveryCopy} />);
    expect(await screen.findByText("Recovery is off")).toBeTruthy();
    expect(screen.queryByRole("button", { name: "Start a recovery" })).toBeNull();
  });
});
