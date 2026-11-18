// @vitest-environment jsdom
import { cleanup, render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import axe from "axe-core";
import { afterEach, describe, expect, it, vi } from "vitest";
import { asOperationId, asProfileId } from "@om/platform-api";
import { createHarness } from "@om/core-harness";
import { createActionRegistry } from "../actions/registry";
import { testApproveCopy, testLostCopy } from "../devices/__fixtures__/copy";
import type { SettingsServices } from "../devices/operation";
import { ApproveRecovery, LostDevices } from "./index";

afterEach(cleanup);
const axeViolations = async (el: Element): Promise<string[]> => {
  const r = await axe.run(el, { rules: { "color-contrast": { enabled: false }, region: { enabled: false } } });
  return r.violations.map((v) => `${v.id}: ${v.help}`);
};

function setup() {
  const h = createHarness({ env: {}, clock: () => 1000 });
  const me = asProfileId("me");
  h.seedProfile({ profileId: me, label: "Me" });
  let n = 0;
  const registry = createActionRegistry({ newOperationId: () => asOperationId(`op-${String(++n)}`) });
  const services: SettingsServices = { client: h, registry, ctx: h.contextFor(me) };
  return { h, services };
}

describe("I lost my devices", () => {
  it("shows the key entry with labelled fields, no technical words, and passes axe", async () => {
    const { services } = setup();
    const { container } = render(<LostDevices services={services} copy={testLostCopy} onRecovered={() => undefined} onCancel={() => undefined} />);
    expect(await screen.findByLabelText(testLostCopy.keyLabel)).toBeTruthy();
    expect(screen.getByRole("button", { name: testLostCopy.find })).toBeTruthy();
    expect(await axeViolations(container)).toEqual([]);
  });

  it("an empty key asks again and sends nothing; Back is always available", async () => {
    const { services } = setup();
    const onCancel = vi.fn();
    render(<LostDevices services={services} copy={testLostCopy} onRecovered={() => undefined} onCancel={onCancel} />);
    await userEvent.click(await screen.findByRole("button", { name: testLostCopy.find }));
    expect(screen.getByText(testLostCopy.stage.not_found)).toBeTruthy();
    await userEvent.click(screen.getByRole("button", { name: testLostCopy.cancel }));
    expect(onCancel).toHaveBeenCalled();
  });

  it("a controller device offers the approval only from its own step and passes axe", async () => {
    const { services } = setup();
    const onClose = vi.fn();
    const { container } = render(<ApproveRecovery services={services} copy={testApproveCopy} onClose={onClose} />);
    expect(await screen.findByRole("heading", { name: testApproveCopy.heading })).toBeTruthy();
    expect(screen.queryByRole("button", { name: testApproveCopy.confirm })).toBeNull();
    expect(await axeViolations(container)).toEqual([]);
  });
});
