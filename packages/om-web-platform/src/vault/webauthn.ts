// WebAuthn PRF ceremonies, from the measured V-006 probe (verification/prototypes/v006-web-vault).
import type { CredentialBackup } from "@om/platform-api";
import type { Bytes, VaultEnv } from "./env";
import { randomBytes } from "./crypto";

/** Bounded wait for document focus before every ceremony (Safari: "The document is not focused" ~1.4 s after a prompt). */
export const FOCUS_WAIT_MS = 3000;
const FOCUS_POLL_MS = 100;
const CEREMONY_TIMEOUT_MS = 60000;

export async function waitForFocus(env: VaultEnv): Promise<boolean> {
  const t0 = env.now();
  while (!env.hasFocus()) {
    if (env.now() - t0 >= FOCUS_WAIT_MS) return false;
    await env.sleep(FOCUS_POLL_MS);
  }
  return true;
}

export type CeremonyFailure = { readonly ok: false; readonly reason: "needs_user_gesture" | "unavailable" | "failed" };

const asRecord = (v: unknown): Record<string, unknown> | undefined =>
  typeof v === "object" && v !== null ? (v as Record<string, unknown>) : undefined;

/** Copies ArrayBuffer/view bytes; with `wipe`, zeroes the source buffer we were handed. */
function toBytes(v: unknown, wipe = false): Bytes | undefined {
  let view: Uint8Array | undefined;
  if (v instanceof ArrayBuffer) view = new Uint8Array(v);
  else if (ArrayBuffer.isView(v)) view = new Uint8Array(v.buffer, v.byteOffset, v.byteLength);
  if (view === undefined) return undefined;
  const out = new Uint8Array(view.length);
  out.set(view);
  if (wipe) view.fill(0);
  return out;
}

function callMethod(target: unknown, name: string): unknown {
  const fn = asRecord(target)?.[name];
  return typeof fn === "function" ? (fn as (this: unknown) => unknown).call(target) : undefined;
}

const prfResult = (cred: unknown): Record<string, unknown> | undefined => asRecord(asRecord(callMethod(cred, "getClientExtensionResults"))?.["prf"]);

/** Authenticator-data flags byte (offset 32): BE = 0x08. No authenticator data => `unknown` (treated as synced). */
function backupOf(authData: Bytes | undefined): CredentialBackup {
  if (authData === undefined || authData.length < 37) return "unknown";
  return ((authData[32] ?? 0) & 0x08) === 0 ? "device_bound" : "synced";
}

export interface CreatedCredential {
  readonly credentialId: Bytes;
  readonly prfEnabled: boolean;
  readonly backup: CredentialBackup;
}

export async function createVaultCredential(
  env: VaultEnv, user: { readonly name: string; readonly displayName: string }, prfIn: Bytes,
): Promise<{ readonly ok: true; readonly value: CreatedCredential } | CeremonyFailure> {
  if (env.credentials === undefined) return { ok: false, reason: "unavailable" };
  if (!(await waitForFocus(env))) return { ok: false, reason: "needs_user_gesture" };
  let cred: unknown;
  try {
    cred = await env.credentials.create({ publicKey: {
      rp: { name: env.rpName, id: env.rpId },
      user: { id: randomBytes(env.crypto, 16), name: user.name, displayName: user.displayName },
      challenge: randomBytes(env.crypto, 32),
      pubKeyCredParams: [{ type: "public-key", alg: -7 }, { type: "public-key", alg: -257 }],
      authenticatorSelection: { residentKey: "preferred", userVerification: "required" },
      timeout: CEREMONY_TIMEOUT_MS,
      extensions: { prf: { eval: { first: prfIn } } },
    } });
  } catch {
    return { ok: false, reason: "failed" };
  }
  const credentialId = toBytes(asRecord(cred)?.["rawId"]);
  if (credentialId === undefined || credentialId.length === 0) return { ok: false, reason: "failed" };
  const prf = prfResult(cred);
  // An output at create is not the acceptance test (a later get() is); wipe it.
  toBytes(asRecord(prf?.["results"])?.["first"], true);
  const authData = toBytes(callMethod(asRecord(cred)?.["response"], "getAuthenticatorData"));
  return { ok: true, value: { credentialId, prfEnabled: prf?.["enabled"] === true, backup: backupOf(authData) } };
}

/** `output` is undefined when the authenticator returned no PRF result (third-party providers, R-391). */
export async function evaluateVaultPrf(
  env: VaultEnv, rpId: string, credentialId: Bytes, prfIn: Bytes,
): Promise<{ readonly ok: true; readonly output: Bytes | undefined } | CeremonyFailure> {
  if (env.credentials === undefined) return { ok: false, reason: "unavailable" };
  if (!(await waitForFocus(env))) return { ok: false, reason: "needs_user_gesture" };
  let assertion: unknown;
  try {
    assertion = await env.credentials.get({ publicKey: {
      challenge: randomBytes(env.crypto, 32),
      rpId,
      userVerification: "required",
      allowCredentials: [{ type: "public-key", id: credentialId }],
      timeout: CEREMONY_TIMEOUT_MS,
      extensions: { prf: { eval: { first: prfIn } } },
    } });
  } catch {
    return { ok: false, reason: "failed" };
  }
  return { ok: true, output: toBytes(asRecord(prfResult(assertion)?.["results"])?.["first"], true) };
}

export function base64url(b: Uint8Array): string {
  let s = "";
  for (const v of b) s += String.fromCharCode(v);
  return btoa(s).replace(/\+/g, "-").replace(/\//g, "_").replace(/=+$/, "");
}
