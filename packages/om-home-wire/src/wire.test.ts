// SPDX-License-Identifier: MPL-2.0
import { readFileSync } from "node:fs";
import { describe, expect, it } from "vitest";
import {
  BLOB_CIPHERTEXT_LABEL, CborError, HOME_ERROR_CODES, blobCiphertextDigest, blobLocator, blobLocatorPattern, blobMatches, decodeBlobPutRequest, decodeBlobPutResponse, encodeBlobDigestHeader, encodeBlobPutRequest, encodeBlobPutResponse, newBlobCap, parseBlobDigestHeader, HomeError, decode, decodeAckRequest, decodeDepositRequest, decodeDrainResponse, decodeHomeError, decodeRelayEntry, decodeTicketParams, encode, encodeAckRequest,
  decodeWakeAuth, decodeWakeEvent, encodeWakeAuth, encodeWakeEvent, parseWakeProtocols, wakeLabel,
  encodeDepositRequest, encodeDrainResponse, encodeHomeError, encodeRelayEntry, encodeTicketParams, fromHex, isRetryable, map, parseDeposit, routeBinding, toHex, type HomeErrorName,
} from "./index";

const rid = new Uint8Array(16).fill(7);

describe("deterministic CBOR", () => {
  it("encodes shortest-form, sorted, definite-length items", () => {
    expect(toHex(encode(map([[2, "b"], [0, 1], [1, new Uint8Array([9])]])))).toBe("a3000101410902" + "6162");
    expect(toHex(encode(24))).toBe("1818");
    expect(toHex(encode(256))).toBe("190100");
    expect(toHex(encode(2n ** 63n))).toBe("1b8000000000000000");
    expect(toHex(encode([true, false, null]))).toBe("83f5f4f6");
  });

  it("sorts mixed keys by encoded bytes (integers before text)", () => {
    expect(toHex(encode(map([["a", 1], [10, 2], [1, 3]])))).toBe("a30103" + "0a02" + "616101");
  });

  it("rejects everything outside the profile", () => {
    const bad: Record<string, string> = {
      "non-shortest integer": "1800",
      "indefinite array": "9fff",
      "indefinite bytes": "5fff",
      "negative integer": "20",
      "tag": "c101",
      "float": "f93c00",
      "undefined": "f7",
      "unsorted map keys": "a202000100",
      "duplicate map key": "a200000001",
      "trailing bytes": "0000",
      "truncated": "4201",
      "bad utf-8": "62c328",
      "map key not int or text": "a1410000",
    };
    for (const [name, h] of Object.entries(bad)) expect(() => decode(fromHex(h)), name).toThrow(CborError);
  });

  it("bounds nesting and rejects an absurd length before allocating", () => {
    expect(() => decode(fromHex("9".repeat(1) + "b" + "ffffffff"))).toThrow(CborError);
    expect(() => decode(new Uint8Array(40).fill(0x81))).toThrow(CborError);
  });

  it("round-trips a nested value", () => {
    const v = map([[0, 1], [1, [map([[0, new Uint8Array([1, 2])]]), "x"]], [5, null]]);
    expect(decode(encode(v))).toEqual(v);
  });
});

describe("error registry", () => {
  it("matches spec/registries/om-home-errors.json exactly (codes, names, retryability)", () => {
    const reg = JSON.parse(readFileSync(new URL("../../../spec/registries/om-home-errors.json", import.meta.url), "utf8")) as { errors: { code: number; name: string; retryable: boolean }[] };
    expect(Object.entries(HOME_ERROR_CODES).map(([name, code]) => ({ name, code })).sort((a, b) => a.code - b.code))
      .toEqual(reg.errors.map((e) => ({ name: e.name, code: e.code })));
    for (const e of reg.errors) expect(isRetryable(e.name as HomeErrorName), e.name).toBe(e.retryable);
  });

  it("retry_after_seconds is emitted only for retryable errors; unknown codes decode as non-retryable", () => {
    expect(decodeHomeError(encodeHomeError(new HomeError("rate_limited", { retryAfterSeconds: 5 }))).retryAfterSeconds).toBe(5);
    expect(decodeHomeError(encodeHomeError(new HomeError("quota", { retryAfterSeconds: 5 }))).retryAfterSeconds).toBeUndefined();
    const unknown = decodeHomeError(encode(map([[0, 1], [1, 99]])));
    expect(unknown).toMatchObject({ name: "unknown", retryable: false });
    expect(() => decodeHomeError(encode(map([[0, 2], [1, 1]])))).toThrow(CborError);
  });
});

