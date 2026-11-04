// SPDX-License-Identifier: MPL-2.0
// Several Homes of one person (HOME_ONBOARDING_TIERS 2: a person may hold several tiers at once, an active Home and fallbacks;
// MULTI_RELAY_AND_DELIVERY_RESILIENCE 1: one RelaySet entry per Home, each with its own capability).
//
// Each Home is an ordinary `HttpProvider` configured for that Home, so it creates, drains and acknowledges only its own
// queues and is the only one that ever sees its own admission secret. This wrapper decides which of them a Core call is for:
//   * a mailbox is polled and acknowledged at the Home its route names (`relayUri`, which Core carries per mailbox);
//   * a deposit goes to the Home its destination route names, which is the recipient's, never ours unless it is;
//   * key material, invites and introductions live at the first (active) Home;
//   * new routes are offered across the Homes, the active one preferred (`priority` = position), and Core adds a route at each
//     Home to every relationship (relay-set lineage in Core), so no Home is a single point of failure;
//   * a draining Home (one the person is leaving, after `retireHome`) is still polled, acknowledged and deposited to until Core
//     closes its routes, but is never offered again, so Core does not add a route there back.
// With one Home and nothing draining it is exactly that Home.
import type { Inbound, MailboxKind, NetCall, NetResult, PerformExtra, RouteOffer } from "@om/core-wasm";
import type { HttpProvider, Reachability } from "./provider";

export function createHomesProvider(offered: readonly HttpProvider[], draining: readonly HttpProvider[] = []): HttpProvider {
  const first = offered[0];
  if (first === undefined) throw new Error("at least one Home");
  if (offered.length === 1 && draining.length === 0) return first;
  const homes = [...offered, ...draining];

  /** The Home a route name belongs to; anything else (a peer's Home, no name) is handled by the active Home. */
  const homeFor = (relayUri: string | undefined): HttpProvider => (relayUri === undefined ? first : homes.find((h) => `${h.baseUrl}/` === relayUri) ?? first);

  return {
    baseUrl: first.baseUrl,
    admissionRefused: () => homes.some((h) => h.admissionRefused()),
    reachability(): Reachability {
      const all = homes.map((h) => h.reachability());
      return all.includes("reachable") ? "reachable" : all.includes("unreachable") ? "unreachable" : "unknown";
    },
    lostMailboxes: () => homes.flatMap((h) => [...h.lostMailboxes()]),
    routeEntry: (mailbox, expiresAt) => first.routeEntry(mailbox, expiresAt),
    capabilities: () => first.capabilities(),
    setWakeActive: (active) => { for (const h of homes) h.setWakeActive(active); },
    onWake(listener) {
      const offs = homes.map((h) => h.onWake(listener));
      return () => { for (const off of offs) off(); };
    },
    wakeStats: () => homes.map((h) => h.wakeStats()).reduce((a, b) => ({ sockets: a.sockets + b.sockets, live: a.live + b.live, wakes: a.wakes + b.wakes, connects: a.connects + b.connects })),
    dispose: () => { for (const h of homes) h.dispose(); },
    async allocateRoutes(count, held = []): Promise<readonly RouteOffer[]> {
      // Fill the Home Core holds the fewest offers for (the active one on a tie), so a fallback Home never runs dry.
      const have = offered.map((h) => held.filter((u) => u === `${h.baseUrl}/`).length);
      const out: RouteOffer[] = [];
      for (let i = 0; i < count; i++) {
        const at = have.indexOf(Math.min(...have));
        have[at] = (have[at] ?? 0) + 1;
        for (const o of await (offered[at] as HttpProvider).allocateRoutes(1)) out.push({ ...o, priority: at });
      }
      return out;
    },
    // Files live at the active Home; an object of ours is deleted at the Home whose address its location carries.
    blobLimits: () => (first.blobLimits ? first.blobLimits() : Promise.resolve(null)),
    perform(call: NetCall, extra?: PerformExtra): Promise<NetResult> {
      switch (call.call) {
        case "delete_blob": return (homes.find((h) => call.location.includes(new URL(h.baseUrl).host)) ?? first).perform(call);
        case "put_blob": return first.perform(call, extra);
        case "deposit": return (call.relayUri === undefined ? first : homeFor(call.relayUri)).perform(call);
        case "ack": return (call.kind === "intro" ? first : homeFor(call.relayUri)).perform(call);
        case "open_queue": return homeFor(call.relayUri).perform(call);
        default: return first.perform(call);
      }
    },
    poll(mailbox: string, after: number, kind?: MailboxKind, relayUri?: string, capability?: string): Promise<readonly Inbound[]> {
      return (kind === "intro" ? first : homeFor(relayUri)).poll(mailbox, after, kind, relayUri, capability);
    },
  };
}
