// @vitest-environment jsdom
// SPDX-License-Identifier: MPL-2.0
import type { OutcomeClass } from "@om/platform-api";
import { act, cleanup, render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import axe from "axe-core";
import { afterEach, describe, expect, it, vi } from "vitest";
import { Communities } from "./CommunityView";
import type { CommunitiesDetails, CommunitiesPlace, CommunitiesService, CommunitiesView } from "./communities";
import { communitiesTestCopy as copy } from "./__fixtures__/copy";
import {
  announcementFixture, announcementPlace, communityFixture, communityPlace, createCommunitiesPreview,
  exchangeFixture, exchangePlace, forumFixture, forumPlace, orphanFixture, previewHref, publicationFixture, publicationPlace,
} from "./__fixtures__/service";

afterEach(cleanup);

function reply(details: CommunitiesDetails | null, outcome: OutcomeClass | null = null): CommunitiesView {
  return { details, outcome };
}

function fake(initial: CommunitiesView, next = initial) {
  return {
    state: vi.fn<CommunitiesService["state"]>().mockResolvedValue(initial),
    change: vi.fn<CommunitiesService["change"]>().mockResolvedValue(next),
  };
}

function deferredView() {
  let resolve!: (view: CommunitiesView) => void;
  const promise = new Promise<CommunitiesView>((done) => { resolve = done; });
  return { promise, resolve };
}

function show(service: CommunitiesService, place: CommunitiesPlace = communityPlace) {
  return render(<Communities service={service} place={place} hrefFor={previewHref} copy={copy} />);
}

async function axeViolations(element: Element) {
  const report = await axe.run(element, {
    rules: { "color-contrast": { enabled: false }, region: { enabled: false } },
  });
  return report.violations.map(({ id, help }) => `${id}: ${help}`);
}

describe("Community and Room admission", () => {
  it("mounts only with an injected service", () => {
    const { container } = render(<Communities place={communityPlace} hrefFor={previewHref} copy={copy} />);
    expect(container.childNodes).toHaveLength(0);
  });

  it("reviews the Community invitation by keyboard and uses only the returned join result", async () => {
    const user = userEvent.setup();
    const service = fake(reply(communityFixture));
    const answer = deferredView();
    service.change.mockReturnValueOnce(answer.promise);
    const { container } = show(service);
    const review = await screen.findByRole("button", { name: copy.admission.review });
    expect(screen.getByText(copy.admission.boundary)).toBeTruthy();
    expect(screen.getByRole("link", { name: "Workdays" }).getAttribute("href")).toBe(previewHref(forumPlace));
    expect(screen.getByRole("link", { name: "Garden Notes" }).getAttribute("href")).toBe(previewHref(publicationPlace));
    expect(await axeViolations(container)).toEqual([]);

    await user.tab();
    expect(document.activeElement).toBe(review);
    await user.keyboard("{Enter}");
    let dialog = screen.getByRole("dialog", { name: copy.admission.reviewTitle });
    expect(document.activeElement).toBe(within(dialog).getByRole("heading", { name: "Community · Neighbourhood Garden" }));
    expect(within(dialog).getByText(copy.admission.boundary)).toBeTruthy();
    expect(within(dialog).getByText(copy.admission.uses.one_time)).toBeTruthy();
    expect(within(dialog).getByText(copy.admission.expires)).toBeTruthy();
    expect(within(dialog).getByText("See the Community’s member pages")).toBeTruthy();
    expect(await axeViolations(dialog)).toEqual([]);
    await user.keyboard("{Escape}");
    expect(document.activeElement).toBe(review);
    expect(service.change).not.toHaveBeenCalled();

    await user.keyboard("{Enter}");
    dialog = screen.getByRole("dialog", { name: copy.admission.reviewTitle });
    await user.click(within(dialog).getByRole("button", { name: copy.action.join_community }));
    expect(service.change).toHaveBeenCalledExactlyOnceWith(communityPlace, {
      kind: "join_community", target: "garden-invitation",
    });
    expect(document.activeElement).toBe(screen.getByRole("heading", { name: communityFixture.name }));
    expect(screen.getByRole("status").textContent).toBe(copy.changing);
    expect(screen.queryByText(copy.admission.membership.community.joined)).toBeNull();
    await act(async () => {
      answer.resolve(reply({ ...communityFixture, admission: { membership: "joined", action: null } }, { kind: "Committed" }));
      await answer.promise;
    });
    expect(screen.getByText(copy.admission.membership.community.joined)).toBeTruthy();
    expect(screen.getByText(copy.admission.boundary)).toBeTruthy();
  });

  it("keeps the Room request separate and shows the approval and membership supplied by the service", async () => {
    const user = userEvent.setup();
    const preview = createCommunitiesPreview("active");
    const service = { state: vi.fn(preview.state.bind(preview)), change: vi.fn(preview.change.bind(preview)) };
    const { rerender } = show(service);
    await user.click(await screen.findByRole("button", { name: copy.admission.review }));
    await user.click(screen.getByRole("button", { name: copy.action.join_community }));
    await screen.findByText(copy.admission.membership.community.joined);

    rerender(<Communities service={service} place={exchangePlace} hrefFor={previewHref} copy={copy} />);
    expect(await screen.findByText(copy.admission.membership.room.not_joined)).toBeTruthy();
    await user.click(screen.getByRole("button", { name: copy.admission.review }));
    const dialog = screen.getByRole("dialog", { name: copy.admission.reviewTitle });
    expect(within(dialog).getByText(copy.admission.approval)).toBeTruthy();
    expect(within(dialog).getByText(copy.admission.roomBoundary)).toBeTruthy();
    await user.click(within(dialog).getByRole("button", { name: copy.action.request_room_join }));
    expect(service.change).toHaveBeenLastCalledWith(exchangePlace, {
      kind: "request_room_join", target: "garden-exchange-request",
    });
    expect(await screen.findByText(copy.admission.membership.room.pending)).toBeTruthy();
    expect(screen.getByRole("status").textContent).toBe(copy.outcome.PendingAuthorization);
  });

  it("labels directory observations and keeps the invitation review read-only", async () => {
    const service = fake(reply({
      kind: "room", id: exchangeFixture.id, name: "Observed Room", nameSource: "host_observed",
      topics: [], participation: [], members: [],
      admission: {
        membership: null,
        offer: { id: "observed-offer", approvalRequired: true, source: "host_observed" },
        action: { availability: "blocked", kind: "request_room_join", reason: "unsupported" },
      },
    }));
    show(service, exchangePlace);
    expect(await screen.findByText(copy.directoryObservation)).toBeTruthy();
    expect(screen.queryByText(copy.admission.membership.room.not_joined)).toBeNull();
    await userEvent.click(screen.getByRole("button", { name: copy.admission.review }));
    const review = within(screen.getByRole("dialog"));
    expect(review.getByText(copy.admission.observed)).toBeTruthy();
    expect(review.getByRole<HTMLButtonElement>("button", { name: copy.action.request_room_join }).disabled).toBe(true);
    expect(service.change).not.toHaveBeenCalled();
  });

  it("states why a stale admission is disabled and accepts no replacement action", async () => {
    const user = userEvent.setup();
    const service = fake(reply({
      ...exchangeFixture,
      admission: {
        ...exchangeFixture.admission,
        membership: "denied",
        action: { availability: "blocked", kind: "request_room_join", reason: "stale_state" },
      },
    }));
    show(service, exchangePlace);
    await user.click(await screen.findByRole("button", { name: copy.admission.review }));
    const sheet = within(screen.getByRole("dialog"));
    const action = sheet.getByRole<HTMLButtonElement>("button", { name: copy.action.request_room_join });
    expect(action.disabled).toBe(true);
    expect(action.getAttribute("aria-describedby")).toContain(sheet.getByText(copy.rejected.stale_state).id);
    await user.click(action);
    expect(service.change).not.toHaveBeenCalled();
  });
});

describe("Core views", () => {
  it("shows topics with their state and keeps a visible route back to the parent Room", async () => {
    const { container, rerender } = show(fake(reply(forumFixture)), forumPlace);
    const [topic] = forumFixture.topics;
    if (topic === undefined) throw new Error("The forum fixture needs a topic");
    const topicPlace = { kind: "topic", roomId: forumFixture.id, id: topic.id } as const;
    expect(await screen.findByText(copy.room.mode.forum)).toBeTruthy();
    expect(screen.getByRole("link", { name: topic.title }).getAttribute("href")).toBe(previewHref(topicPlace));
    for (const status of Object.values(copy.room.topicStatus)) expect(screen.getByText(status)).toBeTruthy();
    expect(await axeViolations(container)).toEqual([]);
    rerender(
      <Communities
        service={fake(reply({ ...forumFixture, selectedTopic: topic }))}
        place={topicPlace} hrefFor={previewHref} copy={copy}
      />,
    );
    expect(await screen.findByRole("heading", { name: topic.title })).toBeTruthy();
    expect(screen.getByRole("link", { name: "Back to Workdays" }).getAttribute("href")).toBe(previewHref(forumPlace));
  });

  it("shows different permission answers for the same role and does not lift a restriction by the clock", async () => {
    const { container } = show(fake(reply(announcementFixture)), announcementPlace);
    expect(await screen.findByText(copy.room.mode.announcement)).toBeTruthy();
    const mira = screen.getByText("Mira");
    const alex = screen.getByText("Alex");
    const user = userEvent.setup();
    await user.click(mira);
    await user.click(alex);
    expect(mira.closest("details")?.textContent).toContain("Granted by the Publisher role");
    expect(alex.closest("details")?.textContent).toContain(copy.authority.answer.restricted);
    expect(screen.getByText(copy.authority.restriction.active)).toBeTruthy();
    expect(screen.getByText(copy.authority.intendedEnd)).toBeTruthy();
    expect(await axeViolations(container)).toEqual([]);
  });

  it.each(["orphaned", "conflict"] as const)("shows %s and links only to a supplied successor", async (state) => {
    const { container } = show(fake(reply({
      ...orphanFixture,
      governance: { ...orphanFixture.governance, state },
    })));
    expect(await screen.findByText(copy.governance.notice[state])).toBeTruthy();
    expect(screen.getByText(copy.governance.fork)).toBeTruthy();
    expect(screen.getByRole("link", { name: "Open successor Garden Together" }).getAttribute("href")).toBe("#community/garden-successor");
    expect(screen.getByRole<HTMLButtonElement>("button", { name: copy.action.join_community }).disabled).toBe(true);
    expect(await axeViolations(container)).toEqual([]);
  });

  it("shows the public feed, edit and retraction, and follows or unfollows only through the service", async () => {
    const user = userEvent.setup();
    const preview = createCommunitiesPreview("active");
    const service = { state: vi.fn(preview.state.bind(preview)), change: vi.fn(preview.change.bind(preview)) };
    const { container } = show(service, publicationPlace);
    expect(await screen.findByText(copy.publication.publicFeed)).toBeTruthy();
    expect(screen.getByText(copy.publication.provenance.confirmed)).toBeTruthy();
    expect(screen.getByText(copy.publication.edited)).toBeTruthy();
    expect(screen.getByText(copy.publication.retracted)).toBeTruthy();
    expect(screen.getByText(copy.publication.copiesRemain)).toBeTruthy();
    expect(screen.getByRole("link", { name: "Workdays" }).getAttribute("href")).toBe(previewHref(forumPlace));
    expect(await axeViolations(container)).toEqual([]);

    await user.click(screen.getByRole("button", { name: copy.action.follow }));
    expect(service.change).toHaveBeenLastCalledWith(publicationPlace, { kind: "follow", target: "garden-notes" });
    expect(await screen.findByText(copy.publication.following)).toBeTruthy();
    await user.click(screen.getByRole("button", { name: copy.action.unfollow }));
    expect(service.change).toHaveBeenLastCalledWith(publicationPlace, { kind: "unfollow", target: "garden-notes" });
    expect(await screen.findByText(copy.publication.notFollowing)).toBeTruthy();
  });
});

describe("Waiting and recovery", () => {
  it("shows a slow read and discards an old profile’s answer when the service changes", async () => {
    const old = fake(reply(communityFixture));
    const slow = deferredView();
    old.state.mockReturnValueOnce(slow.promise);
    const next = fake(reply({ ...communityFixture, name: "Reading Circle" }));
    const { container, rerender } = show(old);
    expect(screen.getByRole("status").textContent).toBe(copy.loading);
    expect(await axeViolations(container)).toEqual([]);
    rerender(<Communities service={next} place={communityPlace} hrefFor={previewHref} copy={copy} />);
    await screen.findByRole("heading", { name: "Reading Circle" });
    await act(async () => {
      slow.resolve(reply(communityFixture));
      await slow.promise;
    });
    expect(screen.queryByRole("heading", { name: communityFixture.name })).toBeNull();
  });

  it("retries a failed read and announces the error beside the affected view", async () => {
    const service = fake(reply(communityFixture));
    service.state.mockRejectedValueOnce(new Error("read failed"));
    const { container } = show(service);
    expect((await screen.findByRole("alert")).textContent).toBe(copy.loadFailed);
    expect(await axeViolations(container)).toEqual([]);
    await userEvent.click(screen.getByRole("button", { name: copy.refresh }));
    expect(await screen.findByRole("heading", { name: communityFixture.name })).toBeTruthy();
    expect(service.state).toHaveBeenCalledTimes(2);
    expect(service.change).not.toHaveBeenCalled();
  });

  it("refreshes an uncertain follow without replaying it or assuming it succeeded", async () => {
    const user = userEvent.setup();
    const service = fake(reply(publicationFixture));
    service.change.mockRejectedValueOnce(new Error("response lost"));
    service.state.mockResolvedValueOnce(reply(publicationFixture)).mockResolvedValueOnce(
      reply(publicationFixture, { kind: "PendingReconciliation" }),
    );
    show(service, publicationPlace);
    const follow = await screen.findByRole<HTMLButtonElement>("button", { name: copy.action.follow });
    await user.click(follow);
    expect((await screen.findByRole("alert")).textContent).toBe(copy.changeUnknown);
    expect(follow.disabled).toBe(true);
    expect(screen.getByText(copy.publication.notFollowing)).toBeTruthy();
    await user.click(screen.getByRole("button", { name: copy.refresh }));
    expect(screen.getByRole("status").textContent).toBe(copy.outcome.PendingReconciliation);
    expect(follow.disabled).toBe(true);
    expect(service.change).toHaveBeenCalledTimes(1);
  });

  const outcomes: readonly [OutcomeClass, string][] = [
    ...Object.entries(copy.outcome).map(
      ([kind, message]): [OutcomeClass, string] => [{ kind: kind as keyof typeof copy.outcome }, message],
    ),
    ...Object.entries(copy.rejected).map(
      ([reason, message]): [OutcomeClass, string] => [{ kind: "Rejected", reason: reason as keyof typeof copy.rejected }, message],
    ),
    ...Object.entries(copy.needsAction).map(
      ([action, message]): [OutcomeClass, string] => [{ kind: "NeedsUserAction", action: action as keyof typeof copy.needsAction }, message],
    ),
  ];

  it.each(outcomes)("renders the returned outcome %j", async (outcome, message) => {
    show(fake(reply(publicationFixture, outcome)), publicationPlace);
    await screen.findByRole("heading", { name: publicationFixture.name });
    expect(screen.getByRole("status").textContent).toBe(message);
  });
});
