// What Settings > Lock can do, over the active identity's vault. Kept apart from the screen so each step can be tested without a DOM.
import type { DeviceVault, ProfileId, VaultErrorReason, VaultWrappingInfo } from "@om/platform-api";
import { passphraseBytes } from "@om/ui";
import type { IdleMinutes } from "./lockPrefs";

export type LockResult = { readonly ok: true; readonly wrapping: VaultWrappingInfo } | { readonly ok: false; readonly reason: VaultErrorReason };

export interface LockServices {
  /** How the active identity's vault is wrapped on this device; null when it cannot be read. */
  readonly wrapping: () => Promise<VaultWrappingInfo | null>;
  /** Adds a password, or changes it (`current` is needed when one is set). */
  readonly setPassword: (next: string, current?: string) => Promise<LockResult>;
  readonly removePassword: (current: string) => Promise<LockResult>;
  /** Switches to the device's own prompt (fingerprint, face, code). The browser may not offer one: the result then still says "device key". */
  readonly useDevicePrompt: (signal?: AbortSignal) => Promise<LockResult>;
  /** Removes every lock: the password if there is one, then the passkey. May need two presses (each prompt wants its own click). */
  readonly stopLocking: (current?: string) => Promise<LockResult>;
  /** Locks every identity that has something to unlock with, now. */
  readonly lockNow: () => Promise<void>;
  readonly idleMinutes: () => IdleMinutes;
  readonly setIdleMinutes: (minutes: IdleMinutes) => void;
}

export interface LockServicesInit {
  readonly vault: DeviceVault;
  readonly profileId: ProfileId;
  readonly passkeyUser: { readonly name: string; readonly displayName: string };
  readonly lockOpen: () => Promise<void>;
  readonly idleMinutes: () => IdleMinutes;
  readonly setIdleMinutes: (minutes: IdleMinutes) => void;
}

export function createLockServices({ vault, profileId, passkeyUser, lockOpen, idleMinutes, setIdleMinutes }: LockServicesInit): LockServices {
  const factor = (value: string) => ({ kind: "passphrase" as const, secret: passphraseBytes(value) });
  const info = async (): Promise<LockResult> => {
    const r = await vault.wrapping(profileId);
    return r.ok ? { ok: true, wrapping: r.wrapping } : { ok: false, reason: r.reason };
  };
  return {
    wrapping: async () => { const r = await vault.wrapping(profileId); return r.ok ? r.wrapping : null; },
    setPassword: async (next, current) => {
      const r = await vault.changePassphrase(profileId, { next: factor(next), ...(current === undefined ? {} : { current: factor(current) }) });
      return r.ok ? r : { ok: false, reason: r.reason };
    },
    removePassword: async (current) => {
      const r = await vault.changePassphrase(profileId, { current: factor(current) });
      return r.ok ? r : { ok: false, reason: r.reason };
    },
    useDevicePrompt: async (signal) => {
      const r = await vault.rotateWrapping(profileId, { wrapping: "auto", passkeyUser, ...(signal === undefined ? {} : { signal }) });
      return r.ok ? r : { ok: false, reason: r.reason };
    },
    stopLocking: async (current) => {
      const now = await info();
      if (!now.ok) return now;
      if (now.wrapping.passphraseProtected) {
        const r = await vault.changePassphrase(profileId, { ...(current === undefined ? {} : { current: factor(current) }) });
        if (!r.ok) return { ok: false, reason: r.reason };
      }
      if (now.wrapping.method === "webauthn-prf") {
        const r = await vault.rotateWrapping(profileId, { wrapping: "device-key" });
        if (!r.ok) return { ok: false, reason: r.reason };
      }
      return info();
    },
    lockNow: lockOpen,
    idleMinutes,
    setIdleMinutes,
  };
}
