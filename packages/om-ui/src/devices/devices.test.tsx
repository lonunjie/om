// @vitest-environment jsdom
import { cleanup, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import axe from "axe-core";
import { afterEach, describe, expect, it, vi } from "vitest";
import { asOperationId, asProfileId } from "@om/platform-api";
import { createHarness, type SeedDevices } from "@om/core-harness";
import { createActionRegistry } from "../actions/registry";
import { testDevicesCopy, testLinkThisDeviceCopy } from "./__fixtures__/copy";
import { DevicesScreen, LinkThisDevice, type SettingsServices } from "./index";

afterEach(cleanup);
const axeViolations = async (el: Element): Promise<string[]> => {
  const r = await axe.run(el, { rules: { "color-contrast": { enabled: false }, region: { enabled: false } } });
  return r.violations.map((v) => `${v.id}: ${v.help}`);
};

const ME = asProfileId("me");
const NEW = asProfileId("new");
const TWO: SeedDevices = {
  devices: [
    { id: "d-this", label: "This laptop", role: "control", current: true },
    { id: "d-phone", label: "Phone", role: "control" },
    { id: "d-tablet", label: "Tablet", role: "messaging", lastSeen: "long_ago" },
  ],
};
const THREE: SeedDevices = { devices: [...(TWO.devices ?? []), { id: "d-desk", label: "Desktop", role: "control" }] };

function setup(account: SeedDevices = TWO, opIds: string[] = []) {
  const h = createHarness({ env: {}, clock: () => 1000 });
  h.seedProfile({ profileId: ME, label: "Me", account });
  h.seedProfile({ profileId: NEW, label: "New" });
  let n = 0;
  const registry = createActionRegistry({ newOperationId: () => asOperationId(opIds[n++] ?? `op-${String(n)}`) });
  const services: SettingsServices = { client: h, registry, ctx: h.contextFor(ME) };
  return { h, services, registry };
}
const kinds = async (h: ReturnType<typeof setup>["h"], s: SettingsServices) => (await h.pendingOperations(s.ctx)).map((o) => o.intentKind);

describe("Devices screen", () => {
  it("lists devices from Core; rows only open details (no action runs from a list row)", async () => {
    const { h, services } = setup();
    const { container } = render(<DevicesScreen services={services} copy={testDevicesCopy} />);
    await screen.findByRole("heading", { name: "Other devices" });
    expect(await axeViolations(container)).toEqual([]);
    expect(screen.queryByRole("button", { name: /Suspend|Remove/ })).toBeNull();
    await userEvent.click(screen.getByRole("button", { name: /Tablet/ }));
    expect(await screen.findByRole("heading", { name: "Device: Tablet" })).toBe(document.activeElement);
    expect(screen.getByText("Long idle")).toBeTruthy();
    expect(await kinds(h, services)).toEqual([]);
    expect(await axeViolations(container)).toEqual([]);
  });

  it("with two controllers, suspending is not offered (Core: it would freeze); removal is distinct and stays pending until authorized", async () => {
    const { h, services } = setup(TWO, ["op-ic", "op-revoke"]);
    render(<DevicesScreen services={services} copy={testDevicesCopy} />);
    await userEvent.click(await screen.findByRole("button", { name: /Phone/ }));
    expect(await screen.findByText("Suspending would freeze the account")).toBeTruthy();
    expect(screen.queryByRole("button", { name: "Suspend this device" })).toBeNull();
    await userEvent.click(screen.getByRole("button", { name: "Remove permanently" }));
    const group = screen.getByRole("group", { name: "Remove Phone permanently?" });
    expect(within(group).getByText("Data already on it stays")).toBeTruthy();
    expect(await kinds(h, services)).toEqual([]); // opening the confirmation dispatched nothing
    await userEvent.click(within(group).getByRole("button", { name: "Yes, remove permanently" }));
    expect(await screen.findByText("Waiting for approval")).toBeTruthy();
    expect(await screen.findByText("Removal waiting for approval")).toBeTruthy();
    expect(screen.queryByRole("button", { name: "Remove permanently" })).toBeNull(); // no second operation while pending
    h.resolveAuthorization(asOperationId("op-revoke"), { kind: "Committed" });
    expect(await screen.findByText("Removed", { selector: "li" })).toBeTruthy();
  });

  it("suspend uses its own consequence wording and confirmation, and Core's quarantine state is shown", async () => {
    const { h, services } = setup(THREE);
    const { container } = render(<DevicesScreen services={services} copy={testDevicesCopy} />);
    await userEvent.click(await screen.findByRole("button", { name: /Phone/ }));
    await userEvent.click(await screen.findByRole("button", { name: "Suspend this device" }));
    const group = screen.getByRole("group", { name: "Suspend Phone?" });
    expect(within(group).getByText("Not removed")).toBeTruthy();
    expect(within(group).queryByText("Can never come back")).toBeNull();
    expect(await axeViolations(container)).toEqual([]);
    await userEvent.click(within(group).getByRole("button", { name: "Yes, suspend" }));
    expect(await screen.findByText("Suspended", { selector: "p" })).toBeTruthy();
    expect(await kinds(h, services)).toEqual(["IssueConfirmation", "SuspendDevice"]);
    expect(screen.getByText("Suspended", { selector: "li" })).toBeTruthy();
  });

  it("Safety Freeze is shown on the reserved surface and no device change is offered", async () => {
    const { services } = setup({ ...THREE, controlState: "safety_freeze" });
    render(<DevicesScreen services={services} copy={testDevicesCopy} />);
    const banner = (await screen.findByText("Safety Freeze is on")).closest(".om-security-notice");
    expect(banner?.getAttribute("data-reason-code")).toBe("safety_freeze");
    expect(within(banner as HTMLElement).queryByRole("button")).toBeNull(); // not dismissable
    await userEvent.click(screen.getByRole("button", { name: /Phone/ }));
    expect(await screen.findByText("Changes paused by Safety Freeze")).toBeTruthy();
    expect(screen.queryByRole("button", { name: /Suspend|Remove/ })).toBeNull();
  });
});

describe("device linking (NoSilentLink)", () => {
  it("existing device: paste offer -> review shows who and the same code -> explicit approval -> enrollment notice", async () => {
    const { h, services } = setup();
    const offer = h.simulateNewDeviceOffer("Work laptop");
    const { container } = render(<DevicesScreen services={services} copy={testDevicesCopy} />);
    await userEvent.click(await screen.findByRole("button", { name: "Link a new device" }));
    const box = screen.getByRole("textbox", { name: "Link from the new device" });
    await userEvent.type(box, "om:invite/AAAAAAAAAAAAAAAAAAAAAA");
    await userEvent.click(screen.getByRole("button", { name: "Continue" }));
    expect(await screen.findByRole("alert")).toHaveProperty("textContent", "Not a device link");
    expect(await kinds(h, services)).toEqual([]);
    await userEvent.clear(box);
    await userEvent.type(box, offer.link);
    await userEvent.click(screen.getByRole("button", { name: "Continue" }));

    expect(await screen.findByRole("heading", { name: "Check the new device" })).toBeTruthy();
    expect(screen.getByText("Work laptop wants to join your account")).toBeTruthy();
    expect(screen.getByText(offer.verificationCode)).toBeTruthy();
    expect(screen.getByText("Only link a device you are holding")).toBeTruthy();
    expect(screen.getByText("All your other devices will be told")).toBeTruthy();
    expect(await kinds(h, services)).toEqual(["RequestDeviceLink"]); // nothing approved automatically
    expect((await h.query({ kind: "GetDevices", ctx: services.ctx })).devices).toHaveLength(3);
    expect(await axeViolations(container)).toEqual([]);

    await userEvent.click(screen.getByRole("button", { name: "The codes match, link it" }));
    expect(await screen.findByText("Device linked")).toBeTruthy();
    expect(await kinds(h, services)).toEqual(["RequestDeviceLink", "IssueConfirmation", "ApproveDeviceLink"]);
    const notice = screen.getByText("New device linked: Work laptop").closest(".om-security-notice");
    expect(notice?.getAttribute("data-reason-code")).toBe("device_linked");
  });

  it("reject links nothing; Hardened approval renders PendingAuthorization, never success", async () => {
    const { h, services } = setup({ ...TWO, linkNeedsAuthorization: true }, ["op-open", "op-ic", "op-approve"]);
    const offer = h.simulateNewDeviceOffer("Laptop");
    render(<DevicesScreen services={services} copy={testDevicesCopy} />);
    await userEvent.click(await screen.findByRole("button", { name: "Link a new device" }));
    await userEvent.type(screen.getByRole("textbox", { name: "Link from the new device" }), offer.link);
    await userEvent.click(screen.getByRole("button", { name: "Continue" }));
    await userEvent.click(await screen.findByRole("button", { name: "The codes match, link it" }));
    expect(await screen.findByText("Waiting for approval")).toBeTruthy();
    expect(screen.getByText("Waiting for your other devices")).toBeTruthy();
    expect(screen.queryByText("Device linked")).toBeNull();
    h.resolveAuthorization(asOperationId("op-approve"), { kind: "Committed" });
    expect(await screen.findByText("Device linked")).toBeTruthy();

    cleanup();
    const second = h.simulateNewDeviceOffer("Unknown");
    render(<DevicesScreen services={services} copy={testDevicesCopy} />);
    await userEvent.click(await screen.findByRole("button", { name: "Link a new device" }));
    await userEvent.type(screen.getByRole("textbox", { name: "Link from the new device" }), second.link);
    await userEvent.click(screen.getByRole("button", { name: "Continue" }));
    await userEvent.click(await screen.findByRole("button", { name: "Not mine, reject" }));
    expect(await screen.findByText("Link rejected")).toBeTruthy();
    expect((await h.query({ kind: "GetDevices", ctx: services.ctx })).devices).toHaveLength(4);
  });

  it("UI-12/13: a timed-out approval stays pending across restart and is never offered twice", async () => {
    const { h, services } = setup(TWO, ["op-open", "op-ic", "op-approve"]);
    const offer = h.simulateNewDeviceOffer("Laptop");
    h.injectNetworkTimeoutAfterSend(asOperationId("op-approve"));
    render(<DevicesScreen services={services} copy={testDevicesCopy} />);
    await userEvent.click(await screen.findByRole("button", { name: "Link a new device" }));
    await userEvent.type(screen.getByRole("textbox", { name: "Link from the new device" }), offer.link);
    await userEvent.click(screen.getByRole("button", { name: "Continue" }));
    await userEvent.click(await screen.findByRole("button", { name: "The codes match, link it" }));
    expect(await screen.findByText("Checking result")).toBeTruthy();
    expect(screen.queryByRole("alert")).toBeNull(); // a timeout is not a failure

    cleanup();
    h.simulateRestart();
    render(<DevicesScreen services={{ ...services, ctx: h.contextFor(ME) }} copy={testDevicesCopy} />);
    expect(await screen.findByText("Approving link: Checking result")).toBeTruthy();
    await userEvent.click(screen.getByRole("button", { name: /Laptop wants to link/ }));
    expect(await screen.findByRole("heading", { name: "Check the new device" })).toBeTruthy();
    expect(screen.queryByRole("button", { name: "The codes match, link it" })).toBeNull();
    h.resolveReconciliation(asOperationId("op-approve"), { kind: "Committed" });
    expect(await screen.findByText("Linked")).toBeTruthy();
    expect((await h.pendingOperations(services.ctx)).filter((o) => o.intentKind === "ApproveDeviceLink")).toHaveLength(1);
  });

  it("NeedsUserAction is shown as an action for the person, not as success", async () => {
    const { h, services } = setup(TWO, ["op-open", "op-ic", "op-approve"]);
    const offer = h.simulateNewDeviceOffer("Laptop");
    h.injectUserActionRequired(asOperationId("op-approve"), "reauthenticate");
    render(<DevicesScreen services={services} copy={testDevicesCopy} />);
    await userEvent.click(await screen.findByRole("button", { name: "Link a new device" }));
    await userEvent.type(screen.getByRole("textbox", { name: "Link from the new device" }), offer.link);
    await userEvent.click(screen.getByRole("button", { name: "Continue" }));
    await userEvent.click(await screen.findByRole("button", { name: "The codes match, link it" }));
    expect(await screen.findByRole("alert")).toHaveProperty("textContent", "Action: reauthenticate");
  });

  it("scan slot feeds the same open step; a cancelled scan dispatches nothing", async () => {
    const { h, services } = setup();
    const offer = h.simulateNewDeviceOffer("Phone 2");
    const scanQr = vi.fn<() => Promise<string | null>>().mockResolvedValueOnce(null).mockResolvedValueOnce(offer.link);
    render(<DevicesScreen services={services} copy={testDevicesCopy} scanQr={scanQr} />);
    await userEvent.click(await screen.findByRole("button", { name: "Link a new device" }));
    await userEvent.click(screen.getByRole("button", { name: "Scan QR code" }));
    expect(await screen.findByText("No code found")).toBeTruthy();
    expect(await kinds(h, services)).toEqual([]);
    await userEvent.click(screen.getByRole("button", { name: "Scan QR code" }));
    expect(await screen.findByText("Phone 2 wants to join your account")).toBeTruthy();
  });

  it("new device: shows the offer only when asked, then the same code the existing device sees", async () => {
    const { h, registry } = setup(TWO, ["op-offer"]);
    const fresh: SettingsServices = { client: h, registry, ctx: h.contextFor(NEW) };
    const renderQr = vi.fn((payload: string) => <span data-testid="qr">{payload}</span>);
    const { container } = render(<LinkThisDevice services={fresh} copy={testLinkThisDeviceCopy} renderQr={renderQr} />);
    expect(renderQr).not.toHaveBeenCalled();
    await userEvent.click(screen.getByRole("button", { name: "Show link code" }));
    const link = (await screen.findByTestId("qr")).textContent;
    expect(link).toMatch(/^om:link\//);
    expect(screen.getByText("Waiting for scan")).toBeTruthy();
    expect(await axeViolations(container)).toEqual([]);

    const me = h.contextFor(ME);
    const handle = await h.dispatch({ kind: "RequestDeviceLink", ctx: me, operationId: asOperationId("op-other"), link });
    const request = await h.query({ kind: "GetLinkRequest", ctx: me, operationId: handle.operationId });
    expect(await screen.findByText("Approve on your other device")).toBeTruthy();
    expect(screen.getByText(request?.verificationCode ?? "missing")).toBeTruthy();
    expect(screen.queryByRole("button", { name: /approve|link it/i })).toBeNull(); // approval happens only on the existing device
  });
});

describe("devices screen copy failures", () => {
  it("a Core query failure is shown, not hidden", async () => {
    const { h, services } = setup();
    const broken: SettingsServices = { ...services, client: { ...h, query: () => Promise.reject(new Error("down")) } };
    render(<DevicesScreen services={broken} copy={testDevicesCopy} />);
    await waitFor(() => { expect(screen.getByRole("alert").textContent).toBe("Could not load"); });
  });
});
