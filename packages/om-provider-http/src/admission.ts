// SPDX-License-Identifier: MPL-2.0
// The owner's admission secret for a personal Home (om.admission.v1, ADR-0186, docs/reports/2026-11-03-owner-admission-and-devices.md).
// 32 random bytes made on this device for ONE Home: the Home is deployed holding only `admissionHash(secret)`, so the secret
// is made BEFORE the address is known, kept as "pending" (a reload while the person is still deploying must show the same setup
// value), and bound to the Home's address when that address has answered. It is never sent to, or reused for, any other address.
import { ADMISSION_SECRET_BYTES, admissionHash } from "@om/home-wire";
import type { SecretStore } from "./secrets";

const PENDING = "admission-pending-v1";
const bound = (baseUrl: string): string => `admission-v1:${baseUrl}`;

const valid = (b: Uint8Array | undefined): b is Uint8Array => b !== undefined && b.length === ADMISSION_SECRET_BYTES;

/** The setup value to give a new deployment (lowercase hex, public). The same value is returned until `adoptAdmission` binds it to a Home. */
export async function pendingAdmissionHash(store: SecretStore): Promise<string> {
  let secret = await store.get(PENDING);
  if (!valid(secret)) {
    await store.put(PENDING, crypto.getRandomValues(new Uint8Array(ADMISSION_SECRET_BYTES)));
    secret = await store.get(PENDING); // two tabs racing must end with one value
  }
  if (!valid(secret)) throw new Error("admission secret unavailable");
  return admissionHash(secret);
}

/** Binds the pending secret to this Home's address and retires it, so the next Home gets its own. `false`: there was nothing pending. */
export async function adoptAdmission(store: SecretStore, baseUrl: string): Promise<boolean> {
  const secret = await store.get(PENDING);
  if (!valid(secret)) return false;
  await store.put(bound(baseUrl), secret);
  await store.put(PENDING, new Uint8Array(0));
  return true;
}

/** The secret bound to exactly this address, or `undefined` (a Home that was not set up by this device never receives one). */
export async function readAdmission(store: SecretStore, baseUrl: string): Promise<Uint8Array | undefined> {
  const secret = await store.get(bound(baseUrl));
  return valid(secret) ? secret : undefined;
}
