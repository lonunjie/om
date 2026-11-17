// @vitest-environment jsdom
import { cleanup, render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import axe from "axe-core";
import { afterEach, describe, expect, it } from "vitest";
import { asOperationId, asProfileId, type OutcomeClass } from "@om/platform-api";
import { createHarness } from "@om/core-harness";
import { confirmWithCore } from "../actions/confirm";
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
let opSeq = 0;
function setup() {
  const h = createHarness({ env: {}, clock: () => 1000 });
  h.seedProfile({ profileId: ME, label: "Me" });
  const services: SettingsServices = { client: h, registry: createActionRegistry({ newOperationId: () => asOperationId(`op-${String(++opSeq)}`) }), ctx: h.contextFor(ME) };
  return { h, services };
}
const level = (h: ReturnType<typeof setup>["h"], s: SettingsServices) => h.query({ kind: "GetPrivacyLevel", ctx: s.ctx });
/** What a person's own tap does behind the screen: a Core nonce for exactly this level, then the intent. */
async function change(s: SettingsServices, policy: "recommended" | "hardened" | "maximum", nonceFrom?: { readonly nonce: unknown }): Promise<OutcomeClass> {
  const params = { policy };
  const issued = await confirmWithCore(s, "change-privacy-policy", params);
  if (issued.status !== "issued") throw new Error("no nonce");
  const r = s.registry.resolve({ actionId: "change-privacy-policy", params, ctx: s.ctx, origin: "direct", confirmation: issued.token });
  if (r.status !== "intent") throw new Error("not an intent");
  const intent = nonceFrom === undefined ? r.intent : ({ ...r.intent, confirmation: nonceFrom } as typeof r.intent);
  const handle = await s.client.dispatch(intent);
  return await new Promise<OutcomeClass>((resolve) => { const off = s.client.observe(s.ctx, handle.operationId, (o) => { resolve(o); off(); }); });
}

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
    expect(await change(services, "maximum")).toEqual({ kind: "Committed" });
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

  it("a loosening needs a Core nonce: a forged, reused or stale one changes nothing", async () => {
    const { h, services } = setup();
    expect(await change(services, "hardened")).toEqual({ kind: "Committed" });
    expect(await change(services, "recommended", { nonce: "00".repeat(16) })).toEqual({ kind: "Rejected", reason: "unauthorized" });
    expect(await level(h, services)).toBe("hardened");
    // A nonce issued for a tightening is stale once the level moved up: it can never apply as a loosening.
    const issued = await confirmWithCore(services, "change-privacy-policy", { policy: "maximum" });
    if (issued.status !== "issued") throw new Error("no nonce");
    expect(await change(services, "maximum")).toEqual({ kind: "Committed" });
    expect(await level(h, services)).toBe("maximum");
    const r = services.registry.resolve({ actionId: "change-privacy-policy", params: { policy: "maximum" }, ctx: services.ctx, origin: "direct", confirmation: issued.token });
    if (r.status !== "intent") throw new Error("not an intent");
    const handle = await services.client.dispatch(r.intent);
    const o = await new Promise<OutcomeClass>((resolve) => { const off = services.client.observe(services.ctx, handle.operationId, (x) => { resolve(x); off(); }); });
    expect(o).toEqual({ kind: "Rejected", reason: "conflict" });
    expect(await level(h, services)).toBe("maximum");
  });

  it("a suggestion (Omnibar, learned rank) can never set the level", () => {
    const { services } = setup();
    expect(services.registry.prepareConfirmation({ actionId: "change-privacy-policy", params: { policy: "hardened" }, ctx: services.ctx, origin: "suggestion" })).toEqual({ status: "rejected", code: "suggestion_cannot_confirm" });
  });

  it("a refused change says nothing changed and leaves the level as it was", async () => {
    const { h, services } = setup();
    const refusing: SettingsServices = { ...services, client: new Proxy(h, { get: (t, k) => (k === "dispatch" ? (i: { kind: string }) => (i.kind === "ChangePrivacyPolicy" ? Promise.reject(new Error("down")) : t.dispatch(i as never)) : (Reflect.get(t, k) as unknown)) }) };
    render(<ProtectionScreen services={refusing} copy={testProtectionCopy} />);
    await userEvent.click(await screen.findByRole("button", { name: "Use Hardened" }));
    expect(await screen.findByRole("alert")).toHaveProperty("textContent", "Nothing changed.");
    expect(await level(h, services)).toBe("recommended");
  });
});
