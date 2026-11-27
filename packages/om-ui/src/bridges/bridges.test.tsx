// @vitest-environment jsdom
import { asConversationId } from "@om/platform-api";
import type { OutcomeClass } from "@om/platform-api";
import { act, cleanup, render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import axe from "axe-core";
import { afterEach, describe, expect, it, vi } from "vitest";
import { BridgeRoom } from "./BridgeRoom";
import { BridgeMessage } from "./BridgeMessage";
import type {
  BridgeEntryView,
  BridgeMappingView,
  BridgeRoomView,
  BridgeService,
} from "./bridges";
import { bridgeTestCopy as copy } from "./__fixtures__/copy";

afterEach(cleanup);

const room = asConversationId("example-room");
const principal = { id: "example-bridge-id", name: "Bridge X" };
const mapping: BridgeMappingView = {
  id: "example-mapping",
  network: "Example Chat",
  scope: "Weekend plans",
  direction: "both",
  exportedClasses: ["message.create", "reaction.set"],
  externalClaim:
    "Example Chat advertises end-to-end encryption for this conversation.",
};
const proposed: BridgeEntryView = {
  principal,
  membership: "proposed",
  mappings: [mapping],
  action: {
    availability: "ready",
    intent: { kind: "admit", target: "offer-17" },
  },
};
const member: BridgeEntryView = {
  approversReported: 2,
  principal,
  membership: "member",
  mappings: [{ ...mapping, state: "active" }],
  action: {
    availability: "ready",
    intent: { kind: "remove", target: "membership-23" },
  },
};
const removed: BridgeEntryView = {
  ...member,
  membership: "removed",
  action: null,
};

function view(
  bridge: BridgeEntryView,
  outcome: OutcomeClass | null = null,
): BridgeRoomView {
  return { bridges: [bridge], outcome };
}

function fake(initial: BridgeRoomView, next = initial) {
  return {
    state: vi.fn<BridgeService["state"]>().mockResolvedValue(initial),
    change: vi.fn<BridgeService["change"]>().mockResolvedValue(next),
  };
}

function deferredView() {
  let resolve!: (view: BridgeRoomView) => void;
  const promise = new Promise<BridgeRoomView>((done) => {
    resolve = done;
  });
  return { promise, resolve };
}

async function openReview(user: ReturnType<typeof userEvent.setup>) {
  await user.click(
    await screen.findByRole("button", { name: "Review Bridge X" }),
  );
  return screen.getByRole("dialog", { name: copy.reviewTitle });
}

async function axeViolations(element: Element) {
  const report = await axe.run(element, {
    rules: { "color-contrast": { enabled: false }, region: { enabled: false } },
  });
  return report.violations.map(({ id, help }) => `${id}: ${help}`);
}

describe("Room bridge boundary", () => {
  it("requires an injected service", () => {
    const { container } = render(
      <BridgeRoom conversationId={room} copy={copy} />,
    );
    expect(container.childNodes).toHaveLength(0);
  });

  it("names the active bridge and its external scope, export classes, and separate security claim", async () => {
    const service = fake(view(member));
    const { container } = render(
      <BridgeRoom conversationId={room} service={service} copy={copy} />,
    );
    const notice = within(await screen.findByRole("article"));
    expect(notice.getByText(copy.mapping.active)).toBeTruthy();
    expect(notice.getByText(copy.membership.member)).toBeTruthy();
    expect(notice.getByText(principal.id)).toBeTruthy();
    expect(notice.getByText(mapping.network)).toBeTruthy();
    expect(notice.getByText(mapping.scope)).toBeTruthy();
    expect(notice.getByText(copy.direction.both)).toBeTruthy();
    expect(notice.getByText(copy.eventClass["message.create"])).toBeTruthy();
    expect(notice.getByText(copy.eventClass["reaction.set"])).toBeTruthy();
    expect(notice.getByText(copy.externalSeparate)).toBeTruthy();
    expect(notice.getByText(mapping.externalClaim ?? "")).toBeTruthy();
    expect(
      notice.getByText(/OM encryption protects this message to/).textContent,
    ).toBe("OM encryption protects this message to Bridge X.");
    expect(await axeViolations(container)).toEqual([]);
  });

  it.each(["revoked", "stale", "endpoint_stopped"] as const)(
    "keeps read access visible with a %s mapping",
    async (state) => {
      const service = fake(
        view({
          ...member,
          mappings: [
            {
              ...mapping,
              state,
              direction: "inbound",
              exportedClasses: [],
              externalClaim: null,
            },
          ],
        }),
      );
      render(
        <BridgeRoom conversationId={room} service={service} copy={copy} />,
      );
      expect(await screen.findByText(copy.mapping[state])).toBeTruthy();
      expect(screen.getByText(copy.membership.member)).toBeTruthy();
      expect(screen.getByText(copy.consequence.admit)).toBeTruthy();
      expect(screen.getByText(copy.direction.inbound)).toBeTruthy();
      expect(screen.getByText(copy.noExports)).toBeTruthy();
      expect(screen.getByText(copy.noExternalClaim)).toBeTruthy();
      expect(screen.queryByText(copy.mapping.active)).toBeNull();
    },
  );

  it("supports keyboard review and cancellation, then admits only when Core answers", async () => {
    const user = userEvent.setup();
    const service = fake(view(proposed));
    const answer = deferredView();
    service.change.mockReturnValueOnce(answer.promise);
    render(<BridgeRoom conversationId={room} service={service} copy={copy} />);
    const review = await screen.findByRole("button", {
      name: "Review Bridge X",
    });
    await user.tab();
    expect(document.activeElement).toBe(review);
    await user.keyboard("{Enter}");
    const sheet = screen.getByRole("dialog", { name: copy.reviewTitle });
    expect(document.activeElement).toBe(
      within(sheet).getByRole("heading", { name: principal.name }),
    );
    expect(within(sheet).getByText(copy.consequence.admit)).toBeTruthy();
    expect(within(sheet).getByText(copy.admissionBoundary)).toBeTruthy();
    expect(within(sheet).getByText(mapping.scope).tagName).toBe("BDI");
    expect(within(sheet).getByText(mapping.network).tagName).toBe("BDI");
    expect(await axeViolations(sheet)).toEqual([]);
    await user.keyboard("{Escape}");
    expect(screen.queryByRole("dialog")).toBeNull();
    expect(document.activeElement).toBe(review);
    expect(service.change).not.toHaveBeenCalled();
    await user.keyboard("{Enter}");
    const confirm = screen.getByRole("button", { name: copy.action.admit });
    confirm.focus();
    await user.keyboard("{Enter}");
    expect(document.activeElement).toBe(
      screen.getByRole("heading", { name: copy.heading }),
    );
    expect(service.change.mock.calls).toEqual([
      [room, { kind: "admit", target: "offer-17" }],
    ]);
    expect(screen.getByText(copy.changing)).toBeTruthy();
    expect(screen.getByText(copy.membership.proposed)).toBeTruthy();
    expect(screen.queryByText(copy.mapping.active)).toBeNull();
    await act(async () => {
      answer.resolve(view(member, { kind: "Committed" }));
      await answer.promise;
    });
    expect(screen.getByText(copy.mapping.active)).toBeTruthy();
  });

  it("explains the removal consequence and preserves membership until Core reports removal", async () => {
    const user = userEvent.setup();
    const pending: BridgeEntryView = {
      ...member,
      action: {
        availability: "blocked",
        kind: "remove",
        reason: "unavailable",
      },
    };
    const service = fake(
      view(member),
      view(pending, { kind: "PendingAuthorization" }),
    );
    service.state
      .mockResolvedValueOnce(view(member))
      .mockResolvedValueOnce(view(removed, { kind: "Committed" }));
    render(<BridgeRoom conversationId={room} service={service} copy={copy} />);
    const sheet = within(await openReview(user));
    expect(sheet.getByText(copy.approversAtInstall)).toBeTruthy();
    expect(sheet.getByText(copy.consequence.remove)).toBeTruthy();
    await user.click(sheet.getByRole("button", { name: copy.action.remove }));
    expect(service.change.mock.calls).toEqual([
      [room, { kind: "remove", target: "membership-23" }],
    ]);
    expect(
      await screen.findByText(copy.outcome.PendingAuthorization),
    ).toBeTruthy();
    expect(screen.getByText(copy.membership.member)).toBeTruthy();
    const pendingSheet = within(await openReview(user));
    expect(
      pendingSheet.getByRole<HTMLButtonElement>("button", {
        name: copy.action.remove,
      }).disabled,
    ).toBe(true);
    await user.keyboard("{Escape}");
    await user.click(screen.getByRole("button", { name: copy.refresh }));
    expect(await screen.findByText(copy.membership.removed)).toBeTruthy();
    expect(screen.queryByText(copy.mapping.active)).toBeNull();
    expect(service.change).toHaveBeenCalledTimes(1);
  });

  it("renders a refused change and disables the next attempt with Core’s reason", async () => {
    const user = userEvent.setup();
    const blocked: BridgeEntryView = {
      ...proposed,
      action: {
        availability: "blocked",
        kind: "admit",
        reason: "policy_denied",
      },
    };
    const service = fake(
      view(proposed),
      view(blocked, { kind: "Rejected", reason: "policy_denied" }),
    );
    render(<BridgeRoom conversationId={room} service={service} copy={copy} />);
    await openReview(user);
    await user.click(screen.getByRole("button", { name: copy.action.admit }));
    expect(await screen.findByText(copy.rejected.policy_denied)).toBeTruthy();
    expect(screen.queryByText(copy.mapping.active)).toBeNull();
    const sheet = await openReview(user);
    const confirm = within(sheet).getByRole<HTMLButtonElement>("button", {
      name: copy.action.admit,
      description: copy.rejected.policy_denied,
    });
    expect(confirm.disabled).toBe(true);
    await user.click(confirm);
    expect(service.change).toHaveBeenCalledTimes(1);
    expect(await axeViolations(sheet)).toEqual([]);
  });

  it("rechecks after a lost response without submitting a second change", async () => {
    const user = userEvent.setup();
    const service = fake(view(member));
    service.change.mockRejectedValueOnce(new Error("unreachable"));
    service.state
      .mockResolvedValueOnce(view(member))
      .mockResolvedValueOnce(view(member, { kind: "PendingReconciliation" }));
    render(<BridgeRoom conversationId={room} service={service} copy={copy} />);
    await openReview(user);
    await user.click(screen.getByRole("button", { name: copy.action.remove }));
    expect((await screen.findByRole("alert")).textContent).toBe(
      copy.changeUnknown,
    );
    expect(screen.getByText(copy.mapping.active)).toBeTruthy();
    const sheet = within(await openReview(user));
    expect(
      sheet.getByRole<HTMLButtonElement>("button", {
        name: copy.action.remove,
        description: copy.changeUnknown,
      }).disabled,
    ).toBe(true);
    await user.keyboard("{Escape}");
    await user.click(screen.getByRole("button", { name: copy.refresh }));
    expect(
      await screen.findByText(copy.outcome.PendingReconciliation),
    ).toBeTruthy();
    expect(service.change).toHaveBeenCalledTimes(1);
  });

  it("makes an initial read failure retryable and keeps a late answer out of another conversation", async () => {
    const user = userEvent.setup();
    const service = fake({ bridges: [], outcome: null });
    const answer = deferredView();
    service.state
      .mockRejectedValueOnce(new Error("unavailable"))
      .mockReturnValueOnce(answer.promise);
    const { rerender } = render(
      <BridgeRoom conversationId={room} service={service} copy={copy} />,
    );
    expect((await screen.findByRole("alert")).textContent).toBe(
      copy.loadFailed,
    );
    await user.click(screen.getByRole("button", { name: copy.refresh }));
    expect(screen.getByText(copy.loading)).toBeTruthy();
    rerender(
      <BridgeRoom
        conversationId={asConversationId("other-room")}
        service={service}
        copy={copy}
      />,
    );
    expect(await screen.findByText(copy.empty)).toBeTruthy();
    await act(async () => {
      answer.resolve(view(member));
      await answer.promise;
    });
    expect(screen.queryByText(principal.name)).toBeNull();
  });
});

describe("bridged message", () => {
  it("keeps a linked identity separate from authorship and exposes every supplied fidelity marker", async () => {
    const { container } = render(
      <BridgeMessage
        copy={copy.message}
        view={{
          principal,
          actor: "Alex on Example Chat",
          network: mapping.network,
          identityLink: { kind: "linked", accountName: "Alex on OM" },
          flags: [
            "lossy_formatting",
            "reply_target_unavailable",
            "edit_represented_as_new_message",
            "delete_not_propagated",
            "reaction_not_supported",
            "attachment_transcoded",
            "origin_unverified",
          ],
          carry: "read_only_summary",
        }}
      />,
    );
    expect(
      screen.getByText(copy.message.author).nextElementSibling?.textContent,
    ).toBe(principal.name);
    expect(
      screen.getByText(copy.message.linkedIdentity).nextElementSibling
        ?.textContent,
    ).toBe("Alex on OM");
    const markers = within(
      screen.getByRole("list", { name: copy.message.fidelity }),
    ).getAllByRole("listitem");
    expect(markers.map((marker) => marker.textContent)).toEqual([
      copy.message.flag.lossy_formatting,
      copy.message.flag.reply_target_unavailable,
      copy.message.flag.edit_represented_as_new_message,
      copy.message.flag.delete_not_propagated,
      copy.message.flag.reaction_not_supported,
      copy.message.flag.attachment_transcoded,
      copy.message.flag.origin_unverified,
      copy.message.carry.read_only_summary,
    ]);
    expect(await axeViolations(container)).toEqual([]);
  });

  it("shows unverified attribution and absent links explicitly", () => {
    render(
      <BridgeMessage
        copy={copy.message}
        view={{
          principal,
          actor: null,
          network: mapping.network,
          identityLink: { kind: "not_linked" },
          flags: ["foreign_actor_unverified"],
        }}
      />,
    );
    expect(screen.getByText(copy.message.unknownActor)).toBeTruthy();
    expect(screen.getByText(copy.message.noIdentityLink)).toBeTruthy();
    expect(
      screen.getByText(copy.message.flag.foreign_actor_unverified),
    ).toBeTruthy();
  });
});
