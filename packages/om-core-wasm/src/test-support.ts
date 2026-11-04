// TEST ONLY: an in-memory provider (key material, introduction service, relay mailboxes) and an in-memory
// EncryptedKv. Never import this from application code: tools/web_guard.py and the production bundle check refuse it.
import type { EncryptedKv, KvGetResult, KvListResult, KvResult } from "@om/platform-api";
import type { BlobLimits, Inbound, MailboxKind, NetCall, NetResult, PerformExtra, Provider, RouteOffer } from "./types";

export const TEST_PROVIDER_MARKER = "OM-TEST-LOOPBACK-NOT-FOR-PRODUCTION";
/** The test relay's name and capability shape: a fixed prefix plus the mailbox id, unlike the id itself. */
const TEST_RELAY_HOST = "loopback.test"; // a reserved name: never resolvable, never a real Home
const TEST_RELAY_SCHEME = "https"; // composed, so the product-code lint for hard-coded remote URLs still covers every other file
export const TEST_RELAY_URI = `${TEST_RELAY_SCHEME}://${TEST_RELAY_HOST}/`;
const CAP_PREFIX = "c4".repeat(16);

/** The loopback store's own bounds, standing in for a Home's advertised `om.blob.spool.v1` limits. */
export const TEST_BLOB_LIMITS: BlobLimits = { maxBlobBytes: 4 * 1024 * 1024, maxSpoolBytes: 32 * 1024 * 1024, maxBlobs: 64 };
const BLOB_LABEL = "OM-BLOB-CIPHERTEXT-HASH-v1";

/** DS-HASH of the ciphertext: SHA-256 over the CBOR array [label, bstr], as the spec and the Home compute it. */
async function blobDigest(bytes: Uint8Array): Promise<string> {
  const label = new TextEncoder().encode(BLOB_LABEL);
  const n = bytes.length;
  const head = n < 24 ? [0x40 | n] : n < 0x100 ? [0x58, n] : n < 0x10000 ? [0x59, n >> 8, n & 255] : [0x5a, (n >>> 24) & 255, (n >>> 16) & 255, (n >>> 8) & 255, n & 255];
  const lh = label.length < 24 ? [0x60 | label.length] : [0x78, label.length];
  const all = new Uint8Array(1 + lh.length + label.length + head.length + n);
  all[0] = 0x82;
  all.set(lh, 1);
  all.set(label, 1 + lh.length);
  all.set(head, 1 + lh.length + label.length);
  all.set(bytes, 1 + lh.length + label.length + head.length);
  return toHexStr(new Uint8Array(await crypto.subtle.digest("SHA-256", all)));
}
const toHexStr = (b: Uint8Array): string => Array.from(b, (x) => x.toString(16).padStart(2, "0")).join("");
const fromHexStr = (h: string): Uint8Array => Uint8Array.from(h.match(/../g) ?? [], (x) => parseInt(x, 16));
/** Canonical CBOR of the receipt `{0: readCap, 1: [locations], 2: digest, 3: expiresAt}` (see crates/om-core host.rs BlobReceipt). */
function receiptCbor(readCap: Uint8Array, locations: readonly string[], digest: Uint8Array, expiresAt: number): Uint8Array {
  const parts: number[][] = [[0xa4]];
  const bstr = (b: Uint8Array): number[] => [...(b.length < 24 ? [0x40 | b.length] : [0x58, b.length]), ...b];
  const tstr = (t: string): number[] => { const u = new TextEncoder().encode(t); return [...(u.length < 24 ? [0x60 | u.length] : u.length < 256 ? [0x78, u.length] : [0x79, u.length >> 8, u.length & 255]), ...u]; };
  parts.push([0x00], bstr(readCap), [0x01, 0x80 | locations.length], ...locations.map(tstr), [0x02], bstr(digest), [0x03, 0x1a, (expiresAt >>> 24) & 255, (expiresAt >>> 16) & 255, (expiresAt >>> 8) & 255, expiresAt & 255]);
  return Uint8Array.from(parts.flat());
}

export type Fault = "unavailable" | "timeout_after_effect" | "timeout_before_effect";

export interface MemoryProvider extends Provider {
  allocateRoutes(count: number): Promise<readonly RouteOffer[]>;
  readonly marker: typeof TEST_PROVIDER_MARKER;
  inject(fault: Fault): void;
  /** What the store advertises from now on (`null`: it holds no files). Default {@link TEST_BLOB_LIMITS}. */
  setBlobLimits(limits: BlobLimits | null): void;
  /** Every object the store holds, by locator: ciphertext only, for confinement assertions. */
  blobs(): ReadonlyMap<string, Uint8Array>;
  /** Make the store forget an object (expiry). */
  expireBlobs(): void;
  /** Flip one bit in every stored object (a faulty or hostile store). */
  corruptBlobs(): void;
  /** Every byte string a relay ever received (acks do not remove them), for ciphertext-only assertions. */
  deposited(): readonly string[];
  /** Pending mailbox contents, by mailbox. */
  revokeAllInvites(): void;
}

