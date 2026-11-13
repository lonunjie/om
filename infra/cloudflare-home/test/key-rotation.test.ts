// SPDX-License-Identifier: AGPL-3.0-or-later
// Signed-prekey rotation at the Home (deviation D28): a newly published higher generation drops the stored one-time items of
// lower generations, because one item that fails verification under the new generation fails a client's whole fetch.
import { afterAll, beforeAll, describe, expect, it } from "vitest";
import { encode, map, toHex } from "@om/home-wire";
import { createHttpProvider, createMemorySecretStore, parseProviderUrl, type HttpProvider } from "@om/provider-http";
import { startHome, type Home } from "./harness";

const rand = (n: number): Uint8Array => crypto.getRandomValues(new Uint8Array(n));

/** Just enough of an AsyncKeyBundle for the Home's availability aids: type 9, device id, generation, expiry (nothing is verified). */
const bundle = (device: Uint8Array, generation: number): string =>
  toHex(encode(map([[1, encode(map([[0, 9], [2, device], [6, generation], [13, Math.floor(Date.now() / 1000) + 86_400]]))]])));

describe("key pool across a signed-prekey rotation", () => {
  let home: Home;
  beforeAll(async () => { home = await startHome(); }, 120_000);
  afterAll(async () => { await home.dispose(); });
  const mk = (): HttpProvider => {
    const config = parseProviderUrl(home.url, { allowLoopbackHttp: true });
    if (!config) throw new Error("bad url");
    return createHttpProvider({ config, secrets: createMemorySecretStore(), timeoutMs: 10_000 });
  };
  const fetchItems = async (peer: HttpProvider, dev: string, cap: string): Promise<string[]> => {
    const r = await peer.perform({ call: "fetch_key_material", deviceId: dev, service: `${home.url}/`, capability: cap });
    if (r.result !== "many") throw new Error(`fetch failed: ${JSON.stringify(r)}`);
    return [...r.items];
  };

  it("publishing a higher generation drops one-time items of the lower one and keeps the new batch", async () => {
    const owner = mk();
    const peer = mk();
    const deviceBytes = rand(16);
    const dev = toHex(deviceBytes);
    const cap = toHex(rand(32));
    const [g1, g2] = [bundle(deviceBytes, 1), bundle(deviceBytes, 2)];
    const item = (n: number): string => toHex(new Uint8Array(12).fill(n));
    const [a1, a2, b1, b2] = [item(1), item(2), item(3), item(4)];
    expect(await owner.perform({ call: "publish_key_material", deviceId: dev, bundle: g1, opks: [a1, a2], pqopks: [a1] })).toEqual({ result: "ok" });
    expect(await owner.perform({ call: "grant_key_fetch", deviceId: dev, capability: cap })).toEqual({ result: "ok" });
    expect(await owner.perform({ call: "publish_key_material", deviceId: dev, bundle: g2, opks: [b1], pqopks: [b1] })).toEqual({ result: "ok" });
    // A replenishment of the same generation appends, it does not drop.
    expect(await owner.perform({ call: "publish_key_material", deviceId: dev, bundle: g2, opks: [b2], pqopks: [] })).toEqual({ result: "ok" });
    // The first fetch gets the new bundle and the first remaining items, which are generation 2's: no generation 1 item is left to break it.
    expect(await fetchItems(peer, dev, cap)).toEqual([g2, b1, b1]);
    // Revoking the old generation is a no-op once a newer bundle is published; revoking the current one removes the bundle.
    expect(await owner.perform({ call: "revoke_key_generation", deviceId: dev, generation: 1 })).toEqual({ result: "ok" });
    expect((await fetchItems(peer, dev, cap))[0]).toBe(g2);
    expect(await owner.perform({ call: "revoke_key_generation", deviceId: dev, generation: 2 })).toEqual({ result: "ok" });
    expect(await peer.perform({ call: "fetch_key_material", deviceId: dev, service: `${home.url}/`, capability: cap })).toEqual({ result: "err", error: { kind: "not_found" } });
    // A stranger cannot revoke.
    expect(await peer.perform({ call: "revoke_key_generation", deviceId: dev, generation: 2 })).toEqual({ result: "err", error: { kind: "rejected", reason: "unauthorized" } });
  });
});
