// SPDX-License-Identifier: MPL-2.0
// Wire layer of the RecoveryStore (ADR-0201): the derived identifiers against independently computed values, every body round trip,
// and the rejections a Home relies on (unknown keys, bounds, wrong object types).
import { describe, expect, it } from "vitest";
import {
  CborError, FEATURES, checkRecordEnvelope, checkSignedRequest, checkSignedVeto, decodeFanout, decodeInboxAck, decodeInboxDrain, decodeInboxDrainResponse, decodeInboxRegister, decodeRecordGetResponse, decodeRecordPut, decodeVetoList,
  encode, encodeFanout, encodeInboxAck, encodeInboxDrain, encodeInboxDrainResponse, encodeInboxRegister, encodeRecordGetResponse, encodeRecordPut, encodeVetoList, encodeVetoPost, map, recoveryEtag, recoveryInboxId, recoveryLocator, recoveryRequestDigest, toHex,
} from "./index";

const rand = (n: number): Uint8Array => {
  const out = new Uint8Array(n);
  for (let i = 0; i < n; i += 65536) crypto.getRandomValues(out.subarray(i, Math.min(n, i + 65536)));
  return out;
};
const cap = Uint8Array.from({ length: 32 }, (_, i) => i);
const digest = (b: Uint8Array) => map([[0, 1], [1, b]]);
const sigs = [map([[0, 1], [1, rand(64)]])];
const requestBody = (account = rand(32)) => encode(map([[0, 4], [1, 1], [2, account], [3, 1], [4, rand(16)], [5, digest(rand(32))], [6, digest(rand(32))], [7, rand(16)]]));
const vetoBody = (account = rand(32), d = rand(32)) => encode(map([[0, 5], [1, 1], [2, account], [3, 2], [4, digest(d)]]));
const signed = (body: Uint8Array) => encode(map([[1, body], [2, sigs]]));

describe("derived identifiers (DS-HASH digest values)", () => {
  // Computed outside this code base: SHA-256 over the canonical CBOR array [label, body] (om-domain-separation.json).
  it("locator, inbox id and request digest match independent values", async () => {
    expect(toHex(await recoveryLocator(cap))).toBe("8487f8f84bb4c248a4dbf2e95c2311d2ea89b5c0f3c608f254dff242c73e24c9");
    expect(toHex(await recoveryInboxId(cap))).toBe("98a6fe6bc096ac140b961e1a7fee2f6246d0b610819bb3f303fcc8587c8f508f");
    expect(toHex(await recoveryRequestDigest(cap))).toBe("03968c6bb00085627755c9918016373c77d7dae193d3d96213ffe09c735d27e9");
  });
  it("the three labels never collide on one input, and the ETag is a plain SHA-256 of the envelope", async () => {
    const all = [await recoveryLocator(cap), await recoveryInboxId(cap), await recoveryRequestDigest(cap)].map(toHex);
    expect(new Set(all).size).toBe(3);
    expect(toHex(await recoveryEtag(new Uint8Array(0)))).toBe("e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855");
  });
  it("the feature id is the one the ADR names", () => { expect(FEATURES.recoveryStore).toBe("om.recovery.store.v1"); });
});

describe("record bodies and the envelope shape", () => {
  const env = (ct = 16_400) => encode(map([[0, 35], [1, 1], [2, 1], [3, rand(12)], [4, rand(ct)]]));
  it("put and get round trip; unknown keys and wrong sizes are refused", () => {
    const rid = rand(16);
    const e = env();
    const p = decodeRecordPut(encodeRecordPut({ requestId: rid, envelope: e }));
    expect(toHex(p.requestId)).toBe(toHex(rid));
    expect(toHex(p.envelope)).toBe(toHex(e));
    expect(toHex(decodeRecordGetResponse(encodeRecordGetResponse(e)))).toBe(toHex(e));
    expect(() => decodeRecordPut(encode(map([[0, 1], [1, rid], [2, e], [3, 1]])))).toThrow(CborError);
    expect(() => decodeRecordPut(encode(map([[0, 1], [1, rand(15)], [2, e]])))).toThrow(CborError);
    expect(() => decodeRecordPut(encode(map([[0, 2], [1, rid], [2, e]])))).toThrow(CborError); // another API major
    expect(() => decodeRecordPut(encodeRecordPut({ requestId: rid, envelope: new Uint8Array(1_048_577) }))).toThrow(CborError);
  });
  it("envelope: type 35 version 1, a 12-byte nonce and 16,400 to 1,048,016 bytes of ciphertext; extensions allowed; nothing else", () => {
    expect(() => { checkRecordEnvelope(env()); checkRecordEnvelope(env(1_048_016)); }).not.toThrow();
    expect(() => { checkRecordEnvelope(encode(map([[0, 35], [1, 1], [2, 1], [3, rand(12)], [4, rand(16_400)], [23, map([[1, 1]])]]))); }).not.toThrow();
    for (const bad of [
      env(16_399), env(1_048_017), new Uint8Array([0xff]),
      encode(map([[0, 36], [1, 1], [2, 1], [3, rand(12)], [4, rand(16_400)]])),
      encode(map([[0, 35], [1, 2], [2, 1], [3, rand(12)], [4, rand(16_400)]])),
      encode(map([[0, 35], [1, 1], [2, 0], [3, rand(12)], [4, rand(16_400)]])),
      encode(map([[0, 35], [1, 1], [2, 1], [3, rand(11)], [4, rand(16_400)]])),
      encode(map([[0, 35], [1, 1], [2, 1], [3, rand(12)], [4, rand(16_400)], [5, 1]])),
      encode(map([[0, 35], [1, 1], [2, 1], [3, rand(12)]])),
    ]) expect(() => { checkRecordEnvelope(bad); }).toThrow(CborError);
  });
});

