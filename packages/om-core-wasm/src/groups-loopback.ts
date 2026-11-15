// TEST AND DEV ONLY: faults for the group carriage tests. The carriage itself is the provider's sealed delivery (deposit, poll,
// acknowledge), so nothing here carries a group wire: `createGroupFaults` wraps a Provider, per profile, and adds what a real
// network does (down, duplicate delivery) plus a log of every deposit for ciphertext-only assertions. Never import it from
// product code (the production guards cover `test-support`, which re-exports it).
import type { Inbound, MailboxKind, NetCall, NetResult, PerformExtra, Provider } from "./types";

export interface GroupFaults {
  /** The provider one profile's runtime uses: the shared one, with this profile's faults applied. */
  provider(profile: string): Provider;
  /** While down, every call of this profile (deposit, poll, acknowledge) is `unavailable`. */
  setDown(profile: string, down: boolean): void;
  /** The next deposit of this profile is performed twice (a duplicate delivery). */
  duplicateNext(profile: string): void;
  /** Make the next `n` deposits of this profile time out after taking effect (the sender cannot tell). */
  timeoutNext(profile: string, n?: number): void;
  /** Every deposit that reached the relay, in order. */
  readonly log: readonly { readonly from: string; readonly mailbox: string; readonly bytes: string }[];
}

export function createGroupFaults(inner: Provider): GroupFaults {
  const down = new Set<string>();
  const dup = new Set<string>();
  const timeouts = new Map<string, number>();
  const log: { from: string; mailbox: string; bytes: string }[] = [];
  const unavailable: NetResult = { result: "err", error: { kind: "unavailable" } };
  return {
    log,
    setDown: (p, d) => { if (d) down.add(p); else down.delete(p); },
    duplicateNext: (p) => { dup.add(p); },
    timeoutNext: (p, n = 1) => { timeouts.set(p, n); },
    provider(profile): Provider {
      return {
        ...(inner.blobLimits === undefined ? {} : { blobLimits: () => inner.blobLimits?.() ?? Promise.resolve(null) }),
        ...(inner.allocateRoutes === undefined ? {} : { allocateRoutes: (count: number, held?: readonly string[]) => inner.allocateRoutes?.(count, held) ?? Promise.resolve([]) }),
        async perform(call: NetCall, extra?: PerformExtra): Promise<NetResult> {
          if (down.has(profile)) return unavailable;
          if (call.call !== "deposit") return inner.perform(call, extra);
          const r = await inner.perform(call, extra);
          log.push({ from: profile, mailbox: call.mailbox, bytes: call.bytes });
          if (dup.delete(profile)) await inner.perform(call, extra);
          const left = timeouts.get(profile) ?? 0;
          if (left > 0) { timeouts.set(profile, left - 1); return { result: "err", error: { kind: "timeout" } }; }
          return r;
        },
        poll(mailbox: string, after: number, kind?: MailboxKind, relayUri?: string, capability?: string): Promise<readonly Inbound[]> {
          if (down.has(profile)) return Promise.reject(new Error("down"));
          return inner.poll(mailbox, after, kind, relayUri, capability);
        },
      };
    },
  };
}
