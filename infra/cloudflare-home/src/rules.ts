// SPDX-License-Identifier: MPL-2.0
// The time-dependent decisions of the relay as pure functions of an explicit `now`, so they are tested without a clock.
import { LIMITS } from "./limits";

export type DepositCapStatus = "ok" | "revoked" | "expired";

export interface DepositCapState {
  readonly revoked: boolean;
  readonly expiresAt: number | null;
  /** Rotation generation this capability was minted in. */
  readonly generation: number;
}

export interface QueueGenerations {
  readonly current: number;
  readonly overlapUntil: number;
}

/** A deposit capability works until revoked or expired; after a rotation the previous generation works only through the overlap. */
export function depositCapStatus(cap: DepositCapState, queue: QueueGenerations, now: number): DepositCapStatus {
  if (cap.revoked) return "revoked";
  if (cap.expiresAt !== null && cap.expiresAt + LIMITS.skewSeconds < now) return "expired";
  if (cap.generation < queue.current && queue.overlapUntil + LIMITS.skewSeconds < now) return "expired";
  return "ok";
}

export type ReceiveStatus = "current" | "previous_in_overlap" | "previous_expired" | "unknown";

export function receiveStatus(presented: string, current: string | undefined, previous: string | undefined, overlapUntil: number, now: number): ReceiveStatus {
  if (current !== undefined && presented === current) return "current";
  if (previous !== undefined && presented === previous) return overlapUntil + LIMITS.skewSeconds >= now ? "previous_in_overlap" : "previous_expired";
  return "unknown";
}

export type SealedExpiry = { readonly ok: true; readonly retainUntil: number } | { readonly ok: false; readonly error: "expired" | "invalid_request" };

/**
 * The wrapper's authenticated `expires_at` bounds retention: already past (beyond skew) is `expired`; further out than the
 * Home's retention bound is refused rather than silently shortened, so a sender never believes in a longer life than it gets.
 */
export function sealedExpiry(expiresAt: number, now: number): SealedExpiry {
  if (expiresAt + LIMITS.skewSeconds < now) return { ok: false, error: "expired" };
  if (expiresAt > now + LIMITS.maxRetentionSeconds + LIMITS.skewSeconds) return { ok: false, error: "invalid_request" };
  return { ok: true, retainUntil: Math.min(expiresAt, now + LIMITS.maxRetentionSeconds) };
}
