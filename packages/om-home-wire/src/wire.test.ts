// SPDX-License-Identifier: MPL-2.0
import { readFileSync } from "node:fs";
import { describe, expect, it } from "vitest";
import {
  CborError, HOME_ERROR_CODES, HomeError, decode, decodeAckRequest, decodeDepositRequest, decodeDrainResponse, decodeHomeError, decodeRelayEntry, decodeTicketParams, encode, encodeAckRequest,
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
