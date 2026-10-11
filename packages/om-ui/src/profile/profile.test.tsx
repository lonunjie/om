// @vitest-environment jsdom
import { cleanup, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import axe from "axe-core";
import { afterEach, describe, expect, it } from "vitest";
import { act } from "react";
import { asOperationId, asProfileId } from "@om/platform-api";
import type { CoreIntent, ProfileId } from "@om/platform-api";
import { createActionRegistry } from "../actions/registry";
import {
  ConnectedProfileSwitcher, ProfileProvider, ProfileSwitcher, bindSuggestionToProfile, createMemoryKeyValue, createProfileScopedState,
  dispatchWithCapturedContext, resolveDeepLink, useActiveProfile, useCaptureContext, useDispatch, useProfileContext, useProfiles, useScopedState,
} from "./index";
import { P1, P2, TWO_PROFILES, createFakeCore } from "./test-support";

afterEach(cleanup);
const labels = { group: "Identities", current: "current" };

function Probe({ out }: { readonly out: { api?: ReturnType<typeof grab> } }) {
  out.api = grab();
  return <p>active:{useActiveProfile()?.label ?? "none"}</p>;
}
function grab() {
  return { profiles: useProfiles(), capture: useCaptureContext(), dispatch: useDispatch(), store: useScopedState(), ctx: (() => { try { return useProfileContext(); } catch { return null; } })() };
}
async function mount() {
  const core = createFakeCore(TWO_PROFILES, true);
  const store = createProfileScopedState(createMemoryKeyValue());
  const out: { api?: ReturnType<typeof grab> } = {};
  render(<ProfileProvider client={core} contexts={core} store={store} initialProfileId={P1}><Probe out={out} /><ConnectedProfileSwitcher labels={labels} /></ProfileProvider>);
  await screen.findByText("active:Alice");
  const api = (): ReturnType<typeof grab> => { if (!out.api) throw new Error("not rendered"); return out.api; };
  return { core, store, api };
}

describe("UI-06 profile-namespaced state", () => {
  it("switch preserves each profile's drafts, view cache and adaptive navigation with no cross-profile read", async () => {
    const kv = createMemoryKeyValue();
    const reads: string[] = [];
    const spy = { ...kv, get: (k: string) => { reads.push(k); return kv.get(k); } };
    const s = createProfileScopedState(spy);
    await s.set({ profileId: P1, namespace: "drafts", key: "c1" }, "draft-A");
    await s.set({ profileId: P2, namespace: "drafts", key: "c1" }, "draft-B");
    await s.set({ profileId: P1, namespace: "view-cache", key: "c1" }, "cache-A");
    await s.set({ profileId: P1, namespace: "adaptive-navigation", key: "recent" }, "A");
    await s.set({ profileId: P2, namespace: "adaptive-navigation", key: "recent" }, "B");
    reads.length = 0;
    expect(await s.get({ profileId: P2, namespace: "drafts", key: "c1" })).toBe("draft-B");
    expect(await s.get({ profileId: P2, namespace: "view-cache", key: "c1" })).toBeUndefined();
    expect(reads.every((k) => k.includes('"p2"'))).toBe(true);
    // ClearAdaptiveNavigation for P1 leaves P2 (and P1's drafts) alone.
    await s.clearNamespace(P1, "adaptive-navigation");
    expect(await s.get({ profileId: P1, namespace: "adaptive-navigation", key: "recent" })).toBeUndefined();
    expect(await s.get({ profileId: P2, namespace: "adaptive-navigation", key: "recent" })).toBe("B");
    expect(await s.get({ profileId: P1, namespace: "drafts", key: "c1" })).toBe("draft-A");
  });

  it("profile ids that prefix one another do not collide", async () => {
    const s = createProfileScopedState(createMemoryKeyValue());
    const p10: ProfileId = asProfileId("p10");
    await s.set({ profileId: P1, namespace: "scroll", key: "k" }, "1");
    await s.set({ profileId: p10, namespace: "scroll", key: "k" }, "10");
    await s.clearProfile(P1);
    expect(await s.get({ profileId: p10, namespace: "scroll", key: "k" })).toBe("10");
  });

  it("switching changes only the active selection: no intent is dispatched and the origin's draft survives", async () => {
    const { core, store, api } = await mount();
    await store.set({ profileId: P1, namespace: "drafts", key: "c1" }, "half-typed");
    await userEvent.click(screen.getByRole("button", { name: /Work/ }));
    await screen.findByText("active:Work");
    expect(core.dispatched).toHaveLength(0);
    expect(api().ctx?.profileId).toBe(P2);
    expect(await store.get({ profileId: P2, namespace: "drafts", key: "c1" })).toBeUndefined();
    await userEvent.click(screen.getByRole("button", { name: /Alice/ }));
    await screen.findByText("active:Alice");
    expect(await store.get({ profileId: P1, namespace: "drafts", key: "c1" })).toBe("half-typed");
  });
});

describe("UI-07 deep links", () => {
  it("several profiles and no explicit profile asks the user", () => {
    expect(resolveDeepLink("om:invite/abc", TWO_PROFILES)).toEqual({ kind: "choose_profile", target: "invite/abc", options: TWO_PROFILES });
  });
  it("one profile acts on it; an explicit known profile is honoured", () => {
    expect(resolveDeepLink("om:invite/abc", TWO_PROFILES.slice(0, 1))).toEqual({ kind: "open", profileId: P1, target: "invite/abc" });
    expect(resolveDeepLink("om:invite/abc?profile=p2", TWO_PROFILES)).toEqual({ kind: "open", profileId: P2, target: "invite/abc" });
  });
  it("malformed, unknown-profile and profile-less links fail closed", () => {
    for (const bad of ["", "http://x", "om:", "om:a b", "om:a#f", "om:a?profile=p1&profile=p2", "om:a?=x", "om:a?profile=", `om:${"a".repeat(3000)}`]) {
      expect(resolveDeepLink(bad, TWO_PROFILES).kind, bad).toBe("rejected");
    }
    expect(resolveDeepLink("om:a?profile=zzz", TWO_PROFILES)).toEqual({ kind: "rejected", reason: "unknown_profile" });
    expect(resolveDeepLink("om:a", [])).toEqual({ kind: "rejected", reason: "no_profiles" });
  });
});

describe("UI-10 captured context", () => {
  it("an in-flight dispatch stays bound to the origin profile after a switch", async () => {
    const { core, api } = await mount();
    const dispatch = api().dispatch;
    const inflight = dispatch((ctx): CoreIntent => ({ kind: "ClearAdaptiveNavigation", ctx, operationId: asOperationId("op-x") }));
    await userEvent.click(screen.getByRole("button", { name: /Work/ }));
    await screen.findByText("active:Work");
    act(() => { core.release(); });
    const { ctx, handle } = await inflight;
    expect(ctx.profileId).toBe(P1);
    expect(handle.profileId).toBe(P1);
    expect(core.dispatched[0]?.ctx.profileId).toBe(P1);
    // a dispatch after the switch binds to the new profile
    const second = api().dispatch((c): CoreIntent => ({ kind: "ClearAdaptiveNavigation", ctx: c, operationId: asOperationId("op-y") }));
    act(() => { core.release(); });
    expect((await second).ctx.profileId).toBe(P2);
  });

  it("captured contexts are frozen and the factory must use the captured ctx", async () => {
    const core = createFakeCore(TWO_PROFILES);
    const ctx = core.contextFor(P1);
    const other = core.contextFor(P2);
    const r = await dispatchWithCapturedContext(core, () => ctx, (c): CoreIntent => ({ kind: "ClearAdaptiveNavigation", ctx: c, operationId: asOperationId("o1") }));
    expect(Object.isFrozen(r.ctx)).toBe(true);
    await expect(dispatchWithCapturedContext(core, () => ctx, (): CoreIntent => ({ kind: "ClearAdaptiveNavigation", ctx: other, operationId: asOperationId("o2") }))).rejects.toThrow();
    expect(core.dispatched).toHaveLength(1);
  });

  it("a suggestion executed after a profile switch cancels instead of landing in the other profile", async () => {
    const { api } = await mount();
    const registry = createActionRegistry({ newOperationId: () => asOperationId("op-s") });
    const origin = api().capture();
    const bound = bindSuggestionToProfile({ actionId: "clear-adaptive-navigation", params: {} }, origin, { registry, currentContext: api().capture });
    expect(bound.execute().status).toBe("intent");
    await userEvent.click(screen.getByRole("button", { name: /Work/ }));
    await screen.findByText("active:Work");
    expect(bound.execute()).toEqual({ status: "cancelled", reason: "profile_changed" });
  });
});

describe("UI-11 ranked suggestions", () => {
  it("destructive/security suggestions never auto-execute or supply a confirmation", async () => {
    const { api } = await mount();
    const registry = createActionRegistry({ newOperationId: () => asOperationId("op-d") });
    const ctx = api().capture();
    const bound = bindSuggestionToProfile({ actionId: "block-person", params: { personId: "u1", scope: "local" } }, ctx, { registry, currentContext: api().capture });
    expect(bound.execute()).toEqual({ status: "needs_confirmation", actionId: "block-person" });
    expect(bound.execute.length).toBe(0); // no argument through which a confirmation could be passed
    // A confirmation cannot be minted for a suggestion origin, and smuggling a direct token into a suggestion request is rejected.
    expect(registry.issueConfirmation({ actionId: "block-person", params: { personId: "u1", scope: "local" }, ctx, origin: "suggestion" })).toMatchObject({ status: "rejected" });
    const issued = registry.issueConfirmation({ actionId: "block-person", params: { personId: "u1", scope: "local" }, ctx, origin: "direct" });
    if (issued.status !== "issued") throw new Error("expected token");
    expect(registry.resolve({ actionId: "block-person", params: { personId: "u1", scope: "local" }, ctx, origin: "suggestion", confirmation: issued.token })).toEqual({ status: "rejected", code: "suggestion_cannot_confirm" });
  });
});

describe("ProfileSwitcher", () => {
  it("is hidden with fewer than two profiles", () => {
    const { container } = render(<ProfileSwitcher profiles={TWO_PROFILES.slice(0, 1)} activeProfileId={P1} onSwitch={() => undefined} labels={labels} />);
    expect(container.innerHTML).toBe("");
  });

  it("marks the current profile, is keyboard operable and axe-clean", async () => {
    const picked: ProfileId[] = [];
    const { container } = render(<ProfileSwitcher profiles={TWO_PROFILES} activeProfileId={P1} onSwitch={(id) => { picked.push(id); }} labels={labels} />);
    expect(screen.getByRole("button", { name: /Alice/ }).getAttribute("aria-current")).toBe("true");
    expect(screen.getByRole("list", { name: "Identities" })).toBeTruthy();
    await userEvent.tab();
    await userEvent.tab();
    expect(document.activeElement).toBe(screen.getByRole("button", { name: /Work/ }));
    await userEvent.keyboard("{Enter}");
    expect(picked).toEqual([P2]);
    await userEvent.keyboard(" ");
    expect(picked).toEqual([P2, P2]);
    const r = await axe.run(container);
    expect(r.violations).toEqual([]);
  });

  it("connected switcher lists provider profiles", async () => {
    await mount();
    await waitFor(() => { expect(screen.getAllByRole("button")).toHaveLength(2); });
  });
});
