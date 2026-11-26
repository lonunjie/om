// @vitest-environment jsdom
// SPDX-License-Identifier: MPL-2.0
import { cleanup, render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import axe from "axe-core";
import { afterEach, describe, expect, it, vi } from "vitest";
import { asConversationId } from "@om/platform-api";
import { ConnectedServices, ServiceAuthorship } from "./index";
import type {
  ConnectedServicesService,
  ConnectedServicesView,
  ServiceEntryView,
} from "./index";
import { servicesCopy as copy } from "./__fixtures__/copy";
import {
  fullMember,
  fullMemberOffer,
  invocation,
  servicePrincipal,
} from "./__fixtures__/service";

afterEach(cleanup);

const room = asConversationId("weekend-plans");
const snapshot: ConnectedServicesView = {
  entries: [fullMemberOffer],
  outcome: null,
  invocation: null,
};

function fake(initial = snapshot) {
  const state = vi
    .fn<ConnectedServicesService["state"]>()
    .mockResolvedValue(initial);
  const change = vi
    .fn<ConnectedServicesService["change"]>()
    .mockResolvedValue(initial);
  return { state, change };
}

function show(service: ConnectedServicesService) {
  return render(
    <ConnectedServices conversationId={room} service={service} copy={copy} />,
  );
}

async function reviewService() {
  await userEvent.click(
    await screen.findByRole("button", { name: "Review Agenda assistant" }),
  );
  return screen.getByRole("dialog", { name: "Review service" });
}

async function reviewInvocation() {
  await userEvent.click(
    await screen.findByRole("button", {
      name: "Review what to share with Agenda assistant",
    }),
  );
  return screen.getByRole("dialog", { name: "Review what will be shared" });
}

async function axeViolations(element: HTMLElement) {
  const result = await axe.run(element, {
    rules: { "color-contrast": { enabled: false }, region: { enabled: false } },
  });
  return result.violations.map(({ id, help }) => `${id}: ${help}`);
}

describe("connected services", () => {
  it("omits the surface without an injected service", () => {
    const { container } = render(<ConnectedServices copy={copy} />);
    expect(container.children).toHaveLength(0);
  });

  it("reviews exact requested capabilities and full-member access before sending an install intent", async () => {
    const service = fake();
    service.change.mockResolvedValue({
      ...snapshot,
      entries: [fullMember],
      outcome: { kind: "Committed" },
    });
    const { container } = show(service);
    const user = userEvent.setup();
    const review = await screen.findByRole("button", {
      name: "Review Agenda assistant",
    });
    await user.tab();
    expect(document.activeElement).toBe(review);
    await user.keyboard("{Enter}");
    const sheet = screen.getByRole("dialog", { name: "Review service" });
    expect(document.activeElement).toBe(
      within(sheet).getByRole("heading", { name: "Agenda assistant" }),
    );
    expect(within(sheet).getByText("Full Room member")).toBeTruthy();
    expect(
      within(sheet).getByText(copy.modeConsequence.full_room_member),
    ).toBeTruthy();
    expect(within(sheet).getByText("Requested permissions")).toBeTruthy();
    expect(
      within(sheet)
        .getAllByRole("listitem")
        .map((item) => item.textContent),
    ).toEqual(["Post messages"]);
    expect(service.change).not.toHaveBeenCalled();
    expect(await axeViolations(sheet)).toEqual([]);

    await user.keyboard("{Escape}");
    expect(document.activeElement).toBe(review);
    await user.keyboard("{Enter}");
    const confirm = screen.getByRole("button", { name: "Install service" });
    confirm.focus();
    await user.keyboard("{Enter}");
    expect(service.change).toHaveBeenCalledExactlyOnceWith(room, {
      kind: "install",
      target: "room-offer-1",
    });
    await screen.findByText("Installed");
    expect(document.activeElement).toBe(
      screen.getByRole("heading", { name: "Connected services" }),
    );
    expect(
      screen.getByText(copy.modeConsequence.full_room_member),
    ).toBeTruthy();
    expect(screen.getByText(copy.triggerFilter)).toBeTruthy();
    expect(await axeViolations(container)).toEqual([]);
  });

  it.each([
    [
      "selected_context",
      { kind: "space", name: "Book club" },
      "Invocation only",
    ],
    ["local_personal", { kind: "personal" }, "Local component"],
  ] as const)(
    "shows %s and displays a denied action with a reason",
    async (mode, scope, label) => {
      const offer: ServiceEntryView = {
        ...fullMemberOffer,
        mode,
        scope,
        capabilities: [],
        actions: [
          { availability: "blocked", kind: "install", reason: "policy_denied" },
        ],
      };
      const service = fake({ ...snapshot, entries: [offer] });
      show(service);
      const sheet = await reviewService();
      expect(within(sheet).getByText(label)).toBeTruthy();
      expect(within(sheet).getByText(copy.modeConsequence[mode])).toBeTruthy();
      expect(within(sheet).getByText(copy.noCapabilities)).toBeTruthy();
      if (scope.kind === "space")
        expect(within(sheet).getByText(copy.spaceBoundary)).toBeTruthy();
      const confirm = within(sheet).getByRole<HTMLButtonElement>("button", {
        name: "Install service",
      });
      expect(confirm.disabled).toBe(true);
      expect(confirm.getAttribute("aria-describedby")).toBe(
        within(sheet).getByText(copy.rejected.policy_denied).id,
      );
      await userEvent.click(confirm);
      expect(service.change).not.toHaveBeenCalled();
      expect(await axeViolations(sheet)).toEqual([]);
    },
  );

  it("keeps effective Room read access visible after revoke until Core reports removal", async () => {
    const service = fake({ ...snapshot, entries: [fullMember] });
    const revoked: ServiceEntryView = {
      ...fullMember,
      status: "revoked",
      actions: fullMember.actions.slice(1),
    };
    service.change
      .mockResolvedValueOnce({
        ...snapshot,
        entries: [revoked],
        outcome: { kind: "Committed" },
      })
      .mockResolvedValueOnce({
        ...snapshot,
        entries: [
          {
            ...revoked,
            status: "removed",
            readAccess: { kind: "none" },
            capabilities: [],
            actions: [],
          },
        ],
        outcome: { kind: "Committed" },
      });
    show(service);
    const review = await reviewService();
    expect(within(review).getByText(copy.access)).toBeTruthy();
    expect(within(review).getByText("Ari")).toBeTruthy();
    expect(within(review).getByText("Laptop")).toBeTruthy();
    await userEvent.keyboard("{Escape}");
    const card = screen.getByRole("article", { name: "Agenda assistant" });
    await userEvent.click(
      within(card).getByRole("button", { name: "Revoke access" }),
    );
    await screen.findByText("Permission revoked");
    expect(
      screen.getByText(copy.modeConsequence.full_room_member),
    ).toBeTruthy();
    const remove = await reviewService();
    expect(
      within(remove).getByText(
        new RegExp(copy.previouslyShared.replace(/\.$/u, "")),
      ),
    ).toBeTruthy();
    await userEvent.click(
      within(remove).getByRole("button", { name: "Remove service" }),
    );
    await screen.findByText("Removed");
    expect(
      screen.queryByText(copy.modeConsequence.full_room_member),
    ).toBeNull();
    expect(service.change.mock.calls).toEqual([
      [room, { kind: "revoke", target: "grant-1" }],
      [room, { kind: "remove", target: "room-installation-1" }],
    ]);
  });

  it("shows exactly what leaves in order, and keeps one-time disclosure separate from an egress denial", async () => {
    const service = fake({ ...snapshot, entries: [], invocation });
    show(service);
    const sheet = await reviewInvocation();
    expect(within(sheet).getByText("Who receives this")).toBeTruthy();
    expect(
      within(sheet).getByText("Agenda assistant", {
        exact: true,
        selector: "dd bdi",
      }),
    ).toBeTruthy();
    expect(
      within(sheet)
        .getAllByRole("listitem")
        .map((item) => item.textContent),
    ).toEqual([
      "Triggering message@Agenda assistant summarize this message",
      "Selected messageMeet at the station at 9.",
    ]);
    expect(within(sheet).getByText(copy.tainted)).toBeTruthy();
    expect(
      within(sheet).getByText(copy.egress.blocked_by_conversation),
    ).toBeTruthy();
    expect(service.change).not.toHaveBeenCalled();
    expect(await axeViolations(sheet)).toEqual([]);
    await userEvent.click(
      within(sheet).getByRole("button", { name: "Share and run" }),
    );
    expect(service.change).toHaveBeenCalledExactlyOnceWith(room, {
      kind: "invoke",
      target: "example-disclosure-1",
    });
  });

  it("shows a local invocation and broker refusal without a remote disclosure or enabled send", async () => {
    const service = fake({
      ...snapshot,
      entries: [],
      invocation: {
        ...invocation,
        disclosure: { kind: "local" },
        flow: { tainted: false, egress: "blocked_by_capability" },
        action: {
          availability: "blocked",
          kind: "invoke",
          reason: "policy_denied",
        },
      },
    });
    show(service);
    const sheet = await reviewInvocation();
    expect(within(sheet).getByText(copy.localDisclosure)).toBeTruthy();
    expect(within(sheet).queryByText(copy.destination)).toBeNull();
    expect(within(sheet).getByText(copy.untainted)).toBeTruthy();
    expect(
      within(sheet).getByText(copy.egress.blocked_by_capability),
    ).toBeTruthy();
    expect(
      within(sheet).getByRole<HTMLButtonElement>("button", {
        name: "Run service",
      }).disabled,
    ).toBe(true);
  });

  it("ignores an old Room lookup after the Room changes", async () => {
    let resolveOld: ((view: ConnectedServicesView) => void) | undefined;
    const service = fake({ ...snapshot, entries: [] });
    service.state.mockReturnValueOnce(
      new Promise((resolve) => {
        resolveOld = resolve;
      }),
    );
    const { rerender } = show(service);
    expect(screen.getByRole("status").textContent).toBe(copy.loading);
    rerender(
      <ConnectedServices
        conversationId={asConversationId("other-room")}
        service={service}
        copy={copy}
      />,
    );
    await screen.findByText(copy.empty);
    resolveOld?.(snapshot);
    expect(
      screen.queryByRole("button", { name: "Review Agenda assistant" }),
    ).toBeNull();
  });

  it("shows slow and unreachable reads, then retries the read", async () => {
    const service = fake();
    service.state.mockRejectedValueOnce(new Error("Internal details"));
    show(service);
    expect(screen.getByRole("status").textContent).toBe(copy.loading);
    expect((await screen.findByRole("alert")).textContent).toBe(
      copy.loadFailed,
    );
    await userEvent.click(screen.getByRole("button", { name: copy.refresh }));
    await screen.findByRole("button", { name: "Review Agenda assistant" });
    expect(service.state.mock.calls).toEqual([[room], [room]]);
    expect(service.change).not.toHaveBeenCalled();
  });

  it("renders a Core refusal without inventing an installed state", async () => {
    const service = fake();
    service.change.mockResolvedValueOnce({
      ...snapshot,
      outcome: { kind: "Rejected", reason: "stale_state" },
    });
    show(service);
    const sheet = await reviewService();
    await userEvent.click(
      within(sheet).getByRole("button", { name: "Install service" }),
    );
    await screen.findByText(copy.rejected.stale_state);
    expect(screen.getByText("Available to install")).toBeTruthy();
    expect(screen.queryByText("Installed")).toBeNull();
  });

  it("rechecks an unknown or pending action without sending a second operation", async () => {
    const service = fake();
    service.change.mockRejectedValueOnce(new Error("Lost confirmation"));
    service.state.mockResolvedValueOnce(snapshot).mockResolvedValueOnce({
      ...snapshot,
      outcome: { kind: "PendingReconciliation" },
    });
    show(service);
    const sheet = await reviewService();
    await userEvent.click(
      within(sheet).getByRole("button", { name: "Install service" }),
    );
    expect((await screen.findByRole("alert")).textContent).toBe(
      copy.changeUnknown,
    );
    const unavailable = await reviewService();
    expect(
      within(unavailable).getByRole<HTMLButtonElement>("button", {
        name: "Install service",
      }).disabled,
    ).toBe(true);
    await userEvent.keyboard("{Escape}");
    await userEvent.click(screen.getByRole("button", { name: copy.refresh }));
    await screen.findByText(copy.outcome.PendingReconciliation);
    const pending = await reviewService();
    expect(
      within(pending).getByRole<HTMLButtonElement>("button", {
        name: "Install service",
      }).disabled,
    ).toBe(true);
    expect(service.change).toHaveBeenCalledTimes(1);
  });

  it("always labels service authorship, including a validated via attribution", async () => {
    const { container, rerender } = render(
      <ServiceAuthorship
        view={{ principal: servicePrincipal, representation: null }}
        copy={copy.author}
      />,
    );
    expect(container.textContent).toBe("Agenda assistant · Automation");
    rerender(
      <ServiceAuthorship
        view={{
          principal: servicePrincipal,
          representation: { accountName: "Ari" },
        }}
        copy={copy.author}
      />,
    );
    expect(container.textContent).toBe(
      "Ari · via Agenda assistant · Automation",
    );
    expect(await axeViolations(container)).toEqual([]);
  });
});
