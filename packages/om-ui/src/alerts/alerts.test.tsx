// @vitest-environment jsdom
import { cleanup, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import axe from "axe-core";
import { afterEach, describe, expect, it, vi } from "vitest";
import { asOperationId, asProfileId, type ChosenPrivacyLevel } from "@om/platform-api";
import { createHarness } from "@om/core-harness";
import { createActionRegistry } from "../actions/registry";
import type { SettingsServices } from "../devices/operation";
import { testAlertsCopy } from "./__fixtures__/copy";
import { AlertsScreen, type AlertsServices } from "./index";

afterEach(cleanup);
const axeViolations = async (el: Element): Promise<string[]> => (await axe.run(el, { rules: { "color-contrast": { enabled: false }, region: { enabled: false } } })).violations.map((v) => `${v.id}: ${v.help}`);
const ME = asProfileId("me");
let seq = 0;
async function core(level?: ChosenPrivacyLevel) {
  const h = createHarness({ env: {}, clock: () => 1000 });
  h.seedProfile({ profileId: ME, label: "Me" });
  const services: SettingsServices = { client: h, registry: createActionRegistry({ newOperationId: () => asOperationId(`op-${String(++seq)}`) }), ctx: h.contextFor(ME) };
  if (level !== undefined && level !== "recommended") {
    const { confirmWithCore } = await import("../actions/confirm");
    const params = { policy: level };
    const issued = await confirmWithCore(services, "change-privacy-policy", params);
    if (issued.status !== "issued") throw new Error("no nonce");
    const r = services.registry.resolve({ actionId: "change-privacy-policy", params, ctx: services.ctx, origin: "direct", confirmation: issued.token });
    if (r.status !== "intent") throw new Error("intent");
    const handle = await h.dispatch(r.intent);
    await new Promise<void>((resolve) => { const off = h.observe(services.ctx, handle.operationId, () => { resolve(); off(); }); });
  }
  return services;
}
function alerts(over: { enable?: AlertsServices["enable"]; on?: boolean; maxOk?: boolean; why?: Awaited<ReturnType<AlertsServices["availability"]>> } = {}) {
  let on = over.on ?? false;
  const calls: string[] = [];
  const a: AlertsServices = {
    availability: () => Promise.resolve(over.why ?? "ready"),
    status: () => Promise.resolve({ on, permission: "default", maxOk: over.maxOk ?? false }),
    enable: over.enable ?? ((o) => { calls.push(`enable max=${String(o.confirmedMaximum)}`); on = true; return Promise.resolve("enabled"); }),
    disable: () => { calls.push("disable"); on = false; return Promise.resolve(); },
    refresh: () => Promise.resolve(),
  };
  return { a, calls };
}

describe("Settings > Alerts", () => {
  it("explains what it does, who learns what, the iPhone condition and its limit, with no axe violations", async () => {
    const { a } = alerts();
    const { container } = render(<AlertsScreen services={await core()} alerts={a} copy={testAlertsCopy} />);
    expect(await screen.findByText(testAlertsCopy.off)).toBeTruthy();
    for (const t of [testAlertsCopy.intro, testAlertsCopy.privacy, testAlertsCopy.ios, testAlertsCopy.limit]) expect(screen.getByText(t)).toBeTruthy();
    expect(await axeViolations(container)).toEqual([]);
  });
  it("is off until the person turns it on, then on and says so", async () => {
    const { a, calls } = alerts();
    render(<AlertsScreen services={await core()} alerts={a} copy={testAlertsCopy} />);
    await userEvent.click(await screen.findByRole("button", { name: testAlertsCopy.turnOn }));
    expect(await screen.findByText(testAlertsCopy.turnedOn)).toBeTruthy();
    expect(calls).toEqual(["enable max=false"]);
    await userEvent.click(screen.getByRole("button", { name: testAlertsCopy.turnOff }));
    expect(await screen.findByText(testAlertsCopy.turnedOff)).toBeTruthy();
    expect(calls).toEqual(["enable max=false", "disable"]);
  });
  it("at Maximum Privacy asks first, says the cost, and registers nothing until confirmed", async () => {
    const { a, calls } = alerts();
    render(<AlertsScreen services={await core("maximum")} alerts={a} copy={testAlertsCopy} />);
    await userEvent.click(await screen.findByRole("button", { name: testAlertsCopy.turnOn }));
    expect(await screen.findByText(testAlertsCopy.maximum.body)).toBeTruthy();
    expect(calls).toEqual([]);
    await userEvent.click(screen.getByRole("button", { name: testAlertsCopy.maximum.keepOff }));
    expect(screen.queryByText(testAlertsCopy.maximum.body)).toBeNull();
    expect(calls).toEqual([]);
    await userEvent.click(screen.getByRole("button", { name: testAlertsCopy.turnOn }));
    await userEvent.click(await screen.findByRole("button", { name: testAlertsCopy.maximum.confirm }));
    await waitFor(() => { expect(calls).toEqual(["enable max=true"]); });
  });
  it("alerts that are on while protection is Maximum and unconfirmed are shown as paused, with the choice to keep or turn off", async () => {
    const { a, calls } = alerts({ on: true, maxOk: false });
    render(<AlertsScreen services={await core("maximum")} alerts={a} copy={testAlertsCopy} />);
    expect(await screen.findByText(testAlertsCopy.paused.body)).toBeTruthy();
    await userEvent.click(screen.getByRole("button", { name: testAlertsCopy.paused.keep }));
    await waitFor(() => { expect(calls).toEqual(["enable max=true"]); });
  });
  it("shows why alerts are unavailable, and offers no switch", async () => {
    for (const why of ["no_browser", "needs_install", "no_home", "many_profiles"] as const) {
      cleanup();
      const { a } = alerts({ why });
      render(<AlertsScreen services={await core()} alerts={a} copy={testAlertsCopy} />);
      expect(await screen.findByText(testAlertsCopy.unavailable[why])).toBeTruthy();
      expect(screen.queryByRole("button", { name: testAlertsCopy.turnOn })).toBeNull();
    }
  });
  it("says plainly when the browser blocks notifications or the Home does not take the device", async () => {
    for (const result of ["denied", "refused", "full", "unreachable", "failed"] as const) {
      cleanup();
      const { a } = alerts({ enable: vi.fn(() => Promise.resolve(result)) });
      render(<AlertsScreen services={await core()} alerts={a} copy={testAlertsCopy} />);
      await userEvent.click(await screen.findByRole("button", { name: testAlertsCopy.turnOn }));
      expect(await screen.findByRole("alert")).toHaveProperty("textContent", testAlertsCopy.failure[result]);
      expect(screen.getByText(testAlertsCopy.off)).toBeTruthy();
    }
  });
});
