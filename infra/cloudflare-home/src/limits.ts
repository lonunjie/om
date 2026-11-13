// SPDX-License-Identifier: AGPL-3.0-or-later
// Operational limits of this Home. They are implementation parameters, never protocol constants (CLOUDFLARE_REFERENCE_DEPLOYMENT.md);
// they are advertised in ProviderCapabilities.limits and every one fails with a typed `quota`, `payload_too_large` or `rate_limited`.

import type { Env } from "./env";

export const DAY = 86_400;

export const LIMITS = {
  /** Largest SealedDeliveryEnvelope: 131088-byte ciphertext, 1120-byte HPKE enc and a few dozen bytes of framing. */
  maxDepositBytes: 133_000,
  maxQueueItems: 512,
  maxQueueBytes: 16 * 1024 * 1024,
  /** Retention bound; a sealed wrapper's own authenticated expiry may be shorter, never longer (SEALED_SENDER_DELIVERY_PROFILE section 7). */
  maxRetentionSeconds: 14 * DAY,
  defaultRetentionSeconds: 7 * DAY,
  /** spec/registries/om-time-fields.json default skew. */
  skewSeconds: 300,
  /** Per deposit capability token bucket. */
  depositBurst: 30,
  depositRefillPerSecond: 1,
  /** Wake channel (feature om.relay.wake.v1): sockets one queue holds at once, authenticated or not. */
  maxWakeSockets: 8,
  drainPageItems: 64,
  drainPageBytes: 1024 * 1024,
  maxDepositCapabilities: 64,
  /** Rows of the replay set and of the request_id idempotency table kept per queue (oldest dropped first): a hostile but valid depositor cannot grow storage without bound. */
  maxReplayRows: 8192,
  maxCapabilityRows: 512,
  /** request_id idempotency horizon across restarts (home-api.md section 5). */
  idempotencySeconds: 7 * DAY,
  rotateOverlapSeconds: DAY,
  rotateResultSeconds: 600,
  /**
   * Blob spool (feature om.blob.spool.v1), deliberately small: a free-plan Home keeps no card and no R2, so the spool is a
   * temporary hand-over area for small attachments, not storage. 32 MiB holds a few photos for a week; one object is at most
   * 4 MiB so a put stays well inside a Worker's 128 MB and a Durable Object's request budget. Larger files travel directly
   * between the two devices or through a BlobStore the person enables (STORAGE_AND_BLOB_ARCHITECTURE). Operational values, never protocol constants.
   */
  maxBlobSpoolBytes: 32 * 1024 * 1024,
  maxBlobBytes: 4 * 1024 * 1024,
  maxBlobs: 64,
  /** Key pool. */
  maxOneTimePerKind: 100,
  maxBundleBytes: 65_536,
  /** Relationship fetch capabilities one device pool holds (one per accepted relationship). */
  maxFetchCapabilities: 256,
  fetchBurst: 8,
  fetchRefillSeconds: 600,
  /** Descriptor store. */
  maxDescriptorBytes: 32_768,
  maxDescriptorSeconds: 30 * DAY,
  descriptorReadBurst: 20,
  descriptorReadRefillSeconds: 5,
  /** IntroGate. */
  maxTicketsPerService: 256,
  maxTicketUses: 16,
  maxTicketSeconds: 14 * DAY,
  maxTicketPayload: 65_536,
  maxPendingRequests: 256,
  submitBurst: 20,
  submitRefillPerSecond: 2,
} as const;

export type Limits = { readonly [K in keyof typeof LIMITS]: number };

/**
 * The `personal` profile (OM_PROFILE=personal): one owner and a handful of mailboxes, so queue items, queue bytes, deposit
 * capabilities and the blob spool are smaller than the multi-person defaults above. Only these keys differ; everything else is
 * LIMITS. Operational values, never protocol constants; advertised in ProviderCapabilities.limits.
 */
export const PERSONAL_LIMITS: Partial<Limits> = {
  maxQueueItems: 128,
  maxQueueBytes: 8 * 1024 * 1024,
  maxDepositCapabilities: 16,
  maxBlobSpoolBytes: 16 * 1024 * 1024,
  maxBlobs: 32,
};

/**
 * Usage ceilings the Home measures itself against under the personal profile (per UTC day, the reset of the free-plan request
 * allowances). CHECK THESE AGAINST CLOUDFLARE'S PUBLISHED FREE-PLAN NUMBERS ON THE DAY THEY ARE DEPLOYED: they are copied from
 * the plan as last known (Workers 100,000 requests/day; Durable Objects 100,000 requests/day) and Cloudflare changes them. A
 * deployment may override either with OM_USAGE_CEILING_WORKER / OM_USAGE_CEILING_DO (positive integers).
 * `soft` from softRatio of a ceiling, `hard` from hardRatio (new deposits are shed below the real limit so drains and acks still fit).
 */
export const USAGE_CONFIG = {
  workerRequestsPerDay: 100_000,
  durableObjectRequestsPerDay: 100_000,
  softRatio: 0.8,
  hardRatio: 0.95,
} as const;

export type ProfileName = "default" | "personal";

/** `personal` selects the personal profile. Unset, empty or `default` is the default profile. Any other value is a typo and gets the stricter personal profile, never silently the looser one. */
export const profileOf = (env: Env): ProfileName => {
  const p = (env.OM_PROFILE ?? "").trim().toLowerCase();
  return p === "" || p === "default" ? "default" : "personal";
};

export const limitsOf = (env: Env): Limits => (profileOf(env) === "personal" ? { ...LIMITS, ...PERSONAL_LIMITS } : LIMITS);