describe("bodies follow spec/cddl/home-api.cddl", () => {
  it("round-trips and bounds-checks", () => {
    const cap = new Uint8Array(48).fill(1);
    const d = decodeDepositRequest(encodeDepositRequest({ requestId: rid, cap, envelope: new Uint8Array(10) }));
    expect(d.cap).toEqual(cap);
    expect(() => decodeDepositRequest(encodeDepositRequest({ requestId: new Uint8Array(15), cap, envelope: new Uint8Array(10) }))).toThrow(CborError);
    expect(() => decodeDepositRequest(encodeDepositRequest({ requestId: rid, cap: new Uint8Array(15), envelope: new Uint8Array(10) }))).toThrow(CborError);
    expect(() => decodeDepositRequest(encode(map([[0, 1], [1, rid], [2, cap], [3, new Uint8Array(1)], [9, 1]])))).toThrow(CborError); // unknown key
    expect(() => decodeDepositRequest(encode(map([[0, 2], [1, rid], [2, cap], [3, new Uint8Array(1)]])))).toThrow(CborError); // api major
    expect(decodeAckRequest(encodeAckRequest({ requestId: rid, cap, itemIds: [rid] })).itemIds).toHaveLength(1);
    expect(() => decodeAckRequest(encodeAckRequest({ requestId: rid, cap, itemIds: [] }))).toThrow(CborError);
    expect(() => decodeAckRequest(encodeAckRequest({ requestId: rid, cap, itemIds: Array.from({ length: 257 }, () => rid) }))).toThrow(CborError);
    const r = decodeDrainResponse(encodeDrainResponse({ items: [{ id: rid, bytes: new Uint8Array([1]), seq: 4 }], next: new Uint8Array(8) }));
    expect(r.items[0]?.seq).toBe(4);
    expect(decodeTicketParams(encodeTicketParams({ ttlSeconds: 60, maxUses: 1, payloadLimitBytes: 100 })).maxUses).toBe(1);
  });
});

describe("sealed-delivery outer object", () => {
  const sealed = (over: Record<number, unknown> = {}): Uint8Array =>
    encode(map(Object.entries({ 0: 23, 1: 1, 2: 1, 3: new Uint8Array(16), 4: new Uint8Array(16), 5: new Uint8Array(1120), 6: new Uint8Array(1040), 7: 99, ...over }).map(([k, v]) => [Number(k), v as never])));

  it("accepts exactly the profile-1 shapes and exposes no sender field", () => {
    const v = parseDeposit(sealed());
    expect(v.kind).toBe("sealed");
    expect(Object.keys(v).sort()).toEqual(["cryptoProfile", "deliveryId", "deliveryKeyId", "expiresAt", "kind"]);
    for (const len of [1040, 4112, 16400, 65552, 131088]) expect(parseDeposit(sealed({ 6: new Uint8Array(len) })).kind).toBe("sealed");
  });

  it("rejects profile-1 length drift, extra fields (a sender id cannot ride along) and foreign objects", () => {
    expect(() => parseDeposit(sealed({ 6: new Uint8Array(1041) }))).toThrow(CborError);
    expect(() => parseDeposit(sealed({ 5: new Uint8Array(1119) }))).toThrow(CborError);
    expect(() => parseDeposit(sealed({ 8: new Uint8Array(32) }))).toThrow(CborError); // e.g. an AccountID
    expect(() => parseDeposit(sealed({ 0: 10 }))).toThrow(CborError);
    expect(() => parseDeposit(new Uint8Array([1, 2, 3]))).toThrow(CborError);
  });
});

describe("route object and binding (SEALED_SENDER_DELIVERY_PROFILE.md section 3)", () => {
  it("route_binding = SHA-256(OM-CBOR [label, relay_uri, deposit_capability]); vectors computed independently in Python", async () => {
    expect(toHex(await routeBinding("om-core:mailbox/v0", Uint8Array.from({ length: 16 }, (_, i) => i)))).toBe("7f90172215325428856fde1a70d4f0839a08198877ed5150550654936389a2eb");
    expect(toHex(await routeBinding("https://relay.example/", new Uint8Array(32).fill(7)))).toBe("45808f4f0891ced23198e03a787bf2a7dc46d11b4eb49ab429d5a3c1149dde27");
  });

  it("RelayEntry round-trips with the frozen field numbers and bounds", () => {
    const e = { relayUri: "https://relay.example/", capability: new Uint8Array(32).fill(3), priority: 0, expiresAt: 1_800_000_000, transports: ["https"] };
    expect(decodeRelayEntry(encodeRelayEntry(e))).toEqual(e);
    expect(() => decodeRelayEntry(encode(map([[0, "u"], [1, new Uint8Array(15)], [2, 0], [4, 1]])))).toThrow(CborError);
    expect(() => decodeRelayEntry(encode(map([[0, "u"], [1, new Uint8Array(16)], [2, 256], [4, 1]])))).toThrow(CborError);
  });
});

