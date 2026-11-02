import { describe, expect, it } from "vitest";
import { asProfileId } from "@om/platform-api";
import { createVaultWithKdfPolicy } from "../../../../packages/om-web-platform/src/vault/vault";
import { FakeAuthenticator, fakeEnv } from "../../../../packages/om-web-platform/src/vault/__fixtures__/fake-env";
import { createLockServices, type LockServices } from "./lockServices";

const A = asProfileId("profile-a");
const POLICY = { floor: { m: 8, t: 1, p: 1 }, standard: { m: 64, t: 1, p: 1 }, max: { m: 1024, t: 4, p: 1 } };
const PW = "correct horse battery staple";

function rig(auth: FakeAuthenticator | null = null): { readonly services: LockServices; readonly vault: ReturnType<typeof createVaultWithKdfPolicy>; readonly locked: { n: number } } {
  const vault = createVaultWithKdfPolicy(fakeEnv({ auth }), POLICY);
  const locked = { n: 0 };
  const services = createLockServices({
    vault, profileId: A, passkeyUser: { name: "OM", displayName: "OM" }, lockOpen: () => { locked.n++; return Promise.resolve(); },
    idleMinutes: () => null, setIdleMinutes: () => undefined,
  });
  return { services, vault, locked };
}

describe("lock services over a real vault", () => {
  it("adds, changes and removes a password, refusing a wrong current one", async () => {
    const { services, vault } = rig();
    await vault.createProfileVault(A, { wrapping: "device-key" });
    expect(await services.setPassword(PW)).toMatchObject({ ok: true, wrapping: { passphraseProtected: true } });
    expect(await services.setPassword("another horse battery staple", "not the password")).toEqual({ ok: false, reason: "wrong_passphrase" });
    expect(await services.setPassword("another horse battery staple", PW)).toMatchObject({ ok: true });
    expect(await services.removePassword(PW)).toEqual({ ok: false, reason: "wrong_passphrase" });
    expect(await services.removePassword("another horse battery staple")).toMatchObject({ ok: true, wrapping: { passphraseProtected: false } });
  });

  it("stopping the locks needs the password, and removes a password and a passkey one after the other", async () => {
    const auth = new FakeAuthenticator({ prf: "ok", backupEligible: false });
    const { services, vault } = rig(auth);
    await vault.createProfileVault(A, { additionalFactor: { kind: "passphrase", secret: new TextEncoder().encode(PW) } });
    expect(await services.wrapping()).toMatchObject({ method: "webauthn-prf", passphraseProtected: true });
    expect(await services.stopLocking()).toEqual({ ok: false, reason: "passphrase_required" });
    expect(await services.stopLocking(PW)).toMatchObject({ ok: true, wrapping: { method: "device-key", passphraseProtected: false } });
    vault.lock(A);
    expect(await vault.unlock(A)).toEqual({ ok: true }); // nothing to type or touch any more
  });

  it("the device prompt replaces the key kept in the browser; a browser with no passkeys leaves it as it was", async () => {
    const withKeys = rig(new FakeAuthenticator({ prf: "ok", backupEligible: false }));
    await withKeys.vault.createProfileVault(A, { wrapping: "device-key" });
    expect(await withKeys.services.useDevicePrompt()).toMatchObject({ ok: true, wrapping: { method: "webauthn-prf" } });

    const without = rig(null);
    await without.vault.createProfileVault(A, { wrapping: "device-key" });
    expect(await without.services.useDevicePrompt()).toMatchObject({ ok: true, wrapping: { method: "device-key" } });
  });

  it("lock now goes to the host", async () => {
    const { services, locked } = rig();
    await services.lockNow();
    expect(locked.n).toBe(1);
  });
});
