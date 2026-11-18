// Independent check of the ADR-0203 sheet and transcript vectors using Node crypto.
import assert from "node:assert/strict";
import { createHash, createHmac, hkdfSync } from "node:crypto";
import { readFileSync } from "node:fs";

const fixture = JSON.parse(readFileSync(process.argv[2], "utf8"));
const { sheet, ceremony, vectors } = fixture;
const bytes = (hex) => Buffer.from(hex, "hex");
const sha = (data) => createHash("sha256").update(data).digest();
const hmac = (key, data) => createHmac("sha256", key).update(data).digest();

function header(major, size) {
  const initial = major << 5;
  if (size < 24) return Buffer.from([initial | size]);
  if (size <= 255) return Buffer.from([initial | 24, size]);
  if (size <= 65535) return Buffer.from([initial | 25, size >> 8, size & 255]);
  const output = Buffer.alloc(5);
  output[0] = initial | 26;
  output.writeUInt32BE(size, 1);
  return output;
}

function cbor(value) {
  if (typeof value === "number") return header(0, value);
  if (typeof value === "string") {
    const encoded = Buffer.from(value);
    return Buffer.concat([header(3, encoded.length), encoded]);
  }
  if (Buffer.isBuffer(value)) return Buffer.concat([header(2, value.length), value]);
  assert(Array.isArray(value));
  return Buffer.concat([header(4, value.length), ...value.map(cbor)]);
}

const ds = (label, body) => sha(cbor([label, body]));
const hex = (value, expected) => assert.equal(value.toString("hex"), expected);

assert(sheet.qr.startsWith("om:recover/"));
const binary = Buffer.from(sheet.qr.slice("om:recover/".length), "base64url");
hex(binary, sheet.binary);
assert.deepEqual([...binary.subarray(0, 2)], [2, 1]);
const checkpoint = vectors.find(({ id }) => id === "OwnerRecoveryCheckpoint/generation-one");
hex(binary.subarray(69 + binary[68], -4), checkpoint.hex);
assert.deepEqual(binary.subarray(-4), ds("OM-RECOVERY-SHEET-CHECK-v1", binary.subarray(0, -4)).subarray(0, 4));
const bits = [...binary].map((byte) => byte.toString(2).padStart(8, "0")).join("");
const alphabet = "ABCDEFGHIJKLMNOPQRSTUVWXYZ234567";
assert.equal(bits.match(/.{1,5}/g).map((digit) => alphabet[Number.parseInt(digit.padEnd(5, "0"), 2)]).join(""), sheet.compact);

const prefix = "om:recover-approve/";
assert(ceremony.offer.startsWith(prefix));
const offer = Buffer.from(ceremony.offer.slice(prefix.length), "base64url");
assert.equal(offer[0], 1);
const expiry = offer.readUInt32BE(1);
const secret = offer.subarray(5, 21);
const bLengthOffset = 22 + offer[21];
const requestCap = offer.subarray(22, bLengthOffset);
const homeOffset = bLengthOffset + 1 + offer[bLengthOffset];
const responseCap = offer.subarray(bLengthOffset + 1, homeOffset);
const home = offer.subarray(homeOffset);

const label = (use) => "OM-RECOVERY-APPROVAL-" + use + "-v1";
const context = cbor(["recovery_approval", 1, expiry, home, requestCap, responseCap]);
const id = ds(label("KDF"), context);
hex(context, ceremony.context);
hex(id, ceremony.transcript);
const keys = Object.fromEntries(["req", "resp", "code", "bind"].map((purpose) => [
  purpose, Buffer.from(hkdfSync("sha256", secret, id, cbor([label("KDF"), purpose]), 32)),
]));
for (const [purpose, key] of Object.entries(keys)) hex(key, ceremony.keys[purpose]);
for (const direction of [1, 2]) hex(cbor([label("AEAD-AD"), id, direction]), ceremony.ad[direction - 1]);
hex(ds(label("MAILBOX-HASH"), responseCap).subarray(0, 16), ceremony.mailbox);

const proposal = bytes(vectors.find(({ id }) => id === "RecoveryApprovalProposal/v1").hex);
const mac = (key, role, body) => hmac(key, cbor([label("MAC"), role, body]));
const code = mac(keys.code, 1, proposal).readBigUInt64BE() % 1000000n;
assert.equal(code.toString().padStart(6, "0"), ceremony.code);
const binding = (descriptor) => mac(keys.bind, 2, cbor([id, sha(proposal), descriptor]));
hex(binding(bytes(ceremony.descriptor)), ceremony.binding);

const reply = vectors.find(({ id }) => id === "RecoveryApprovalReply/manifest").value;
const descriptor = new Map(new Map(reply.$map).get(2).$map);
const chunks = vectors.filter(({ id }) => id.startsWith("RecoveryApprovalChunk/chunk-")).map(({ value }) => new Map(value.$map));
assert.equal(chunks.length, descriptor.get(7));
const assembled = Buffer.concat(chunks.map((chunk, index) => {
  assert.equal(chunk.get(3), index);
  assert.deepEqual(chunk.get(2), descriptor.get(5));
  const data = bytes(chunk.get(4).$bytes);
  if (index + 1 < chunks.length) assert.equal(data.length, 50_000);
  return data;
}));
assert.equal(assembled.length, descriptor.get(6));
hex(assembled, ceremony.evidence);
hex(ds(label("EVIDENCE-HASH"), assembled), descriptor.get(5).$bytes);

const refusal = (name) => ceremony.refusals.find((item) => item.case === name);
const substitute = refusal("substituted-target");
assert.notEqual(binding(bytes(substitute.descriptor)).toString("hex"), substitute.tag);
assert(refusal("missing-chunk").indexes.length < descriptor.get(7));
assert.equal(refusal("expired").now, expiry);
assert(!refusal("crossed-purpose").offer.startsWith(prefix));
assert(!refusal("old-client").offer.startsWith("om:link/"));
assert(refusal("stale-parent").authenticated_head_generation > refusal("stale-parent").proposal_parent_generation);
console.log("ADR-0203 sheet, transcript, binding and manifest fixtures match (Node)");
