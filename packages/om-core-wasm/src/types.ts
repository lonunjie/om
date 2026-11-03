// Wire types shared with crates/om-core (host.rs). Binary values are lower-case hex strings.
import type { RejectReason } from "@om/platform-api";

export type NetCall =
  | { readonly call: "publish_key_material"; readonly deviceId: string; readonly bundle: string; readonly opks: readonly string[]; readonly pqopks: readonly string[] }
  | { readonly call: "fetch_key_material"; readonly deviceId: string }
  | { readonly call: "put_invite"; readonly inviteId: string; readonly descriptor: string; readonly expiresAt: number }
  | { readonly call: "get_invite"; readonly inviteId: string }
  | { readonly call: "revoke_invite"; readonly inviteId: string }
  /**
   * `mailbox` is the destination's deposit capability exactly as its RelayEntry carried it; `relayUri` is that entry's
   * relay_uri, so an adapter can refuse a destination at a Home other than the one it is configured for. Both are
   * absent/legacy only for a pass-2 destination: no `relayUri`, and `mailbox` is the bare 16-byte mailbox id.
   */
  | { readonly call: "deposit"; readonly mailbox: string; readonly relayUri?: string; readonly bytes: string }
  | { readonly call: "ack"; readonly mailbox: string; readonly seq: number };

/** Provider-neutral failure classes. `timeout` means the bytes may have been delivered (never a rejection). */
export type NetError =
  | { readonly kind: "unavailable" }
  | { readonly kind: "timeout" }
  | { readonly kind: "not_found" }
  | { readonly kind: "rejected"; readonly reason: RejectReason };

export type NetResult =
  | { readonly result: "ok" }
  | { readonly result: "bytes"; readonly bytes: string }
  | { readonly result: "many"; readonly items: readonly string[] }
  | { readonly result: "err"; readonly error: NetError };

export interface NetOp { readonly id: number; readonly call: NetCall }
export interface Inbound { readonly seq: number; readonly bytes: string }
export interface PollTarget { readonly mailbox: string; readonly after: number }
export interface Write { readonly key: string; readonly value: string | null }
export interface Commit { readonly id: number; readonly profileId: string; readonly writes: readonly Write[] }
export interface Record { readonly key: string; readonly value: string }

/**
 * The capability a host supplies for relay/introduction/key-material access. The reference Home is one
 * implementation (provider_reference scope); nothing in Core depends on it. A rejected promise is treated as
 * `unavailable`, never as a definitive failure.
 */
export interface Provider {
  perform(call: NetCall): Promise<NetResult>;
  poll(mailbox: string, after: number): Promise<readonly Inbound[]>;
  /**
   * Allocates routes for `count` new mailboxes: a fresh random 16-byte `mailbox` id (hex) for Core to poll, and the
   * `relayUri` and deposit `capability` (hex) peers will use to reach it. Core binds its delivery keys to exactly
   * these. Local work only (no network). A provider without it (none is configured) leaves Core unable to start
   * anything that needs a mailbox; there is no stand-in.
   */
  allocateRoutes?(count: number): Promise<readonly RouteOffer[]>;
}

/** One new mailbox and the route to it (see Provider.allocateRoutes). */
export interface RouteOffer { readonly mailbox: string; readonly relayUri: string; readonly capability: string }

/** Password-hashing settings for a backup file (Argon2id). The host derives the key with exactly these. */
export interface BackupKdf { readonly mKib: number; readonly t: number; readonly p: number }
/** The settings and fresh random salt (hex) for the next backup file. */
export interface BackupPlan { readonly kdf: BackupKdf; readonly salt: string }
/** The public header of a backup file: what the host needs to derive the key for it. */
export interface BackupHeader { readonly version: number; readonly profile: number; readonly kdf: BackupKdf; readonly salt: string; readonly ciphertextLen: number }
/** What a restore did. */
export interface ImportSummary { readonly chatsAdded: number; readonly chatsUpdated: number; readonly chatsSkipped: number; readonly messagesAdded: number }
/** Every reason a backup file is refused, as Core names it (the error code is `backup_<reason>`). */
export const BACKUP_REFUSALS = ["not_a_backup", "unsupported_version", "unsupported_profile", "unsupported_parameters", "truncated", "malformed", "wrong_password", "tampered", "invalid_content", "too_large"] as const;
export type BackupRefusal = typeof BACKUP_REFUSALS[number];

export class CoreCallError extends Error {
  constructor(readonly code: string, message: string) {
    super(message);
    this.name = "CoreCallError";
  }
}
/** The host store refused a commit: nothing was released and the operation does not exist. */
export class CoreStorageError extends Error {
  constructor(message: string) {
    super(message);
    this.name = "CoreStorageError";
  }
}

const HEX = "0123456789abcdef";
export function toHex(b: Uint8Array): string {
  let s = "";
  for (const x of b) s += HEX.charAt(x >> 4) + HEX.charAt(x & 15);
  return s;
}
export function fromHex(s: string): Uint8Array<ArrayBuffer> {
  if (s.length % 2 !== 0 || !/^[0-9a-f]*$/.test(s)) throw new CoreCallError("invalid", "bad hex");
  const out = new Uint8Array(s.length / 2);
  for (let i = 0; i < out.length; i++) out[i] = parseInt(s.slice(2 * i, 2 * i + 2), 16);
  return out;
}

/** The reason behind a backup refusal, or undefined when the error is something else. */
export function backupRefusal(e: unknown): BackupRefusal | undefined {
  if (!(e instanceof CoreCallError) || !e.code.startsWith("backup_")) return undefined;
  const reason = e.code.slice("backup_".length);
  return (BACKUP_REFUSALS as readonly string[]).includes(reason) ? (reason as BackupRefusal) : undefined;
}