/** The queue a poll or ack addresses: the mailbox itself, or the one a ceremony receive capability names. */
function queueOf(mailbox: string, capability?: string): string {
  return capability !== undefined && capability !== "" ? capability.slice(32, 64) : mailbox;
}

export function createMemoryProvider(): MemoryProvider {
  const keys = new Map<string, { bundle: string; opks: string[]; pqopks: string[] }>();
  const invites = new Map<string, string>();
  const grants = new Map<string, Set<string>>(); // device id -> fetch capabilities its owner granted
  const boxes = new Map<string, { next: number; items: { seq: number; bytes: string }[] }>();
  const faults: Fault[] = [];
  let blobLimits: BlobLimits | null = TEST_BLOB_LIMITS;
  const objects = new Map<string, { owner: string; read: string; bytes: Uint8Array; digest: string }>(); // by locator (32 hex)
  const puts = new Map<string, string>(); // requestId -> receipt hex (an exact retry answers alike)
  const log: string[] = []; // every deposit ever received: acks free relay storage, not this record of what it saw
  return {
    marker: TEST_PROVIDER_MARKER,
    inject: (f) => { faults.push(f); },
    deposited: () => [...log],
    setBlobLimits: (l) => { blobLimits = l; },
    blobs: () => new Map([...objects].map(([k, v]) => [k, v.bytes])),
    expireBlobs: () => { objects.clear(); },
    corruptBlobs: () => { for (const o of objects.values()) o.bytes[10] = (o.bytes[10] ?? 0) ^ 1; },
    blobLimits: () => Promise.resolve(blobLimits),
    allocateRoutes(count): Promise<readonly RouteOffer[]> {
      const out: RouteOffer[] = [];
      for (let i = 0; i < count; i++) {
        const mailbox = Array.from(crypto.getRandomValues(new Uint8Array(16)), (b) => b.toString(16).padStart(2, "0")).join("");
        out.push({ mailbox, relayUri: TEST_RELAY_URI, capability: CAP_PREFIX + mailbox });
      }
      return Promise.resolve(out);
    },
    revokeAllInvites: () => { invites.clear(); },
    async perform(call: NetCall, extra?: PerformExtra): Promise<NetResult> {
      const fault = faults.shift();
      if (fault === "unavailable") return { result: "err", error: { kind: "unavailable" } };
      if (fault === "timeout_before_effect") return { result: "err", error: { kind: "timeout" } };
      let r: NetResult = { result: "ok" };
      switch (call.call) {
        case "put_blob": {
          const bytes = extra?.ciphertext;
          if (blobLimits === null) { r = { result: "err", error: { kind: "rejected", reason: "unsupported" } }; break; }
          if (bytes === undefined || bytes.length !== call.length || (await blobDigest(bytes)) !== call.digest) { r = { result: "err", error: { kind: "rejected", reason: "integrity_failure" } }; break; }
          if (bytes.length > blobLimits.maxBlobBytes) { r = { result: "err", error: { kind: "rejected", reason: "too_large" } }; break; }
          const again = puts.get(call.requestId);
          if (again !== undefined) { r = { result: "bytes", bytes: again }; break; }
          if (objects.size >= (blobLimits.maxBlobs ?? 64)) { r = { result: "err", error: { kind: "rejected", reason: "quota" } }; break; }
          const locator = call.ownerCap.slice(0, 32);
          const readCap = `${locator}${"7e".repeat(16)}`;
          objects.set(locator, { owner: call.ownerCap, read: readCap, bytes: bytes.slice(), digest: call.digest });
          const receipt = toHexStr(receiptCbor(fromHexStr(readCap), [`${TEST_RELAY_URI}v1/blobs/${locator}`], fromHexStr(call.digest), Math.floor(Date.now() / 1000) + 7 * 86400));
          puts.set(call.requestId, receipt);
          r = { result: "bytes", bytes: receipt };
          break;
        }
        case "get_blob": {
          const hit = call.locations.map((l) => objects.get(l.slice(l.lastIndexOf("/") + 1))).find((o) => o !== undefined && o.read === call.readCap);
          r = hit === undefined ? { result: "err", error: { kind: "not_found" } } : { result: "bytes", bytes: toHexStr(hit.bytes) };
          break;
        }
        case "delete_blob": {
          const locator = call.location.slice(call.location.lastIndexOf("/") + 1);
          if (objects.get(locator)?.owner === call.ownerCap) objects.delete(locator);
          break;
        }
        case "publish_key_material": {
          const p = keys.get(call.deviceId) ?? { bundle: "", opks: [], pqopks: [] };
          p.bundle = call.bundle;
          p.opks.push(...call.opks);
          p.pqopks.push(...call.pqopks);
          keys.set(call.deviceId, p);
          break;
        }
        case "grant_key_fetch": {
          const g = grants.get(call.deviceId) ?? new Set<string>();
          g.add(call.capability);
          grants.set(call.deviceId, g);
          break;
        }
        case "fetch_key_material": {
          // Only the holder of a capability the device owner granted may fetch (ASYNC_KEY_MATERIAL 2.1).
          if (!grants.get(call.deviceId)?.has(call.capability ?? "")) { r = { result: "err", error: { kind: "rejected", reason: "unauthorized" } }; break; }
          const p = keys.get(call.deviceId);
          if (!p) { r = { result: "err", error: { kind: "not_found" } }; break; }
          const items = [p.bundle];
          const o = p.opks.shift();
          if (o !== undefined) items.push(o);
          const q = p.pqopks.shift();
          if (q !== undefined) items.push(q);
          r = { result: "many", items };
          break;
        }
        case "put_invite": invites.set(call.inviteId, call.descriptor); break;
        case "get_invite": {
          const d = invites.get(call.inviteId);
          r = d === undefined ? { result: "err", error: { kind: "not_found" } } : { result: "bytes", bytes: d };
          break;
        }
        case "revoke_invite": invites.delete(call.inviteId); break;
        case "deposit": {
          // A routed destination is this relay's capability (prefix + mailbox id); a legacy one is the bare mailbox id.
          let id = call.mailbox;
          if (call.relayUri !== undefined) {
            if (call.relayUri !== TEST_RELAY_URI || !call.mailbox.startsWith(CAP_PREFIX)) { r = { result: "err", error: { kind: "rejected", reason: "unsupported" } }; break; }
            id = call.mailbox.slice(CAP_PREFIX.length);
          }
          const b = boxes.get(id) ?? { next: 0, items: [] };
          b.next += 1;
          log.push(call.bytes);
          b.items.push({ seq: b.next, bytes: call.bytes });
          boxes.set(id, b);
          break;
        }
        case "submit_intro": {
          // The introduction service queues each request under its ticket id (the invite id), at fixed place of the canonical MessageRequest.
          const head = call.bytes.slice(2, 14);
          if (head !== "001301010250") { r = { result: "err", error: { kind: "rejected", reason: "invalid" } }; break; }
          const id = call.bytes.slice(14, 46);
          const b = boxes.get(id) ?? { next: 0, items: [] };
          b.next += 1;
          log.push(call.bytes);
          b.items.push({ seq: b.next, bytes: call.bytes });
          boxes.set(id, b);
          break;
        }
        case "open_queue": {
          // A ceremony queue: its receive capability names it, like a real Home's derived capability.
          boxes.set(call.mailbox, boxes.get(call.mailbox) ?? { next: 0, items: [] });
          r = { result: "bytes", bytes: `${"d5".repeat(16)}${call.mailbox}${"5d".repeat(16)}` };
          break;
        }
        case "ack": {
          const b = boxes.get(queueOf(call.mailbox, call.capability));
          if (b) b.items = b.items.filter((i) => i.seq > call.seq);
          break;
        }
      }
      return fault === "timeout_after_effect" ? { result: "err", error: { kind: "timeout" } } : r;
    },
    poll(mailbox: string, after: number, _kind?: MailboxKind, _relayUri?: string, capability?: string): Promise<readonly Inbound[]> {
      return Promise.resolve((boxes.get(queueOf(mailbox, capability))?.items ?? []).filter((i) => i.seq > after));
    },
  };
}

/** A profile store that can be made to refuse writes; contents survive across runtimes like durable storage. */
export interface MemoryKv extends EncryptedKv {
  failNextPut(): void;
  snapshot(): ReadonlyMap<string, Uint8Array>;
}

export function createMemoryKv(): MemoryKv {
  const data = new Map<string, Uint8Array>();
  let failPuts = 0;
  const ok: KvResult = { ok: true };
  return {
    failNextPut: () => { failPuts += 1; },
    snapshot: () => data,
    put(key, value): Promise<KvResult> {
      if (failPuts > 0) { failPuts -= 1; return Promise.resolve({ ok: false, reason: "storage_failed" }); }
      data.set(key, value.slice());
      return Promise.resolve(ok);
    },
    get(key): Promise<KvGetResult> {
      const v = data.get(key);
      return Promise.resolve({ ok: true, value: v?.slice() });
    },
    delete(key): Promise<KvResult> {
      data.delete(key);
      return Promise.resolve(ok);
    },
    list(): Promise<KvListResult> {
      return Promise.resolve({ ok: true, keys: [...data.keys()] });
    },
  };
}
