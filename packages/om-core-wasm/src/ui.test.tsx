// @vitest-environment jsdom
// The existing onboarding UI (om-ui OnboardingFlow, unchanged) running against the real Core instead of the harness:
// the typed intents/queries/outcomes are the same, only the Core behind the CoreClient differs (task C1, mechanical swap).
import { existsSync, readFileSync } from "node:fs";
import { join } from "node:path";
import { cleanup, render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, describe, expect, it, vi } from "vitest";
import { FAMILY_NAMES, asProfileId, available, unavailable } from "@om/platform-api";
import type { CapabilityEntry, CapabilityReport, FamilyName, PlatformAdapter } from "@om/platform-api";
import { OnboardingFlow, createActionRegistry } from "@om/ui";
import { createWebDeviceVault } from "@om/web-platform";
import { FakeAuthenticator, fakeEnv } from "../../om-web-platform/src/vault/__fixtures__/fake-env";
import { testCopy } from "../../om-ui/src/onboarding/__fixtures__/copy";
import { createCoreRuntime, instantiateCore } from "./index";
import { createMemoryKv, createMemoryProvider } from "./test-support";

const WASM = join(import.meta.dirname, "../dist/om_wasm.wasm");
afterEach(() => { cleanup(); });

function platform(): PlatformAdapter {
  const entry = (state: CapabilityEntry["state"]): CapabilityEntry => ({ state });
  const families = Object.fromEntries(FAMILY_NAMES.map((f) => [f, entry("unavailable")])) as Record<FamilyName, CapabilityEntry>;
  const report: CapabilityReport = { families, backgroundPush: "none", localDiscovery: "manual_known_endpoint", localContacts: "unavailable", callSystemIntegration: false };
  return {
    capabilities: () => report,
    share: () => unavailable<never>(),
    camera: () => unavailable<never>(),
    clipboard: () => available({ write: () => Promise.resolve("written" as const) }),
  } as unknown as PlatformAdapter;
}

// jsdom quirk shared with om-ui's own tests: WebCrypto ArrayBuffers come from another realm. Re-wrap them.
const foreign = (v: unknown): v is ArrayBuffer => Object.prototype.toString.call(v) === "[object ArrayBuffer]" && !(v instanceof ArrayBuffer);
function sameRealm(v: unknown): unknown {
  if (foreign(v)) return new Uint8Array(v).slice().buffer;
  if (typeof v === "function") return (...a: unknown[]): unknown => sameRealm((v as (...x: unknown[]) => unknown)(...a));
  if (v instanceof Promise) return v.then(sameRealm);
  if (typeof v === "object" && v !== null && Object.getPrototypeOf(v) === Object.prototype) return Object.fromEntries(Object.entries(v).map(([k, x]) => [k, sameRealm(x)]));
  return v;
}

describe.skipIf(!existsSync(WASM))("OnboardingFlow against the real Core", () => {
  it("creates an identity, shows the real recovery key once and ends with empty Chats", async () => {
    const env = fakeEnv({ auth: new FakeAuthenticator({ prf: "ok", backupEligible: false }) });
    const vault = createWebDeviceVault(env.credentials === undefined ? env : { ...env, credentials: sameRealm(env.credentials) as NonNullable<typeof env.credentials> });
    const core = await instantiateCore(readFileSync(WASM));
    const rt = await createCoreRuntime({ core, provider: createMemoryProvider(), storage: () => createMemoryKv(), home: { tier: "T1", observers: ["host_sees_ciphertext"], migrationAvailable: true, clientOriginChoice: ["default_origin"] }, autoDrain: false });
    const ctx = await rt.openProfile(asProfileId("me"));
    const done = vi.fn();
    render(<OnboardingFlow client={rt} registry={createActionRegistry()} ctx={ctx} vault={vault} platform={platform()} copy={testCopy} onDone={done as () => void} />);
    const click = (name: string | RegExp) => userEvent.click(screen.getByRole("button", { name }));
    await click("Get started");
    await click("Protect this device");
    await screen.findByRole("heading", { name: "Protection chosen" });
    await click("Continue");
    await screen.findByRole("list", { name: "Recovery key" });
    const groups = within(screen.getByRole("list", { name: "Recovery key" })).getAllByRole("listitem").map((li) => li.textContent.replace(/^Group \d+ of \d+:\s*/, ""));
    expect(groups).toHaveLength(14);
    expect(groups.join("")).toMatch(/^[A-Z2-7]{56}$/); // a real random key, not the harness's OM-DEV-FAKE placeholder
    await click("Check my copy");
    for (const input of screen.getAllByRole("textbox")) {
      const n = Number(/Group (\d+)/.exec(input.closest("label")?.textContent ?? "")?.[1]);
      await userEvent.type(input, groups[n - 1] ?? "");
    }
    await click("I saved it");
    await screen.findByRole("heading", { name: "Add your first contact" });
    await click("Skip for now");
    await screen.findByRole("heading", { name: "You are ready" });
    await click("Open chats");
    expect(done).toHaveBeenCalledTimes(1);
    // Core now holds the identity, and the display-once key is gone for good.
    const keyOp = (await rt.pendingOperations(ctx)).find((o) => o.intentKind === "CreateIdentityProfile");
    expect(keyOp?.outcome).toEqual({ kind: "Committed" });
    expect(await rt.query({ kind: "GetRecoveryKey", ctx, keyOperationId: keyOp?.operationId as never })).toBeNull();
    expect((await rt.query({ kind: "GetRecoveryStatus", ctx })).recoveryKeySaved).toBe(true);
  });
});
