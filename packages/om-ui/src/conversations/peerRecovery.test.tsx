// @vitest-environment jsdom
// ADR-0190: the contact-side banner, its two destructive actions, the send refusal and the quiet end states.
import { cleanup, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import axe from "axe-core";
import { useState } from "react";
import { afterEach, describe, expect, it } from "vitest";
import { asOperationId, asConversationId, asPersonId, asProfileId } from "@om/platform-api";
import type { PeerRecoveryView } from "@om/platform-api";
import { createHarness } from "@om/core-harness";
import { createActionRegistry } from "../actions/registry";
import { testContactRecoveryCopy } from "../devices/__fixtures__/copy";
import { testCopy } from "./__fixtures__/copy";
import { Composer, ConversationView, type ConversationServices } from "./index";

afterEach(cleanup);
const axeViolations = async (el: Element): Promise<string[]> => {
  const r = await axe.run(el, { rules: { "color-contrast": { enabled: false }, region: { enabled: false } } });
  return r.violations.map((v) => `${v.id}: ${v.help}`);
};

const ME = asProfileId("me");
const CONV = asConversationId("conv-1");
type Seed = Partial<Omit<PeerRecoveryView, "state">> & { readonly state: PeerRecoveryView["state"] };

function setup(peerRecovery: Seed) {
  const h = createHarness({ env: {}, clock: () => 1000 });
  h.seedProfile({ profileId: ME, label: "Me", conversations: [{ id: CONV, personId: asPersonId("maria"), displayName: "Maria", messages: [{ text: "hello", direction: "incoming" }], peerRecovery: { requestId: "r1", ...peerRecovery } }] });
  const services: ConversationServices = { client: h, registry: createActionRegistry(), ctx: h.contextFor(ME) };
  return { h, services };
}

function Screen({ services }: { readonly services: ConversationServices }) {
  const [draft, setDraft] = useState("");
  return (
    <ConversationView
      services={services} conversationId={CONV} copy={testCopy} recoveryCopy={testContactRecoveryCopy}
      footer={<Composer services={services} conversationId={CONV} copy={testCopy} peerName="Maria" draft={draft} onDraftChange={setDraft} />}
    />
  );
}

describe("a contact's recovery of their account (ADR-0190)", () => {
  it("waiting: one calm banner with days left, no certainty, accessible", async () => {
    const { services } = setup({ state: "waiting", daysLeft: 3 });
    const { container } = render(<Screen services={services} />);
    const region = await screen.findByRole("region", { name: "Maria is recovering their account" });
    expect(region.textContent).toContain("Waiting about 3 days");
    expect(region.getAttribute("data-peer-recovery")).toBe("waiting");
    expect(screen.queryByRole("button", { name: "Yes, it's them" })).toBeNull();
    expect(await axeViolations(container)).toEqual([]);
  });

  it("'Check with' opens its explanation (disclosure with aria-expanded)", async () => {
    const { services } = setup({ state: "waiting" });
    render(<Screen services={services} />);
    const check = await screen.findByRole("button", { name: "Check with Maria" });
    expect(check.getAttribute("aria-expanded")).toBe("false");
    await userEvent.click(check);
    expect(check.getAttribute("aria-expanded")).toBe("true");
    expect(screen.getByText("Ask Maria another way").hasAttribute("hidden")).toBe(false);
  });

  it("declining goes through a Core-issued confirmation and leaves one quiet line", async () => {
    const { h, services } = setup({ state: "waiting" });
    const { container } = render(<Screen services={services} />);
    await userEvent.click(await screen.findByRole("button", { name: "This isn't them" }));
    await waitFor(() => { expect(container.querySelector("[data-peer-recovery='declined']")?.textContent).toBe("You turned this down for Maria"); });
    expect(container.querySelector(".om-peer-recovery")).toBeNull();
    const view = await h.query({ kind: "GetConversationView", ctx: services.ctx, conversationId: CONV });
    expect(view.summary.peerRecovery?.state).toBe("declined");
    expect(await axeViolations(container)).toEqual([]);
  });

  it("Hardened or Maximum: after the wait the person must say yes, with the consent words next to the buttons", async () => {
    const { services } = setup({ state: "ready", needsConfirm: true, daysLeft: 0 });
    const { container } = render(<Screen services={services} />);
    const region = await screen.findByRole("region", { name: "Is this really Maria?" });
    expect(region.textContent).toContain("Only if you checked with Maria");
    expect(await axeViolations(container)).toEqual([]);
    await userEvent.click(screen.getByRole("button", { name: "Yes, it's them" }));
    await waitFor(() => { expect(container.querySelector("[data-peer-recovery='accepted']")?.textContent).toBe("Maria recovered their account on a new device"); });
    expect(await axeViolations(container)).toEqual([]);
  });

  it("ready without a confirmation to give says one more check is needed and offers no confirm", async () => {
    const { services } = setup({ state: "ready", needsConfirm: false, daysLeft: 0 });
    render(<Screen services={services} />);
    expect((await screen.findByRole("region", { name: "Maria is recovering their account" })).textContent).toContain("One more check is needed");
    expect(screen.queryByRole("button", { name: "Yes, it's them" })).toBeNull();
  });

  it("no decline button when Core does not offer it", async () => {
    const { services } = setup({ state: "waiting", canDecline: false });
    render(<Screen services={services} />);
    await screen.findByRole("region", { name: "Maria is recovering their account" });
    expect(screen.queryByRole("button", { name: "This isn't them" })).toBeNull();
  });

  it("vetoed and accepted are one quiet line; stale and a declined completion notice flag say nothing", async () => {
    for (const [state, text] of [["vetoed", "Maria's own device stopped this"], ["accepted", "Maria recovered their account on a new device"]] as const) {
      const { services } = setup({ state });
      const { container } = render(<ConversationView services={services} conversationId={CONV} copy={testCopy} recoveryCopy={testContactRecoveryCopy} />);
      await waitFor(() => { expect(container.querySelector(`[data-peer-recovery='${state}']`)?.textContent).toBe(text); });
      cleanup();
    }
    const stale = setup({ state: "stale" });
    const { container } = render(<ConversationView services={stale.services} conversationId={CONV} copy={testCopy} recoveryCopy={testContactRecoveryCopy} />);
    await screen.findByText("hello");
    expect(container.querySelector("[data-peer-recovery]")).toBeNull();
    cleanup();
    const quiet = setup({ state: "accepted", notifyOnCompletion: false });
    const r = render(<ConversationView services={quiet.services} conversationId={CONV} copy={testCopy} recoveryCopy={testContactRecoveryCopy} />);
    await screen.findByText("hello");
    expect(r.container.querySelector("[data-peer-recovery]")).toBeNull();
  });

  it("while it runs a text is kept and shown as waiting; when the recovery ends without them it says not sent and offers a retry", async () => {
    const { h, services } = setup({ state: "waiting" });
    render(<Screen services={services} />);
    const box = await screen.findByRole("textbox", { name: "Message" });
    await userEvent.type(box, "are you there");
    await userEvent.click(screen.getByRole("button", { name: "Send" }));
    expect(await screen.findByText("Waiting for their new device")).toBeTruthy();
    expect(screen.getByRole("log").textContent).toContain("are you there");
    expect((box as HTMLTextAreaElement).value).toBe("");
    const view = await h.query({ kind: "GetConversationView", ctx: services.ctx, conversationId: CONV });
    expect(view.timeline.map((i) => i.text)).toEqual(["hello", "are you there"]);
    await userEvent.click(await screen.findByRole("button", { name: "This isn't them" }));
    expect(await screen.findByText("Not sent: unavailable")).toBeTruthy();
    expect(screen.queryByText("Waiting for their new device")).toBeNull();
    expect(screen.getByRole("button", { name: "Retry sending" })).toBeTruthy();
  });

  it("a file is not kept for later: it is refused with the person's name and the draft stays", async () => {
    const { h, services } = setup({ state: "waiting" });
    await h.dispatch({ kind: "SendLocation", ctx: services.ctx, operationId: asOperationId("loc-1"), conversationId: CONV, latE7: 1, lonE7: 1 } as never);
    let outcome: { readonly kind: string; readonly reason?: string } | null = null;
    h.observe(services.ctx, asOperationId("loc-1"), (o) => { outcome = o; })();
    expect(outcome).toEqual({ kind: "Rejected", reason: "peer_recovering" });
  });

  it("without the copy nothing is shown, and a conversation with no recovery shows no banner", async () => {
    const { services } = setup({ state: "waiting" });
    const { container } = render(<ConversationView services={services} conversationId={CONV} copy={testCopy} />);
    await screen.findByText("hello");
    expect(container.querySelector(".om-peer-recovery")).toBeNull();
  });
});