describe("signed objects are checked for shape only", () => {
  it("a request: type 4, version 1, a 32-byte account id; the body bytes and account are returned; any signature bytes pass", () => {
    const account = rand(32);
    const body = requestBody(account);
    const s = checkSignedRequest(signed(body));
    expect(toHex(s.accountId)).toBe(toHex(account));
    expect(toHex(s.body)).toBe(toHex(body));
  });
  it("a veto: type 5, the account id and the request digest it names", () => {
    const account = rand(32);
    const d = rand(32);
    const v = checkSignedVeto(signed(vetoBody(account, d)));
    expect(toHex(v.accountId)).toBe(toHex(account));
    expect(toHex(v.requestDigest)).toBe(toHex(d));
  });
  it("wrong type, a short account id, an unknown key, no signatures, trailing bytes and oversize are all refused", () => {
    expect(() => checkSignedRequest(signed(vetoBody()))).toThrow(CborError);
    expect(() => checkSignedVeto(signed(requestBody()))).toThrow(CborError);
    expect(() => checkSignedRequest(signed(requestBody(rand(31))))).toThrow(CborError);
    expect(() => checkSignedRequest(encode(map([[1, requestBody()], [2, []]])))).toThrow(CborError);
    expect(() => checkSignedRequest(encode(map([[1, requestBody()], [2, sigs], [3, 1]])))).toThrow(CborError);
    expect(() => checkSignedRequest(encode(map([[1, requestBody()], [2, [1]]])))).toThrow(CborError); // a signature is a map
    expect(() => checkSignedRequest(new Uint8Array([...signed(requestBody()), 0]))).toThrow(CborError);
    expect(() => checkSignedRequest(encode(map([[1, encode(map([[0, 4], [1, 1], [2, rand(32)], [3, 1], [4, rand(16)], [5, digest(rand(32))], [6, digest(rand(32))], [7, rand(16)], [9, 1]]))], [2, sigs]])))).toThrow(CborError);
    expect(() => checkSignedRequest(encode(map([[1, new Uint8Array(16_384)], [2, sigs]])))).toThrow(CborError);
  });
  it("fan-out and veto post bodies carry the signed bytes with a 16-byte request id", () => {
    const s = signed(requestBody());
    const rid = rand(16);
    expect(toHex(decodeFanout(encodeFanout({ requestId: rid, signed: s })).signed)).toBe(toHex(s));
    expect(toHex(decodeFanout(encodeVetoPost({ requestId: rid, signed: s })).requestId)).toBe(toHex(rid));
    expect(() => decodeFanout(encodeFanout({ requestId: rid, signed: new Uint8Array(16_385) }))).toThrow(CborError);
  });
});

describe("inbox bodies", () => {
  it("register, drain, ack and the drain response round trip; the item has an id, a kind and bytes and nothing else (no time)", () => {
    const rid = rand(16);
    const c = rand(32);
    expect(toHex(decodeInboxRegister(encodeInboxRegister({ requestId: rid, cap: c })).cap)).toBe(toHex(c));
    expect(decodeInboxDrain(encodeInboxDrain({ requestId: rid, cap: c, pageSize: 16 })).pageSize).toBe(16);
    expect(decodeInboxDrain(encodeInboxDrain({ requestId: rid, cap: c })).pageSize).toBeUndefined();
    expect(() => decodeInboxDrain(encodeInboxDrain({ requestId: rid, cap: c, pageSize: 17 }))).toThrow(CborError);
    expect(() => decodeInboxDrain(encodeInboxDrain({ requestId: rid, cap: c, pageSize: 0 }))).toThrow(CborError);
    const ids = [rand(16), rand(16)];
    expect(decodeInboxAck(encodeInboxAck({ requestId: rid, cap: c, itemIds: ids })).itemIds.map(toHex)).toEqual(ids.map(toHex));
    expect(() => decodeInboxAck(encodeInboxAck({ requestId: rid, cap: c, itemIds: [] }))).toThrow(CborError);
    expect(() => decodeInboxAck(encodeInboxAck({ requestId: rid, cap: c, itemIds: Array.from({ length: 17 }, () => rand(16)) }))).toThrow(CborError);
    const items = [{ id: rand(16), kind: 1 as const, bytes: rand(30) }, { id: rand(16), kind: 2 as const, bytes: rand(40) }];
    const r = decodeInboxDrainResponse(encodeInboxDrainResponse({ items, cursor: rand(8) }));
    expect(r.items.map((i) => [toHex(i.id), i.kind, toHex(i.bytes)])).toEqual(items.map((i) => [toHex(i.id), i.kind, toHex(i.bytes)]));
    expect(r.cursor).toHaveLength(8);
    expect(decodeInboxDrainResponse(encodeInboxDrainResponse({ items: [] })).cursor).toBeUndefined();
    const withTime = encode(map([[0, 1], [1, [map([[0, rand(16)], [1, 1], [2, rand(5)], [3, 1_900_000_000]])]]]));
    expect(() => decodeInboxDrainResponse(withTime)).toThrow(CborError); // an item with a timestamp is not an item
    expect(() => decodeInboxDrainResponse(encode(map([[0, 1], [1, [map([[0, rand(16)], [1, 3], [2, rand(5)]])]]])))).toThrow(CborError);
  });
  it("the veto list holds at most 16 signed vetoes", () => {
    const list = Array.from({ length: 16 }, () => rand(20));
    expect(decodeVetoList(encodeVetoList(list))).toHaveLength(16);
    expect(decodeVetoList(encodeVetoList([]))).toHaveLength(0);
    expect(() => decodeVetoList(encodeVetoList([...list, rand(20)]))).toThrow(CborError);
  });
});
