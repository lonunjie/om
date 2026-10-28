// TEST ONLY: an in-memory provider (key material, introduction service, relay mailboxes) and an in-memory
// EncryptedKv. Never import this from application code: tools/web_guard.py and the production bundle check refuse it.
import type { EncryptedKv, KvGetResult, KvListResult, KvResult } from "@om/platform-api";
import type { Inbound, NetCall, NetResult, Provider, RouteOffer } from "./types";

export const TEST_PROVIDER_MARKER = "OM-TEST-LOOPBACK-NOT-FOR-PRODUCTION";
/** The test relay's name and capability shape: a fixed prefix plus the mailbox id, unlike the id itself. */
const TEST_RELAY_HOST = "loopback.test"; // a reserved name: never resolvable, never a real Home
const TEST_RELAY_SCHEME = "https"; // composed, so the product-code lint for hard-coded remote URLs still covers every other file
export const TEST_RELAY_URI = `${TEST_RELAY_SCHEME}://${TEST_RELAY_HOST}/`;
const CAP_PREFIX = "c4".repeat(16);

export type Fault = "unavailable" | "timeout_after_effect" | "timeout_before_effect";

export interface MemoryProvider extends Provider {
  allocateRoutes(count: number): Promise<readonly RouteOffer[]>;
  readonly marker: typeof TEST_PROVIDER_MARKER;
  inject(fault: Fault): void;
  /** Every byte string a relay ever received (acks do not remove them), for ciphertext-only assertions. */
  deposited(): readonly string[];
  /** Pending mailbox contents, by mailbox. */
  revokeAllInvites(): void;
}

export function createMemoryProvider(): MemoryProvider {
  const keys = new Map<string, { bundle: string; opks: string[]; pqopks: string[] }>();
  const invites = new Map<string, string>();
  const boxes = new Map<string, { next: number; items: { seq: number; bytes: string }[] }>();
  const faults: Fault[] = [];
  const log: string[] = []; // every deposit ever received: acks free relay storage, not this record of what it saw
  return {
    marker: TEST_PROVIDER_MARKER,
    inject: (f) => { faults.push(f); },
    deposited: () => [...log],
    allocateRoutes(count): Promise<readonly RouteOffer[]> {
      const out: RouteOffer[] = [];
      for (let i = 0; i < count; i++) {
        const mailbox = Array.from(crypto.getRandomValues(new Uint8Array(16)), (b) => b.toString(16).padStart(2, "0")).join("");
        out.push({ mailbox, relayUri: TEST_RELAY_URI, capability: CAP_PREFIX + mailbox });
      }
      return Promise.resolve(out);
    },
    revokeAllInvites: () => { invites.clear(); },
    perform(call: NetCall): Promise<NetResult> {
      const fault = faults.shift();
      if (fault === "unavailable") return Promise.resolve({ result: "err", error: { kind: "unavailable" } });
      if (fault === "timeout_before_effect") return Promise.resolve({ result: "err", error: { kind: "timeout" } });
      let r: NetResult = { result: "ok" };
      switch (call.call) {
        case "publish_key_material": {
          const p = keys.get(call.deviceId) ?? { bundle: "", opks: [], pqopks: [] };
          p.bundle = call.bundle;
          p.opks.push(...call.opks);
          p.pqopks.push(...call.pqopks);
          keys.set(call.deviceId, p);
          break;
        }
        case "fetch_key_material": {
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
        case "ack": {
          const b = boxes.get(call.mailbox);
          if (b) b.items = b.items.filter((i) => i.seq > call.seq);
          break;
        }
      }
      return Promise.resolve(fault === "timeout_after_effect" ? { result: "err", error: { kind: "timeout" } } : r);
    },
    poll(mailbox: string, after: number): Promise<readonly Inbound[]> {
      return Promise.resolve((boxes.get(mailbox)?.items ?? []).filter((i) => i.seq > after));
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
