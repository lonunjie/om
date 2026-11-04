// @vitest-environment jsdom
import { cleanup, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import axe from "axe-core";
import { afterEach, describe, expect, it, vi } from "vitest";
import { asOperationId, asProfileId } from "@om/platform-api";
import { createHarness, type SeedDevices } from "@om/core-harness";
import { createActionRegistry } from "../actions/registry";
import { testDevicesCopy, testJoinCopy } from "./__fixtures__/copy";
import { DeviceNotices, DevicesScreen, JoinWithLink, type SettingsServices } from "./index";

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

describe("device linking not ready in Core", () => {
  it("shows no link button, only a plain note, so nobody starts a flow that ends in 'unsupported'", async () => {
    const { services } = setup();
    render(<DevicesScreen services={services} copy={testDevicesCopy} linking="later" />);
    await screen.findByText("Linking is not available yet");
    expect(screen.queryByRole("button", { name: "Link a new device" })).toBeNull();
  });
});

/** Opens "Add a device" on the existing device and shows the link; returns the link text. */
async function showLink(): Promise<string> {
  await userEvent.click(await screen.findByRole("button", { name: "Add a device" }));
  await userEvent.click(screen.getByRole("button", { name: "Show link and QR code" }));
  const link = (await screen.findByText(/^om:link\//)).textContent;
  expect(link).toMatch(/^om:link\//);
  return link;
}

describe("adding a device from the existing device (D24: this device shows the link)", () => {
  it("shows a QR and a link that cannot be mistaken for an invite; the new device asking links nothing; approval is explicit and Core-confirmed", async () => {
    const { h, services } = setup(TWO, ["op-offer", "op-ic", "op-approve"]);
    const renderQr = vi.fn((payload: string) => <span data-testid="qr">{payload}</span>);
    const { container } = render(<DevicesScreen services={services} copy={testDevicesCopy} renderQr={renderQr} />);
    await userEvent.click(await screen.findByRole("button", { name: "Add a device" }));
    expect(renderQr).not.toHaveBeenCalled(); // nothing is offered until asked
    expect(await kinds(h, services)).toEqual([]);
    await userEvent.click(screen.getByRole("button", { name: "Show link and QR code" }));
    const frame = (await screen.findByTestId("qr")).closest("figure");
    expect(frame?.getAttribute("data-kind")).toBe("device-link"); // a distinct frame, not the invite's
    expect(within(frame as HTMLElement).getByText("Adds a device")).toBeTruthy();
    expect(screen.getByText("This is not an invitation")).toBeTruthy();
    expect(screen.getByText("Waiting for the new device")).toBeTruthy();
    const link = screen.getByTestId("qr").textContent;
    expect(link).toMatch(/^om:link\//);
    expect(await axeViolations(container)).toEqual([]);

    expect(h.simulateNewDeviceRequest(link, "Work laptop")).toBe(true);
    expect(await screen.findByRole("heading", { name: "Check the new device" })).toBeTruthy();
    expect(screen.getByText("Work laptop wants to join your account")).toBeTruthy();
    expect(screen.getByText("Only link a device you are holding")).toBeTruthy();
    expect(screen.getByText("All your other devices will be told")).toBeTruthy();
    const code = (await h.query({ kind: "GetLinkOffer", ctx: services.ctx, operationId: asOperationId("op-offer") }))?.verificationCode ?? "missing";
    expect(screen.getByText(code, { selector: "output" })).toBeTruthy();
    expect(await kinds(h, services)).toEqual(["OfferDeviceLink"]); // nothing approved automatically
    expect((await h.query({ kind: "GetDevices", ctx: services.ctx })).devices).toHaveLength(3);
    expect(await axeViolations(container)).toEqual([]);

    await userEvent.click(screen.getByRole("button", { name: "The codes match, link it" }));
    expect(await screen.findByText("Device added")).toBeTruthy();
    expect(await kinds(h, services)).toEqual(["OfferDeviceLink", "IssueConfirmation", "ApproveDeviceLink"]);
    expect((await h.query({ kind: "GetDevices", ctx: services.ctx })).devices).toHaveLength(4);
  });

  it("declining links nothing and offers a fresh start", async () => {
    const { h, services } = setup(TWO, ["op-offer", "op-reject"]);
    render(<DevicesScreen services={services} copy={testDevicesCopy} />);
    const link = await showLink();
    h.simulateNewDeviceRequest(link, "Unknown");
    await userEvent.click(await screen.findByRole("button", { name: "Not mine, reject" }));
    expect((await screen.findAllByText("Declined")).length).toBeGreaterThan(0);
    expect((await h.query({ kind: "GetDevices", ctx: services.ctx })).devices).toHaveLength(3);
    await userEvent.click(screen.getByRole("button", { name: "Try again" }));
    expect(await screen.findByRole("button", { name: "Show link and QR code" })).toBeTruthy();
  });

  it("Hardened approval renders PendingAuthorization, never success, until another device agrees", async () => {
    const { h, services } = setup({ ...TWO, linkNeedsAuthorization: true }, ["op-offer", "op-ic", "op-approve"]);
    render(<DevicesScreen services={services} copy={testDevicesCopy} />);
    const link = await showLink();
    h.simulateNewDeviceRequest(link, "Laptop");
    await userEvent.click(await screen.findByRole("button", { name: "The codes match, link it" }));
    expect(await screen.findByText("Waiting for approval")).toBeTruthy();
    expect(screen.getByText("Waiting for your other devices")).toBeTruthy();
    expect(screen.queryByText("Device linked")).toBeNull();
    h.resolveAuthorization(asOperationId("op-approve"), { kind: "Committed" });
    expect(await screen.findByText("Device added")).toBeTruthy();
  });

  it("UI-12/13: a timed-out approval stays pending across restart and is never offered twice", async () => {
    const { h, services } = setup(TWO, ["op-offer", "op-ic", "op-approve"]);
    h.injectNetworkTimeoutAfterSend(asOperationId("op-approve"));
    render(<DevicesScreen services={services} copy={testDevicesCopy} />);
    const link = await showLink();
    h.simulateNewDeviceRequest(link, "Laptop");
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
    const { h, services } = setup(TWO, ["op-offer", "op-ic", "op-approve"]);
    h.injectUserActionRequired(asOperationId("op-approve"), "reauthenticate");
    render(<DevicesScreen services={services} copy={testDevicesCopy} />);
    const link = await showLink();
    h.simulateNewDeviceRequest(link, "Laptop");
    await userEvent.click(await screen.findByRole("button", { name: "The codes match, link it" }));
    expect(await screen.findByRole("alert")).toHaveProperty("textContent", "Action: reauthenticate");
  });

  it("a device that cannot manage the account is not offered the flow", async () => {
    const { services } = setup({ devices: [{ id: "d-this", label: "Tablet", role: "messaging", current: true }, { id: "d-phone", label: "Phone", role: "control" }] });
    render(<DevicesScreen services={services} copy={testDevicesCopy} />);
    await screen.findByText("Add devices from a device that manages your account");
    expect(screen.queryByRole("button", { name: "Add a device" })).toBeNull();
  });
});

describe("joining from the new device (D24: this device types or scans the link)", () => {
  function joiner() {
    const { h, registry } = setup(TWO, ["op-ask"]);
    const fresh: SettingsServices = { client: h, registry, ctx: h.contextFor(NEW) };
    return { h, fresh };
  }

  it("a wrong link dispatches nothing; a good one shows the same code and waits; only the other device's approval moves it on", async () => {
    const { h, fresh } = joiner();
    const offer = h.simulateExistingDeviceOffer();
    const onLinked = vi.fn();
    const { container } = render(<JoinWithLink services={fresh} copy={testJoinCopy} onLinked={onLinked} onCancel={vi.fn()} />);
    const box = screen.getByRole("textbox", { name: "Link from your other device" });
    await userEvent.type(box, "om:invite/AAAAAAAAAAAAAAAAAAAAAA");
    await userEvent.click(screen.getByRole("button", { name: "Continue" }));
    expect(await screen.findByRole("alert")).toHaveProperty("textContent", "Not a device link");
    expect(await kinds(h, fresh)).toEqual([]);
    await userEvent.clear(box);
    await userEvent.type(box, offer.link);
    await userEvent.click(screen.getByRole("button", { name: "Continue" }));

    expect(await screen.findByText(offer.verificationCode, { selector: "output" })).toBeTruthy();
    expect(screen.getByText("Approve on your other device")).toBeTruthy();
    expect(screen.queryByRole("button", { name: /approve|link it/i })).toBeNull(); // approval happens only on the existing device
    expect(onLinked).not.toHaveBeenCalled();
    expect(await kinds(h, fresh)).toEqual(["RequestDeviceLink"]);
    expect(await axeViolations(container)).toEqual([]);

    h.simulateExistingDeviceDecision(offer.link, "approve");
    expect(await screen.findByText("You are linked")).toBeTruthy();
    await waitFor(() => { expect(onLinked).toHaveBeenCalledTimes(1); });
  });

  it("a declined or expired request ends the attempt with a way to start again; the scan slot feeds the same step", async () => {
    const { h, fresh } = joiner();
    const offer = h.simulateExistingDeviceOffer();
    const onCancel = vi.fn();
    const scanQr = vi.fn<() => Promise<string | null>>().mockResolvedValueOnce(null).mockResolvedValueOnce(offer.link);
    render(<JoinWithLink services={fresh} copy={testJoinCopy} scanQr={scanQr} onLinked={vi.fn()} onCancel={onCancel} />);
    await userEvent.click(screen.getByRole("button", { name: "Scan QR code" }));
    expect(await screen.findByText("No code found")).toBeTruthy();
    expect(await kinds(h, fresh)).toEqual([]); // a cancelled scan dispatches nothing
    await userEvent.click(screen.getByRole("button", { name: "Scan QR code" }));
    await screen.findByText("Approve on your other device");
    h.simulateExistingDeviceDecision(offer.link, "reject");
    expect(await screen.findByRole("alert")).toHaveProperty("textContent", "Declined");
    await userEvent.click(screen.getByRole("button", { name: "Start again" }));
    expect(onCancel).toHaveBeenCalledTimes(1);
  });

  it("Core refusing the link (already used) is shown and nothing waits", async () => {
    const { h, fresh } = joiner();
    const offer = h.simulateExistingDeviceOffer();
    const other = h.seedProfile({ profileId: asProfileId("other-new"), label: "Other" });
    await h.dispatch({ kind: "RequestDeviceLink", ctx: other, operationId: asOperationId("op-first"), link: offer.link });
    render(<JoinWithLink services={fresh} copy={testJoinCopy} onLinked={vi.fn()} onCancel={vi.fn()} />);
    await userEvent.type(screen.getByRole("textbox", { name: "Link from your other device" }), offer.link);
    await userEvent.click(screen.getByRole("button", { name: "Continue" }));
    expect(await screen.findByRole("alert")).toHaveProperty("textContent", "Rejected: conflict");
    expect(screen.queryByText("Approve on your other device")).toBeNull();
  });
});

describe("the new-device notice (R-166)", () => {
  it("shows on every other device, on the reserved surface, with no way to dismiss it", async () => {
    const { h, services } = setup(TWO, ["op-offer", "op-ic", "op-approve"]);
    // The account has more than one device when a third joins: the notice is what the others see.
    const { container } = render(<><DeviceNotices services={services} copy={testDevicesCopy} /><DevicesScreen services={services} copy={testDevicesCopy} showNotices={false} /></>);
    expect(container.querySelector(".om-security-notice")).toBeNull();
    const offerHandle = await h.dispatch({ kind: "OfferDeviceLink", ctx: services.ctx, operationId: asOperationId("op-offer") });
    const offer = await h.query({ kind: "GetLinkOffer", ctx: services.ctx, operationId: offerHandle.operationId });
    h.simulateNewDeviceRequest(offer?.link ?? "", "Work laptop");
    const request = await h.query({ kind: "GetLinkRequest", ctx: services.ctx, operationId: offerHandle.operationId });
    const issue = await h.dispatch({ kind: "IssueConfirmation", ctx: services.ctx, operationId: asOperationId("op-ic"), intentKind: "ApproveDeviceLink", target: request?.requestId ?? "" });
    const nonce = await h.query({ kind: "GetConfirmation", ctx: services.ctx, operationId: issue.operationId });
    await h.dispatch({ kind: "ApproveDeviceLink", ctx: services.ctx, operationId: asOperationId("op-approve"), requestId: request?.requestId ?? "", confirmation: Object.freeze({ nonce: nonce?.nonce ?? "" }) as never });
    const notice = (await screen.findByText("New device linked: Work laptop")).closest(".om-security-notice");
    expect(notice?.getAttribute("data-reason-code")).toBe("device_linked");
    expect(within(notice as HTMLElement).queryByRole("button")).toBeNull();
    expect(container.querySelectorAll(".om-security-notice")).toHaveLength(1); // not repeated by the Devices screen
    expect(await axeViolations(container)).toEqual([]);
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
