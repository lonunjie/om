// @vitest-environment jsdom
import { cleanup, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import axe from "axe-core";
import { afterEach, describe, expect, it, vi } from "vitest";
import { asProfileId, available, unavailable, FAMILY_NAMES } from "@om/platform-api";
import type { CapabilityEntry, CapabilityReport, CreateVaultOptions, DeviceVault, FamilyName, PlatformAdapter, ProfileExecutionContext, ProfileId } from "@om/platform-api";
import { createHarness } from "@om/core-harness";
import type { Harness } from "@om/core-harness";
import { createWebDeviceVault } from "@om/web-platform";
import { FakeAuthenticator, fakeEnv } from "../../../om-web-platform/src/vault/__fixtures__/fake-env";
import type { FakeEnv } from "../../../om-web-platform/src/vault/__fixtures__/fake-env";
import { createActionRegistry } from "../actions/registry";
import { ConversationList } from "../conversations/ConversationList";
import { useConversationList } from "../conversations/hooks";
import { testCopy as conversationCopy } from "../conversations/__fixtures__/copy";
import { testCopy } from "./__fixtures__/copy";
import { OnboardingFlow } from "./OnboardingFlow";
import type { OnboardingPlatform } from "./platform";

afterEach(() => { cleanup(); vi.restoreAllMocks(); window.localStorage.clear(); window.sessionStorage.clear(); });

const PID = asProfileId("me");
const axeViolations = async (el: Element): Promise<string[]> => {
  const r = await axe.run(el, { rules: { "color-contrast": { enabled: false }, region: { enabled: false } } });
  return r.violations.map((v) => `${v.id}: ${v.help}`);
};

interface PlatformOpts {
  readonly clipboard?: "sensitive" | "unsupported" | "unavailable";
  readonly share?: "shared" | "cancelled" | "failed" | "unavailable";
  readonly camera?: "granted" | "denied" | "unavailable";
}
interface TestPlatform { readonly platform: OnboardingPlatform; readonly written: string[]; readonly shared: unknown[]; readonly cameraCalls: { n: number }; readonly contactsTouched: { n: number } }

function makePlatform(o: PlatformOpts = {}): TestPlatform {
  const written: string[] = []; const shared: unknown[] = []; const cameraCalls = { n: 0 }; const contactsTouched = { n: 0 };
  const clip = o.clipboard ?? "unavailable";
  const entry = (state: CapabilityEntry["state"], detail?: string): CapabilityEntry => (detail === undefined ? { state } : { state, detail });
  const families = Object.fromEntries(FAMILY_NAMES.map((f) => [f, entry("unavailable")])) as Record<FamilyName, CapabilityEntry>;
  const report: CapabilityReport = {
    families: { ...families, clipboard: clip === "sensitive" ? entry("available") : clip === "unsupported" ? entry("partial", "sensitive_unsupported") : entry("unavailable") },
    backgroundPush: "none", localDiscovery: "manual_known_endpoint", localContacts: "unavailable", callSystemIntegration: false,
  };
  const full = {
    capabilities: () => report,
    share: () => (o.share === undefined || o.share === "unavailable" ? unavailable<never>() : available({ share: (c: unknown) => { shared.push(c); return Promise.resolve(o.share as "shared" | "cancelled" | "failed"); } })),
    camera: () => (o.camera === undefined || o.camera === "unavailable" ? unavailable<never>() : available({ requestAccess: () => { cameraCalls.n++; return Promise.resolve(o.camera as "granted" | "denied"); } })),
    clipboard: () => (clip === "unavailable" ? unavailable<never>() : available({ write: (t: string) => { written.push(t); return Promise.resolve(clip === "sensitive" ? "written" as const : "sensitive_unsupported" as const); } })),
    get contactsLocal(): never { contactsTouched.n++; throw new Error("contactsLocal must not be used"); },
  } as unknown as PlatformAdapter;
  return { platform: full, written, shared, cameraCalls, contactsTouched };
}

// jsdom quirk: WebCrypto ArrayBuffers are not `instanceof` the jsdom-global ArrayBuffer, which the vault's byte
// coercion checks. Re-wrap them (tests only; a real browser has one realm).
const isForeignBuffer = (v: unknown): v is ArrayBuffer => Object.prototype.toString.call(v) === "[object ArrayBuffer]" && !(v instanceof ArrayBuffer);
function sameRealm(v: unknown): unknown {
  if (isForeignBuffer(v)) return new Uint8Array(v).slice().buffer;
  if (typeof v === "function") return (...a: unknown[]): unknown => sameRealm((v as (...x: unknown[]) => unknown)(...a));
  if (v instanceof Promise) return v.then(sameRealm);
  if (typeof v === "object" && v !== null && Object.getPrototypeOf(v) === Object.prototype) {
    return Object.fromEntries(Object.entries(v).map(([k, x]) => [k, sameRealm(x)]));
  }
  return v;
}
const realmSafe = (env: FakeEnv): FakeEnv => (env.credentials === undefined ? env : { ...env, credentials: sameRealm(env.credentials) as NonNullable<FakeEnv["credentials"]> });

interface Rig { readonly h: Harness; readonly ctx: ProfileExecutionContext; readonly env: FakeEnv; readonly auth: FakeAuthenticator; readonly vault: DeviceVault; readonly done: ReturnType<typeof vi.fn> }
function rig(auth = new FakeAuthenticator({ prf: "ok", backupEligible: false })): Rig {
  const h = createHarness({ env: {}, clock: () => 1000 });
  const ctx = h.seedProfile({ profileId: PID, label: "pending" });
  const env = fakeEnv({ auth });
  return { h, ctx, env, auth, vault: createWebDeviceVault(realmSafe(env)), done: vi.fn() };
}

function ui(r: Rig, tp: TestPlatform, extra: Partial<Parameters<typeof OnboardingFlow>[0]> = {}) {
  return <OnboardingFlow client={r.h} registry={createActionRegistry()} ctx={r.ctx} vault={r.vault} platform={tp.platform} copy={testCopy} onDone={r.done as () => void} {...extra} />;
}

const click = (name: string | RegExp) => userEvent.click(screen.getByRole("button", { name }));
async function toProtected(r: Rig, tp: TestPlatform, extra = {}) {
  const view = render(ui(r, tp, extra));
  await click("Get started");
  await click("Protect this device");
  await screen.findByRole("heading", { name: "Protection chosen" });
  return view;
}
const keyGroups = (): string[] => within(screen.getByRole("list", { name: "Recovery key" })).getAllByRole("listitem").map((li) => li.textContent.replace(/^Group \d+ of \d+:\s*/, ""));
async function toRecovery(r: Rig, tp: TestPlatform) {
  const view = await toProtected(r, tp);
  await click("Continue");
  await screen.findByRole("list", { name: "Recovery key" });
  return view;
}
/** Retypes whichever groups the form asks for (from the visible key, as a user who wrote it down would). */
async function verifyWith(groups: readonly string[], wrong = false) {
  await click("Check my copy");
  for (const input of screen.getAllByRole("textbox")) {
    const n = Number(/Group (\d+)/.exec(input.closest("label")?.textContent ?? "")?.[1]);
    await userEvent.type(input, wrong ? "ZZZZ" : (groups[n - 1] ?? ""));
  }
}
async function toInviteFirst(r: Rig, tp: TestPlatform, extra = {}) {
  const view = render(ui(r, tp, extra));
  await click("Get started");
  await click("Protect this device");
  await screen.findByRole("heading", { name: "Protection chosen" });
  await click("Continue");
  const groups = keyGroups();
  await verifyWith(groups);
  await click("I saved it");
  await screen.findByRole("heading", { name: "Add your first contact" });
  return { view, groups };
}

describe("protect_device", () => {
  it("happy path: PRF on a device-bound passkey, then recovery, invites skipped, empty Chats", async () => {
    const r = rig();
    const tp = makePlatform();
    const { container } = render(ui(r, tp));
    expect(await axeViolations(container)).toEqual([]);
    await click("Get started");
    await screen.findByRole("heading", { name: "Protect this device" });
    expect(document.activeElement).toBe(screen.getByRole("heading", { name: "Protect this device" }));
    expect(await axeViolations(container)).toEqual([]);
    await click("Protect this device");
    await screen.findByRole("heading", { name: "Protection chosen" });
    expect(screen.getByText("Method: webauthn-prf")).toBeTruthy();
    expect(screen.getByText("Because: prf_verified")).toBeTruthy();
    expect(screen.queryByText(/REDUCED/)).toBeNull();
    expect(screen.queryByText(/DEVICE KEY/)).toBeNull();
    expect(await axeViolations(container)).toEqual([]);
    expect(r.auth.createCalls).toBe(1);

    await click("Continue");
    await screen.findByRole("heading", { name: "Your recovery key" });
    expect(document.activeElement).toBe(screen.getByRole("heading", { name: "Your recovery key" }));
    expect(await axeViolations(container)).toEqual([]);
    const groups = keyGroups();
    expect(groups.join("-")).toMatch(/^OM-DEV-FAKE-/);
    await verifyWith(groups);
    expect(await axeViolations(container)).toEqual([]);
    await click("I saved it");

    await screen.findByRole("heading", { name: "Add your first contact" });
    expect(document.activeElement).toBe(screen.getByRole("heading", { name: "Add your first contact" }));
    expect(await axeViolations(container)).toEqual([]);
    await click("Skip for now");
    await screen.findByRole("heading", { name: "You are ready" });
    expect(await axeViolations(container)).toEqual([]);
    await click("Open chats");
    expect(r.done).toHaveBeenCalledTimes(1);
    expect(tp.contactsTouched.n).toBe(0);

    function Chats() {
      const list = useConversationList({ client: r.h, registry: createActionRegistry(), ctx: r.ctx });
      return list.status === "ready" ? <ConversationList conversations={list.value} selectedId={null} onSelect={() => undefined} copy={conversationCopy} /> : null;
    }
    cleanup();
    render(<Chats />);
    expect(await screen.findByText("No conversations")).toBeTruthy();
  });

  it("a synced passkey (PRF) shows the reduced at-rest notice", async () => {
    const r = rig(new FakeAuthenticator({ prf: "ok", backupEligible: true }));
    const { container } = await toProtected(r, makePlatform());
    expect(screen.getByText(/REDUCED/)).toBeTruthy();
    expect(screen.queryByText(/DEVICE KEY/)).toBeNull();
    expect(await axeViolations(container)).toEqual([]);
  });

  it("device-key shows the distinct device-key limit, the native recommendation and platform limits", async () => {
    const r = rig(new FakeAuthenticator({ prf: "not_enabled", backupEligible: false }));
    const { container } = await toProtected(r, makePlatform());
    expect(screen.getByText("Method: device-key")).toBeTruthy();
    expect(screen.getByText(/DEVICE KEY: a copy of this browser profile/)).toBeTruthy();
    expect(screen.getByText(/native app/)).toBeTruthy();
    expect(screen.getByText("Limit: secure_surface_unavailable")).toBeTruthy();
    expect(screen.queryByText(/REDUCED/)).toBeNull();
    expect(await axeViolations(container)).toEqual([]);
  });

  it("needs_user_gesture waits for another click and never retries by itself", async () => {
    const auth = new FakeAuthenticator({ prf: "ok", backupEligible: false });
    const r = rig(auth);
    r.env.focused = () => false;
    const { container } = render(ui(r, makePlatform()));
    await click("Get started");
    await click("Protect this device");
    expect(await screen.findByText("Another confirmation is needed (enroll_create)")).toBeTruthy();
    expect(screen.queryByRole("button", { name: "Protect this device" })).toBeNull();
    expect(await axeViolations(container)).toEqual([]);
    await new Promise((res) => setTimeout(res, 50));
    expect(auth.createCalls).toBe(0);
    expect(screen.queryByRole("heading", { name: "Protection chosen" })).toBeNull();
    r.env.focused = () => true;
    await click("Continue confirmation");
    await screen.findByRole("heading", { name: "Protection chosen" });
    expect(auth.createCalls).toBe(1);
  });

  it("other failures show a reason-code error with retry", async () => {
    const auth = new FakeAuthenticator({ prf: "ok" });
    const r = rig(auth);
    const vault: DeviceVault = { ...r.vault, createProfileVault: vi.fn().mockResolvedValueOnce({ ok: false, reason: "storage_failed" }).mockImplementation((p: ProfileId, o?: CreateVaultOptions) => r.vault.createProfileVault(p, o)) };
    render(ui({ ...r, vault }, makePlatform()));
    await click("Get started");
    await click("Protect this device");
    expect((await screen.findByRole("alert")).textContent).toBe("Error: storage_failed");
    await click("Try again");
    await screen.findByRole("heading", { name: "Protection chosen" });
  });

  it("already_exists never overwrites, recreates or deletes; it only offers the recovery flow", async () => {
    const r = rig();
    const create = vi.fn().mockResolvedValue({ ok: false, reason: "already_exists" });
    const del = vi.fn();
    const rotate = vi.fn();
    const vault = { ...r.vault, createProfileVault: create, deleteProfileVault: del, rotateWrapping: rotate } as DeviceVault;
    const onExisting = vi.fn();
    render(ui({ ...r, vault }, makePlatform(), { onExistingVault: onExisting }));
    await click("Get started");
    await click("Protect this device");
    expect((await screen.findByRole("alert")).textContent).toBe("This browser already has an OM vault.");
    expect(screen.getAllByRole("button").map((b) => b.textContent)).toEqual(["Open recovery"]);
    await click("Open recovery");
    expect(onExisting).toHaveBeenCalledWith("already_exists");
    expect(create).toHaveBeenCalledTimes(1);
    expect(del).not.toHaveBeenCalled();
    expect(rotate).not.toHaveBeenCalled();
    expect((await r.h.pendingOperations(r.ctx)).filter((o) => o.intentKind === "CreateIdentityProfile")).toHaveLength(0);
  });

  it("corrupt never offers to recreate or delete", async () => {
    const r = rig();
    const create = vi.fn().mockResolvedValue({ ok: false, reason: "corrupt" });
    const del = vi.fn();
    const vault = { ...r.vault, createProfileVault: create, deleteProfileVault: del } as DeviceVault;
    render(ui({ ...r, vault }, makePlatform(), { onExistingVault: vi.fn() }));
    await click("Get started");
    await click("Protect this device");
    expect((await screen.findByRole("alert")).textContent).toBe("The OM vault here is damaged.");
    expect(screen.getAllByRole("button").map((b) => b.textContent)).toEqual(["Open recovery"]);
    expect(create).toHaveBeenCalledTimes(1);
    expect(del).not.toHaveBeenCalled();
  });

  it("an identity failure after the vault exists retries only the identity, not the vault", async () => {
    const auth = new FakeAuthenticator({ prf: "ok", backupEligible: false });
    const r = rig(auth);
    // A taken label slot makes the first create-profile conflict: pre-create the identity directly.
    await r.h.dispatch({ kind: "CreateIdentityProfile", ctx: r.ctx, operationId: "pre" as never, label: "Pre" });
    render(ui(r, makePlatform()));
    await click("Get started");
    await click("Protect this device");
    expect((await screen.findByRole("alert")).textContent).toBe("Could not create your identity.");
    await click("Retry identity");
    await screen.findByRole("alert");
    expect(auth.createCalls).toBe(1);
  });
});

describe("recovery_key", () => {
  it("verification gates the confirm button; wrong groups keep it disabled", async () => {
    const r = rig();
    await toRecovery(r, makePlatform());
    const groups = keyGroups();
    await verifyWith(groups, true);
    expect(screen.getByRole("button", { name: "I saved it" })).toHaveProperty("disabled", true);
    expect(screen.getAllByRole("textbox")).toHaveLength(3);
    for (const input of screen.getAllByRole("textbox")) {
      const n = Number(/Group (\d+)/.exec(input.closest("label")?.textContent ?? "")?.[1]);
      await userEvent.clear(input);
      await userEvent.type(input, (groups[n - 1] ?? "").toLowerCase());
    }
    expect(screen.getByRole("button", { name: "I saved it" })).toHaveProperty("disabled", false);
  });

  it("the key is not retrievable from Core after confirm and never reaches storage, logs or operation records", async () => {
    const logs = [vi.spyOn(console, "log"), vi.spyOn(console, "info"), vi.spyOn(console, "warn"), vi.spyOn(console, "error"), vi.spyOn(console, "debug")];
    const r = rig();
    const { groups } = await toInviteFirst(r, makePlatform());
    const secret = groups.slice(3);
    expect(secret.every((g) => g.length === 4)).toBe(true);
    const everything = JSON.stringify([
      Object.entries(window.localStorage), Object.entries(window.sessionStorage), document.cookie, await r.h.pendingOperations(r.ctx), logs.map((l) => l.mock.calls),
    ]);
    for (const g of secret) expect(everything).not.toContain(g);
    // Core will not show it again, and it is gone from the DOM.
    const ops = await r.h.pendingOperations(r.ctx);
    const creating = ops.find((o) => o.intentKind === "CreateIdentityProfile");
    expect(await r.h.query({ kind: "GetRecoveryKey", ctx: r.ctx, keyOperationId: creating?.operationId as never })).toBeNull();
    for (const g of secret) expect(document.body.textContent).not.toContain(g);
  });

  it("copy uses the sensitive clipboard when available", async () => {
    const r = rig();
    const tp = makePlatform({ clipboard: "sensitive" });
    await toRecovery(r, tp);
    await click("Copy key");
    expect(await screen.findByText("Copied")).toBeTruthy();
    expect(tp.written).toEqual([keyGroups().join("-")]);
  });

  it("without sensitive clipboard support there is no copy button, only the notice", async () => {
    for (const clipboard of ["unsupported", "unavailable"] as const) {
      cleanup();
      const r = rig();
      const tp = makePlatform({ clipboard });
      const { container } = await toRecovery(r, tp);
      expect(screen.queryByRole("button", { name: "Copy key" })).toBeNull();
      expect(screen.getByText(/Copy is not available here/)).toBeTruthy();
      expect(tp.written).toEqual([]);
      expect(await axeViolations(container)).toEqual([]);
    }
  });
});

describe("invite_first", () => {
  const LINK = "om:invite/AAAAAAAAAAAAAAAAAAAAAA";

  it("zero-contact state offers the four invite-first actions and hides QR/scan without their slots", async () => {
    const r = rig();
    const tp = makePlatform({ share: "unavailable", camera: "granted" });
    const { view } = await toInviteFirst(r, tp);
    expect(screen.getByRole("button", { name: "Share invite link" })).toBeTruthy();
    expect(screen.getByRole("button", { name: "Accept invitation" })).toBeTruthy();
    expect(screen.queryByRole("button", { name: "Show my QR" })).toBeNull();
    expect(screen.queryByRole("button", { name: "Scan QR" })).toBeNull();
    expect(screen.getByRole("group", { name: "Add your first contact" }).getAttribute("data-hidden-actions")).toBe("qr_renderer_missing scan_slot_missing");
    expect(await axeViolations(view.container)).toEqual([]);
    expect(tp.contactsTouched.n).toBe(0);
  });

  it("share falls back to showing the link when Share is unavailable", async () => {
    const r = rig();
    await toInviteFirst(r, makePlatform({ share: "unavailable" }));
    await click("Share invite link");
    expect((await screen.findByText(/^om:invite\//)).textContent).toMatch(/^om:invite\/[A-Za-z0-9_-]{22}$/);
    expect(screen.getByText("Expires at 604801000")).toBeTruthy();
  });

  it("share uses the platform Share capability with the invite link", async () => {
    const r = rig();
    const tp = makePlatform({ share: "shared" });
    await toInviteFirst(r, tp);
    await click("Share invite link");
    await waitFor(() => { expect(tp.shared).toHaveLength(1); });
    expect(tp.shared[0]).toMatchObject({ title: "Join me", url: expect.stringMatching(/^om:invite\//) as string });
  });

  it("QR and scan show when their slots exist; camera permission is requested only on click", async () => {
    const r = rig();
    const tp = makePlatform({ camera: "granted", share: "unavailable" });
    const scanQr = vi.fn().mockResolvedValue(LINK);
    const { view } = await toInviteFirst(r, tp, { renderQr: (p: string) => <svg role="img" aria-label={`qr:${p}`} />, scanQr });
    expect(tp.cameraCalls.n).toBe(0);
    expect(scanQr).not.toHaveBeenCalled();
    await click("Show my QR");
    expect(await screen.findByRole("img", { name: /^qr:om:invite\// })).toBeTruthy();
    expect(await axeViolations(view.container)).toEqual([]);
    await click("Scan QR");
    expect(tp.cameraCalls.n).toBe(1);
    await screen.findByRole("heading", { name: "You are ready" });
  });

  it("scan is hidden when the camera is unavailable and reports denial otherwise", async () => {
    const r = rig();
    await toInviteFirst(r, makePlatform({ camera: "unavailable" }), { scanQr: vi.fn() });
    expect(screen.queryByRole("button", { name: "Scan QR" })).toBeNull();
    expect(screen.getByRole("group", { name: "Add your first contact" }).getAttribute("data-hidden-actions")).toContain("camera_unavailable");
    cleanup();
    const r2 = rig();
    const scanQr = vi.fn();
    await toInviteFirst(r2, makePlatform({ camera: "denied" }), { scanQr });
    await click("Scan QR");
    expect((await screen.findByRole("alert")).textContent).toBe("Camera not allowed");
    expect(scanQr).not.toHaveBeenCalled();
  });

  it("accepting rejects anything but a strict om: link without dispatching or fetching", async () => {
    const fetchSpy = vi.fn();
    vi.stubGlobal("fetch", fetchSpy);
    const r = rig();
    await toInviteFirst(r, makePlatform());
    const before = (await r.h.pendingOperations(r.ctx)).length;
    const box = screen.getByRole("textbox", { name: "Invitation link" });
    const cases: readonly [string, string][] = [
      ["https://evil.example/om:invite/AAAAAAAAAAAAAAAAAAAAAA", "That is not an OM invitation"],
      ["om:invite/short", "That is not an OM invitation"],
      [`om:invite/${"A".repeat(400)}`, "That link is too long"],
    ];
    for (const [text, message] of cases) {
      await userEvent.clear(box);
      await userEvent.click(box);
      await userEvent.paste(text);
      await click("Accept invitation");
      expect((await screen.findByRole("alert")).textContent).toBe(message);
    }
    await userEvent.clear(box);
    await click("Accept invitation");
    expect((await screen.findByRole("alert")).textContent).toBe("Enter a link");
    expect((await r.h.pendingOperations(r.ctx)).length).toBe(before);
    expect(fetchSpy).not.toHaveBeenCalled();
    vi.unstubAllGlobals();
  });

  it("a valid pasted link is accepted through Core and moves to ready", async () => {
    const r = rig();
    await toInviteFirst(r, makePlatform());
    await userEvent.click(screen.getByRole("textbox", { name: "Invitation link" }));
    await userEvent.paste(`  ${LINK}  `);
    await click("Accept invitation");
    await screen.findByRole("heading", { name: "You are ready" });
    expect((await r.h.pendingOperations(r.ctx)).some((o) => o.intentKind === "AcceptInvitation" && o.outcome.kind === "Committed")).toBe(true);
  });

  it("an invite Core rejects stays on the step with the reason", async () => {
    const r = rig();
    await toInviteFirst(r, makePlatform({ share: "unavailable" }));
    await click("Share invite link");
    const own = (await screen.findByText(/^om:invite\//)).textContent;
    await userEvent.click(screen.getByRole("textbox", { name: "Invitation link" }));
    await userEvent.paste(own);
    await click("Accept invitation");
    expect((await screen.findByRole("alert")).textContent).toBe("Invitation rejected: invalid");
  });
});

describe("Home facts", () => {
  it("are shown on demand through copy, with no vendor names", async () => {
    const r = rig();
    const { view } = await toInviteFirst(r, makePlatform());
    expect(screen.queryByText("Home tier T1")).toBeNull();
    await click("Where is my Home?");
    expect(await screen.findByText("Home tier T1")).toBeTruthy();
    expect(screen.getByText("Observer: host_sees_metadata")).toBeTruthy();
    expect(screen.getByText("You can move your Home.")).toBeTruthy();
    expect(screen.getByText("Origin: independent_origin_available")).toBeTruthy();
    expect(await axeViolations(view.container)).toEqual([]);
    await click("Where is my Home?");
    expect(screen.queryByText("Home tier T1")).toBeNull();
  });
});

describe("no address-book path", () => {
  it("never reads contactsLocal across the whole flow (the platform subset type excludes it too)", async () => {
    const r = rig();
    const tp = makePlatform({ share: "shared", camera: "granted" });
    await toInviteFirst(r, tp, { renderQr: () => <span />, scanQr: () => Promise.resolve(null) });
    await click("Skip for now");
    await screen.findByRole("heading", { name: "You are ready" });
    expect(tp.contactsTouched.n).toBe(0);
  });
});
