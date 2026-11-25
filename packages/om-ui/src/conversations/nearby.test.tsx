// @vitest-environment jsdom
// "Find each other on the same Wi-Fi" in the contact sheet: only when a host supplies the service, off by default, shows what Core
// reports (never what was tapped), says why it cannot be changed, and fails visibly.
import { cleanup, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import axe from "axe-core";
import { afterEach, describe, expect, it } from "vitest";
import { asConversationId, asOperationId, asPersonId, asProfileId } from "@om/platform-api";
import { createHarness } from "@om/core-harness";
import { createActionRegistry } from "../actions/registry";
import { testCopy } from "./__fixtures__/copy";
import { ConversationView, type ConversationServices, type NearbyService, type NearbyView } from "./index";

afterEach(cleanup);
const axeViolations = async (el: Element): Promise<string[]> => {
  const r = await axe.run(el, { rules: { "color-contrast": { enabled: false }, region: { enabled: false } } });
  return r.violations.map((v) => `${v.id}: ${v.help}`);
};

const ME = asProfileId("me");
const A = asConversationId("c-a");

function setup(nearby?: NearbyService) {
  const h = createHarness({ env: {}, clock: () => 1000 });
  h.seedProfile({ profileId: ME, label: "Me", conversations: [{ id: A, personId: asPersonId("pa"), displayName: "Alex Doe", messages: [] }] });
  let n = 0;
  const registry = createActionRegistry({ newOperationId: () => asOperationId(`op-${String(++n)}`) });
  const services: ConversationServices = { client: h, registry, ctx: h.contextFor(ME), ...(nearby === undefined ? {} : { nearby }) };
  return services;
}

function fake(initial: NearbyView, platform: NearbyService["platform"] = "available", rule?: (on: boolean, now: NearbyView) => NearbyView | "reject") {
  let now = initial;
  const calls: boolean[] = [];
  const service: NearbyService = {
    platform,
    state: () => Promise.resolve(now),
    set: (_id, on) => {
      calls.push(on);
      const r = rule ? rule(on, now) : { ...now, enabled: on };
      if (r === "reject") return Promise.reject(new Error("unreachable"));
      now = r;
      return Promise.resolve(r);
    },
  };
  return { service, calls };
}

const NAME = /Find each other on the same Wi-Fi/;
const nearbySwitch = (sheet: HTMLElement): Promise<HTMLInputElement> => within(sheet).findByRole<HTMLInputElement>("switch", { name: NAME });

async function openSheet(services: ConversationServices) {
  render(<ConversationView services={services} conversationId={A} copy={testCopy} />);
  await userEvent.click(await screen.findByRole("button", { name: "About Alex Doe" }));
  return screen.findByRole("dialog", { name: "About Alex Doe" });
}

describe("find each other on the same Wi-Fi", () => {
  it("is not offered when the host supplies nothing", async () => {
    const sheet = await openSheet(setup());
    expect(within(sheet).queryByText("Find each other on the same Wi-Fi")).toBeNull();
  });

  it("is off by default, turns on only as Core says, says the cost, and is accessible", async () => {
    const { service, calls } = fake({ enabled: false, available: true });
    const sheet = await openSheet(setup(service));
    const toggle = await nearbySwitch(sheet);
    expect(toggle.checked).toBe(false);
    expect(within(sheet).getByText(/They can tell when you are nearby/)).toBeTruthy();
    expect(within(sheet).getByText("Off.")).toBeTruthy();
    expect(await axeViolations(sheet)).toEqual([]);
    await userEvent.click(toggle);
    await waitFor(() => { expect(toggle.checked).toBe(true); });
    expect(calls).toEqual([true]);
    expect(within(sheet).getByText("On for this person.")).toBeTruthy();
    await userEvent.click(toggle);
    await waitFor(() => { expect(toggle.checked).toBe(false); });
    expect(calls).toEqual([true, false]);
  });

  it("shows what Core answers, not what was tapped", async () => {
    const { service } = fake({ enabled: false, available: true }, "available", () => ({ enabled: false, available: false }));
    const sheet = await openSheet(setup(service));
    const toggle = await nearbySwitch(sheet);
    await userEvent.click(toggle);
    await within(sheet).findByText(/Not available right now/);
    expect(toggle.checked).toBe(false);
    expect(toggle.disabled).toBe(true);
  });

  it("cannot be turned on when Core does not allow it, and says so in words", async () => {
    const { service, calls } = fake({ enabled: false, available: false });
    const sheet = await openSheet(setup(service));
    const toggle = await nearbySwitch(sheet);
    expect(toggle.disabled).toBe(true);
    expect(within(sheet).getByText(/Not available right now/)).toBeTruthy();
    await userEvent.click(toggle);
    expect(calls).toEqual([]);
    expect(await axeViolations(sheet)).toEqual([]);
  });

  it("can always be turned off, even when it could not be turned on now", async () => {
    const { service, calls } = fake({ enabled: true, available: false });
    const sheet = await openSheet(setup(service));
    const toggle = await nearbySwitch(sheet);
    expect(toggle.disabled).toBe(false);
    await userEvent.click(toggle);
    await waitFor(() => { expect(calls).toEqual([false]); });
  });

  it("says when this device cannot do it, and when it can only in part", async () => {
    const no = fake({ enabled: false, available: true }, "unavailable");
    const sheetNo = await openSheet(setup(no.service));
    expect((await nearbySwitch(sheetNo)).disabled).toBe(true);
    expect(within(sheetNo).getByText("This device can’t do this.")).toBeTruthy();
    cleanup();
    const some = fake({ enabled: false, available: true }, "partial");
    const sheetSome = await openSheet(setup(some.service));
    expect((await nearbySwitch(sheetSome)).disabled).toBe(false);
    expect(within(sheetSome).getByText("This device may not find them every time.")).toBeTruthy();
  });

  it("fails visibly and leaves the switch where it was", async () => {
    const { service } = fake({ enabled: false, available: true }, "available", () => "reject");
    const sheet = await openSheet(setup(service));
    const toggle = await nearbySwitch(sheet);
    await userEvent.click(toggle);
    expect((await within(sheet).findByRole("alert")).textContent).toBe("Couldn’t change this setting.");
    expect(toggle.checked).toBe(false);
  });
});
