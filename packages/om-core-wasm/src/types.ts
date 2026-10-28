// Wire types shared with crates/om-core (host.rs). Binary values are lower-case hex strings.
import type { RejectReason } from "@om/platform-api";

export type NetCall =
  | { readonly call: "publish_key_material"; readonly deviceId: string; readonly bundle: string; readonly opks: readonly string[]; readonly pqopks: readonly string[] }
  | { readonly call: "fetch_key_material"; readonly deviceId: string }
  | { readonly call: "put_invite"; readonly inviteId: string; readonly descriptor: string; readonly expiresAt: number }
  | { readonly call: "get_invite"; readonly inviteId: string }
  | { readonly call: "revoke_invite"; readonly inviteId: string }
  | { readonly call: "deposit"; readonly mailbox: string; readonly bytes: string }
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
}

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
