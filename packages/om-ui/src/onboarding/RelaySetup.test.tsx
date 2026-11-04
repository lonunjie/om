// @vitest-environment jsdom
import { cleanup, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import axe from "axe-core";
import { afterEach, describe, expect, it, vi } from "vitest";
import { asProfileId, available, unavailable, FAMILY_NAMES } from "@om/platform-api";
import type { CapabilityEntry, CapabilityReport, FamilyName, PlatformAdapter } from "@om/platform-api";
import { createHarness } from "@om/core-harness";
import { createActionRegistry } from "../actions/registry";
import type { HomeProbeResult } from "../home/services";
import { testCopy } from "./__fixtures__/copy";
import { RelaySetup, type RelaySetupServices } from "./RelaySetup";
import type { OnboardingPlatform } from "./platform";

afterEach(() => { cleanup(); vi.restoreAllMocks(); });

const copy = testCopy.relay;
const HEX = "ab".repeat(32);
const DEPLOY = "https://deploy.example/start";

function platformWith(written: string[], clipboard: boolean): OnboardingPlatform {
  const entry = (state: CapabilityEntry["state"]): CapabilityEntry => ({ state });
  const families = Object.fromEntries(FAMILY_NAMES.map((f) => [f, entry("unavailable")])) as Record<FamilyName, CapabilityEntry>;
  const report: CapabilityReport = { families: { ...families, clipboard: entry(clipboard ? "available" : "unavailable") }, backgroundPush: "none", localDiscovery: "manual_known_endpoint", localContacts: "unavailable", callSystemIntegration: false };
  return {
    capabilities: () => report,
    clipboard: () => (clipboard ? available({ write: (t: string) => { written.push(t); return Promise.resolve("written" as const); } }) : unavailable<never>()),
  } as unknown as OnboardingPlatform & PlatformAdapter;
}

interface Rig { readonly services: RelaySetupServices; readonly written: string[]; readonly done: ReturnType<typeof vi.fn>; readonly connect: ReturnType<typeof vi.fn>; readonly probe: ReturnType<typeof vi.fn> }
function mount(over: { probe?: HomeProbeResult; deployUrl?: string | undefined; connect?: boolean; clipboard?: boolean; prepare?: () => Promise<string> } = {}): Rig {
  const h = createHarness({ env: {}, clock: () => 1000 });
  const ctx = h.seedProfile({ profileId: asProfileId("me"), label: "pending" });
  const done = vi.fn();
  const connect = vi.fn(() => Promise.resolve(over.connect ?? true));
  const healthy: HomeProbeResult = { kind: "ok", baseUrl: "https://mine.example", providerId: "mine", health: "healthy", admission: true };
  const probe = vi.fn((): Promise<HomeProbeResult> => Promise.resolve(over.probe ?? healthy));
  const services: RelaySetupServices = { deployUrl: "deployUrl" in over ? over.deployUrl : DEPLOY, prepare: over.prepare ?? (() => Promise.resolve(HEX)), probe, connect, done };
  const written: string[] = [];
  render(<RelaySetup services={services} copy={copy} frame={testCopy.frame} platform={platformWith(written, over.clipboard ?? true)} registry={createActionRegistry()} ctx={ctx} />);
  return { services, written, done, connect, probe };
}

const pickOwn = async (): Promise<void> => { await userEvent.click(screen.getByRole("button", { name: copy.ownFree })); };

describe("RelaySetup (first run, ADR-0186)", () => {
  it("page one says why OM is different, offers the built way as a button and lists the others as plain text, not controls", () => {
    mount();
    expect(screen.getByRole("heading", { name: copy.heading })).toBeTruthy();
    for (const line of [...copy.why, copy.pick, copy.cloudflare.uses]) expect(screen.getByText(line)).toBeTruthy();
    expect(screen.getAllByRole("button").map((b) => b.textContent)).toEqual([copy.ownFree]);
    expect(screen.queryByRole("link")).toBeNull();
    for (const item of copy.later.items) expect(screen.getByText(item).closest("button")).toBeNull();
    expect(screen.getByRole("heading", { name: copy.easyHeading })).toBeTruthy();
    expect(screen.getByRole("heading", { name: copy.advancedHeading })).toBeTruthy();
    expect(screen.getByText(copy.advancedIntro)).toBeTruthy();
  });

  it("choosing it shows the setup value, copies it, and links to the setup in a new tab that cannot reach back", async () => {
    const r = mount();
    await pickOwn();
    expect(await screen.findByText(HEX)).toBeTruthy();
    expect(screen.getByText(copy.cloudflare.protect)).toBeTruthy(); // R-164: steer to a phishing-resistant sign-in on the hosting account
    const link = screen.getByRole("link", { name: new RegExp(copy.cloudflare.open) });
    expect(link.getAttribute("href")).toBe(DEPLOY);
    expect(link.getAttribute("target")).toBe("_blank");
    expect(link.getAttribute("rel")).toContain("noopener");
    expect(link.getAttribute("rel")).toContain("noreferrer");
    await userEvent.click(screen.getByRole("button", { name: copy.setup.copy }));
    expect(r.written).toEqual([HEX]);
    expect(await screen.findByText(copy.setup.copied)).toBeTruthy();
  });

  it("says so when the value cannot be copied, and when it cannot be made", async () => {
    mount({ clipboard: false });
    await pickOwn();
    await userEvent.click(await screen.findByRole("button", { name: copy.setup.copy }));
    expect((await screen.findByRole("alert")).textContent).toBe(copy.setup.copyFailed);
    cleanup();
    mount({ prepare: () => Promise.reject(new Error("no storage")) });
    await pickOwn();
    expect((await screen.findByRole("alert")).textContent).toBe(copy.setup.prepareFailed);
    expect(screen.queryByRole("link")).toBeNull();
  });

  it("without a setup link the choice says it is not ready, and the address can still be typed", async () => {
    mount({ deployUrl: undefined });
    await pickOwn();
    expect(await screen.findByText(copy.cloudflare.notReady)).toBeTruthy();
    expect(screen.queryByRole("link")).toBeNull();
    expect(screen.getByLabelText(copy.setup.addressLabel)).toBeTruthy();
  });

  it("a healthy address is stored and first run continues; nothing is stored before it answers", async () => {
    const r = mount();
    await pickOwn();
    await userEvent.type(await screen.findByLabelText(copy.setup.addressLabel), " https://mine.example ");
    expect(r.connect).not.toHaveBeenCalled();
    await userEvent.click(screen.getByRole("button", { name: copy.setup.check }));
    await waitFor(() => { expect(r.done).toHaveBeenCalledTimes(1); });
    expect(r.probe).toHaveBeenCalledWith(" https://mine.example ");
    expect(r.connect).toHaveBeenCalledWith("https://mine.example");
  });

  it.each<[string, HomeProbeResult, string]>([
    ["unreachable", { kind: "unreachable" }, copy.result.unreachable],
    ["not a delivery service", { kind: "not_a_home" }, copy.result.notAHome],
    ["server error", { kind: "failing", status: 503 }, copy.result.failing(503)],
    ["wrong version", { kind: "wrong_version", offered: ["2"] }, copy.result.wrongVersion("2")],
    ["no mailbox", { kind: "no_mailbox" }, copy.result.noMailbox],
    ["bad address", { kind: "invalid_url", reason: "not_https" }, copy.invalid.not_https],
    ["not working", { kind: "ok", baseUrl: "https://mine.example", providerId: "mine", health: "unhealthy", admission: true }, copy.result.health.unhealthy],
    ["open to anyone (no admission)", { kind: "ok", baseUrl: "https://mine.example", providerId: "mine", health: "healthy", admission: false }, copy.setup.notLocked],
    ["not saying whether it asks for admission", { kind: "ok", baseUrl: "https://mine.example", providerId: "mine", health: "healthy" }, copy.setup.notLocked],
  ])("an address that is %s is explained and nothing is stored", async (_n, probe, said) => {
    const r = mount({ probe });
    await pickOwn();
    await userEvent.type(await screen.findByLabelText(copy.setup.addressLabel), "https://mine.example");
    await userEvent.click(screen.getByRole("button", { name: copy.setup.check }));
    expect((await screen.findByRole("alert")).textContent).toBe(said);
    expect(r.connect).not.toHaveBeenCalled();
    expect(r.done).not.toHaveBeenCalled();
  });

  it("keeps the typed address when saving fails, and does not continue", async () => {
    const r = mount({ connect: false });
    await pickOwn();
    const field = await screen.findByLabelText(copy.setup.addressLabel);
    await userEvent.type(field, "https://mine.example");
    await userEvent.click(screen.getByRole("button", { name: copy.setup.check }));
    expect((await screen.findByRole("alert")).textContent).toBe(copy.setup.saveFailed);
    expect((field as HTMLInputElement).value).toBe("https://mine.example");
    expect(r.done).not.toHaveBeenCalled();
  });

  it("the check button waits for an address; Back returns to page one", async () => {
    mount();
    await pickOwn();
    expect((await screen.findByRole<HTMLButtonElement>("button", { name: copy.setup.check })).disabled).toBe(true);
    await userEvent.click(screen.getByRole("button", { name: copy.setup.back }));
    expect(screen.getByRole("heading", { name: copy.heading })).toBeTruthy();
  });

  it("both pages are free of axe violations", async () => {
    mount();
    const bad = async (): Promise<string[]> => (await axe.run(document.body, { rules: { "color-contrast": { enabled: false }, region: { enabled: false } } })).violations.map((v) => `${v.id}: ${v.help}`);
    expect(await bad()).toEqual([]);
    await pickOwn();
    await screen.findByText(HEX);
    expect(await bad()).toEqual([]);
  });
});