describe("wake channel (om.relay.wake.v1)", () => {
  it("auth and event frames round-trip, are strict, and are tiny", () => {
    const cap = new Uint8Array(48).fill(3);
    expect(decodeWakeAuth(encodeWakeAuth({ cap })).cap).toEqual(cap);
    expect(() => decodeWakeAuth(encodeWakeAuth({ cap: new Uint8Array(15) }))).toThrow();
    for (const kind of ["ready", "wake"] as const) {
      const b = encodeWakeEvent({ kind, seq: 70_000 });
      expect(decodeWakeEvent(b)).toEqual({ kind, seq: 70_000 });
      expect(b.length).toBeLessThan(16);
    }
    expect(() => decodeWakeEvent(encode(map([[0, 1], [1, 9], [2, 1]])))).toThrow(); // unknown kind
    expect(() => decodeWakeEvent(encode(map([[0, 1], [1, 2], [2, 1], [3, 1]])))).toThrow(); // unknown key
    expect(() => decodeWakeEvent(encode(map([[0, 2], [1, 2], [2, 1]])))).toThrow(); // other api major
  });
  it("the routing label names a queue but is not a capability: exactly one label and the feature id are required", () => {
    const q = new Uint8Array(16).map((_, i) => i);
    expect(wakeLabel(q)).toBe("om.q.000102030405060708090a0b0c0d0e0f");
    expect(parseWakeProtocols(`om.relay.wake.v1, ${wakeLabel(q)}`)?.queueId).toEqual(q);
    for (const bad of [null, "", "om.relay.wake.v1", wakeLabel(q), `om.relay.wake.v1, ${wakeLabel(q)}, ${wakeLabel(q)}`, "om.relay.wake.v1, om.q.xyz", `om.relay.wake.v1, om.q.${"0".repeat(64)}`]) {
      expect(parseWakeProtocols(bad), String(bad)).toBeUndefined();
    }
  });
});

describe("blob spool bodies (feature om.blob.spool.v1)", () => {
  it("the digest is SHA-256 over the canonical CBOR array [label, body] (DS-HASH), for every length class", async () => {
    const { createHash } = await import("node:crypto");
    for (const n of [1, 23, 24, 255, 256, 65535, 65536, 70000]) {
      const body = new Uint8Array(n).map((_, i) => (i * 7 + n) & 255);
      const viaCbor = new Uint8Array(createHash("sha256").update(encode([BLOB_CIPHERTEXT_LABEL, body])).digest());
      expect(toHex(await blobCiphertextDigest(body)), `length ${String(n)}`).toBe(toHex(viaCbor));
    }
    expect(await blobMatches(new Uint8Array([1, 2, 3]), await blobCiphertextDigest(new Uint8Array([1, 2, 3])))).toBe(true);
    expect(await blobMatches(new Uint8Array([1, 2, 3]), await blobCiphertextDigest(new Uint8Array([1, 2, 4])))).toBe(false);
  });

  it("the registered label is the one in the domain-separation registry", () => {
    const reg = JSON.parse(readFileSync(new URL("../../../spec/registries/om-domain-separation.json", import.meta.url), "utf8")) as { labels?: { label: string }[]; entries?: { label: string }[] };
    const all = JSON.stringify(reg);
    expect(all).toContain(`"${BLOB_CIPHERTEXT_LABEL}"`);
  });

  it("round-trips the put request and response and rejects what the CDDL rejects", () => {
    const cap = newBlobCap();
    const req = decodeBlobPutRequest(encodeBlobPutRequest({ requestId: rid, cap, ciphertext: new Uint8Array(5).fill(9), retentionSeconds: 3600 }));
    expect(req).toMatchObject({ retentionSeconds: 3600 });
    expect(req.cap).toEqual(cap);
    expect(decodeBlobPutRequest(encodeBlobPutRequest({ requestId: rid, cap, ciphertext: new Uint8Array(1) })).retentionSeconds).toBeUndefined();
    const bad = (o: Partial<{ requestId: Uint8Array; cap: Uint8Array; ciphertext: Uint8Array }>) => () => decodeBlobPutRequest(encodeBlobPutRequest({ requestId: rid, cap, ciphertext: new Uint8Array(1), ...o }));
    expect(bad({ requestId: new Uint8Array(15) })).toThrow(CborError);
    expect(bad({ cap: new Uint8Array(31) })).toThrow(CborError); // a blob_cap needs 16 routing + 16 secret bytes at least
    expect(bad({ ciphertext: new Uint8Array(0) })).toThrow(CborError);
    expect(() => decodeBlobPutRequest(encode(map([[0, 1], [1, rid], [2, cap], [3, new Uint8Array(1)], [9, 1]])))).toThrow(CborError);
    const res = decodeBlobPutResponse(encodeBlobPutResponse({ readCap: cap, digest: new Uint8Array(32).fill(3), expiresAt: 99 }));
    expect(res).toMatchObject({ expiresAt: 99 });
    expect(() => decodeBlobPutResponse(encodeBlobPutResponse({ readCap: cap, digest: new Uint8Array(31) }))).toThrow(CborError);
  });

  it("the locator is the routing prefix, and the digest header accepts exactly 32 bytes of base64url", () => {
    const cap = newBlobCap();
    expect(blobLocator(cap)).toBe(toHex(cap.slice(0, 16)));
    expect(blobLocatorPattern.test(blobLocator(cap))).toBe(true);
    const d = new Uint8Array(32).fill(5);
    expect(parseBlobDigestHeader(encodeBlobDigestHeader(d))).toEqual(d);
    for (const v of [null, "", "AAAA", "not base64 !", encodeBlobDigestHeader(new Uint8Array(33))]) expect(parseBlobDigestHeader(v)).toBeUndefined();
  });
});
