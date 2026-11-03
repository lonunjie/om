// SPDX-License-Identifier: MPL-2.0
import { describe, expect, it } from "vitest";
import type { NetCall, NetResult } from "@om/core-wasm";
import { createHomesProvider } from "./homes";
import type { HttpProvider } from "./provider";

interface Seen { readonly calls: NetCall[]; readonly polls: string[] }

/** Just enough of an HttpProvider to see what the wrapper sends where. */
function fake(baseUrl: string): { p: HttpProvider; seen: Seen } {
  const seen: Seen = { calls: [], polls: [] };
  let n = 0;
  const p = {
    baseUrl,
    admissionRefused: () => false,
    reachability: () => "unknown",
    lostMailboxes: () => [],
    routeEntry: () => Promise.reject(new Error("unused")),
    capabilities: () => Promise.resolve(undefined),
    setWakeActive: () => undefined,
    onWake: () => () => undefined,
    wakeStats: () => ({ sockets: 0, live: 0, wakes: 0, connects: 0 }),
    dispose: () => undefined,
    allocateRoutes: (count: number) => Promise.resolve(Array.from({ length: count }, () => ({ mailbox: `m${String(n++)}`, relayUri: `${baseUrl}/`, capability: "cap" }))),
    perform: (call: NetCall): Promise<NetResult> => { seen.calls.push(call); return Promise.resolve({ result: "ok" }); },
    poll: (mailbox: string) => { seen.polls.push(mailbox); return Promise.resolve([]); },
  } as unknown as HttpProvider;
  return { p, seen };
}

describe("several Homes of one person", () => {
  it("is exactly the Home when there is only one and nothing is draining", () => {
    const one = fake("https://a.example").p;
    expect(createHomesProvider([one])).toBe(one);
    expect(() => createHomesProvider([])).toThrow();
  });

  it("deposits, acknowledges and polls at the Home a route names; everything else goes to the active Home", async () => {
    const a = fake("https://a.example"); const b = fake("https://b.example"); const gone = fake("https://old.example");
    const homes = createHomesProvider([a.p, b.p], [gone.p]);
    await homes.perform({ call: "deposit", mailbox: "cap-b", relayUri: "https://b.example/", bytes: "00" });
    await homes.perform({ call: "deposit", mailbox: "cap-old", relayUri: "https://old.example/", bytes: "00" });
    await homes.perform({ call: "deposit", mailbox: "cap-peer", relayUri: "https://peer.example/", bytes: "00" }); // a peer's Home: the active Home's client reaches it
    await homes.perform({ call: "ack", mailbox: "m", seq: 1, relayUri: "https://b.example/" });
    await homes.perform({ call: "fetch_key_material", deviceId: "d" });
    await homes.poll("mb", 0, "relay", "https://b.example/");
    await homes.poll("mo", 0, "relay", "https://old.example/");
    await homes.poll("mi", 0, "intro");
    expect(b.seen.calls.map((c) => c.call)).toEqual(["deposit", "ack"]);
    expect(gone.seen.calls.map((c) => c.call)).toEqual(["deposit"]);
    expect(a.seen.calls.map((c) => c.call)).toEqual(["deposit", "fetch_key_material"]);
    expect(b.seen.polls).toEqual(["mb"]);
    expect(gone.seen.polls).toEqual(["mo"]);
    expect(a.seen.polls).toEqual(["mi"]);
  });

  it("offers routes at the Home Core holds the fewest for, the active one preferred; a draining Home is never offered", async () => {
    const a = fake("https://a.example"); const b = fake("https://b.example"); const gone = fake("https://old.example");
    const homes = createHomesProvider([a.p, b.p], [gone.p]);
    const uris = (rs: readonly { relayUri: string; priority?: number }[]) => rs.map((r) => `${r.relayUri}${String(r.priority)}`);
    expect(uris(await homes.allocateRoutes(4))).toEqual(["https://a.example/0", "https://b.example/1", "https://a.example/0", "https://b.example/1"]);
    // Core used up the active Home's offers: the next ones refill it, not the fallback.
    expect(uris(await homes.allocateRoutes(2, ["https://b.example/", "https://b.example/"]))).toEqual(["https://a.example/0", "https://a.example/0"]);
    expect(uris(await homes.allocateRoutes(1, ["https://a.example/", "https://a.example/"]))).toEqual(["https://b.example/1"]);
  });
});
