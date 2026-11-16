// @vitest-environment jsdom
import { cleanup, render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import axe from "axe-core";
import { afterEach, describe, expect, it } from "vitest";
import { asOperationId, asProfileId } from "@om/platform-api";
import { createHarness } from "@om/core-harness";
import { createActionRegistry } from "../actions/registry";
import type { SettingsServices } from "../devices/operation";
import { testProtectionCopy } from "./__fixtures__/copy";
import { ProtectionScreen } from "./index";

afterEach(cleanup);
const axeViolations = async (el: Element): Promise<string[]> => {
  const r = await axe.run(el, { rules: { "color-contrast": { enabled: false }, region: { enabled: false } } });
  return r.violations.map((v) => `${v.id}: ${v.help}`);
};
const ME = asProfileId("me");
function setup() {
  const h = createHarness({ env: {}, clock: () => 1000 });
  h.seedProfile({ profileId: ME, label: "Me" });
  const services: SettingsServices = { client: h, registry: createActionRegistry({ newOperationId: () => asOperationId("op") }), ctx: h.contextFor(ME) };
  return { h, services };
}
const level = (h: ReturnType<typeof setup>["h"], s: SettingsServices) => h.query({ kind: "GetPrivacyLevel", ctx: s.ctx });

describe("Settings > Protection", () => {
  it("shows Recommended as the level in use, with the three existing names and no axe violations", async () => {
    const { services } = setup();
    const { container } = render(<ProtectionScreen services={services} copy={testProtectionCopy} />);
    expect(await screen.findByRole("heading", { name: "Recommended" })).toBeTruthy();
    expect(screen.getByRole("heading", { name: "Hardened" })).toBeTruthy();
    expect(screen.getByRole("heading", { name: "Maximum Privacy" })).toBeTruthy();
    expect(screen.getByText("In use")).toBeTruthy();
    expect(screen.queryByRole("button", { name: "Use Recommended" })).toBeNull();
    expect(await axeViolations(container)).toEqual([]);
  });

  it("tightening is one tap and applies at once, with its cost said before the tap", async () => {
    const { h, services } = setup();
    render(<ProtectionScreen services={services} copy={testProtectionCopy} />);
    expect((await screen.findAllByText("You will be asked.")).length).toBe(2);
    await userEvent.click(screen.getByRole("button", { name: "Use Hardened" }));
    expect(await screen.findByText("Now using Hardened.")).toBeTruthy();
    expect(await level(h, services)).toBe("hardened");
    expect(screen.queryByText("Use Recommended instead?")).toBeNull();
  });

  it("loosening names what changes, needs its own confirmation, and keeping what you have changes nothing", async () => {
    const { h, services } = setup();
    await h.privacy.set(services.ctx, { kind: "set_level", level: "maximum" });
    const { container } = render(<ProtectionScreen services={services} copy={testProtectionCopy} />);
    await userEvent.click(await screen.findByRole("button", { name: "Use Recommended" }));
    expect(await screen.findByText("Use Recommended instead?")).toBeTruthy();
    expect(screen.getByText(/without asking you/)).toBeTruthy();
    expect(await level(h, services)).toBe("maximum"); // nothing yet
    expect(await axeViolations(container)).toEqual([]);
    await userEvent.click(screen.getByRole("button", { name: "Keep what I have now" }));
    expect(await level(h, services)).toBe("maximum");
    await userEvent.click(await screen.findByRole("button", { name: "Use Recommended" }));
    await userEvent.click(screen.getByRole("button", { name: "Yes, use Recommended" }));
    expect(await screen.findByText("Now using Recommended.")).toBeTruthy();
    expect(await level(h, services)).toBe("recommended");
  });

  it("an unconfirmed loosening is refused by the client itself, whatever the screen thinks", async () => {
    const { h, services } = setup();
    await h.privacy.set(services.ctx, { kind: "set_level", level: "hardened" });
    expect(await h.privacy.set(services.ctx, { kind: "set_level", level: "recommended" })).toEqual({ ok: false, refused: "loosening_needs_confirmation" });
    expect(await level(h, services)).toBe("hardened");
  });

  it("a suggestion (Omnibar, learned rank) can never set the level", () => {
    const { services } = setup();
    expect(services.registry.resolve({ actionId: "set-privacy-level", params: { level: "hardened" }, ctx: services.ctx, origin: "suggestion" })).toEqual({ status: "rejected", code: "suggestion_cannot_confirm" });
  });

  it("when the client cannot set a level, a tap says nothing changed", async () => {
    const { h, services } = setup();
    const noPrivacy: SettingsServices = { ...services, client: new Proxy(h, { get: (t, k) => (k === "privacy" ? undefined : Reflect.get(t, k) as unknown) }) };
    render(<ProtectionScreen services={noPrivacy} copy={testProtectionCopy} />);
    await userEvent.click(await screen.findByRole("button", { name: "Use Hardened" }));
    expect(await screen.findByRole("alert")).toHaveProperty("textContent", "Nothing changed.");
    expect(await level(h, services)).toBe("recommended");
  });
});
