// SPDX-License-Identifier: MPL-2.0
// How the Core's stand-in mailbox (one random 16-byte id shared with the peer) becomes the Home API's separate
// capabilities. See docs/reports/2026-10-28-relay-status.md "Deviations" (D1..D3).
import { concat, sha256, utf8 } from "@om/home-wire";
import type { SecretStore } from "./secrets";

const SECRET = "install-secret-v1";

export async function installSecret(store: SecretStore): Promise<Uint8Array> {
  const have = await store.get(SECRET);
  if (have && have.length === 32) return have;
  const fresh = crypto.getRandomValues(new Uint8Array(32));
  await store.put(SECRET, fresh);
  // Re-read: two tabs racing to create the secret must end up with one value.
  return (await store.get(SECRET)) ?? fresh;
}

async function hmac(secret: Uint8Array, label: string, data: Uint8Array): Promise<Uint8Array> {
  const key = await crypto.subtle.importKey("raw", secret.slice(), { name: "HMAC", hash: "SHA-256" }, false, ["sign"]);
  return new Uint8Array(await crypto.subtle.sign("HMAC", key, concat(utf8(label), data)));
}

export interface MailboxCaps {
  /** Routing prefix shared by every capability of this queue (a public hash of the mailbox id, not a secret). */
  readonly queueId: Uint8Array;
  /** `queueId || mailbox`: shared with the peer, who is the only other party that may deposit. */
  readonly deposit: Uint8Array;
  readonly receive: Uint8Array;
  readonly admin: Uint8Array;
}

const queueIdOf = async (mailbox: Uint8Array): Promise<Uint8Array> => (await sha256(utf8("om-relay-queue-v0"), mailbox)).slice(0, 16);

export async function mailboxCaps(secret: Uint8Array, mailbox: Uint8Array): Promise<MailboxCaps> {
  const queueId = await queueIdOf(mailbox);
  return {
    queueId,
    deposit: concat(queueId, mailbox),
    receive: concat(queueId, await hmac(secret, "om-relay-receive-v0", mailbox)),
    admin: concat(queueId, await hmac(secret, "om-relay-admin-v0", mailbox)),
  };
}

/** The sender side needs only the deposit capability, which needs no install secret. */
export async function depositCap(mailbox: Uint8Array): Promise<Uint8Array> {
  return concat(await queueIdOf(mailbox), mailbox);
}

export const keyPublishCap = (secret: Uint8Array, deviceId: Uint8Array): Promise<Uint8Array> => hmac(secret, "om-key-publish-v0", deviceId);
export const descriptorPublishCap = (secret: Uint8Array, id: Uint8Array): Promise<Uint8Array> => hmac(secret, "om-descriptor-publish-v0", id);

/** Deterministic, content-bound request id: an exact retry of the same operation, even after a restart, carries the same one. */
export async function requestId(label: string, ...parts: readonly Uint8Array[]): Promise<Uint8Array> {
  return (await sha256(utf8(`om-request-id-v0:${label}`), ...parts.flatMap((p) => [new Uint8Array([(p.length >>> 24) & 255, (p.length >>> 16) & 255, (p.length >>> 8) & 255, p.length & 255]), p]))).slice(0, 16);
}
