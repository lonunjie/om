// node --test deploy/ledger   (no device, no Ledger packages needed)
import assert from "node:assert/strict";
import { generateKeyPairSync, sign } from "node:crypto";
import { test } from "node:test";
import { base58, checkPath, parseEnvelope, signEnvelope, verifyEd25519 } from "./lib.mjs";

const GOLDEN = Buffer.from(
  "ff736f6c616e61206f6666636861696e006f6d2e72656c656173652d64656c65676174696f6e2f31000000000000000000000111111111111111111111111111111111111111111111111111111111111111118a004f4d2072656c656173652064656c65676174696f6e207c2076657273696f6e2033207c206578706972657320323032372d30312d32365430303a30303a30305a207c207368613235362033643361363334663635646339383036373364336533623362633739386465323830656166353932653937336134373138623265633661386431366339306539",
  "hex",
);
const pair = () => {
  const { privateKey, publicKey } = generateKeyPairSync("ed25519");
  return { privateKey, pub: publicKey.export({ format: "der", type: "spki" }).subarray(-32) };
};
const envelopeFor = (pub, text = "OM release delegation | version 1") => {
  const len = Buffer.alloc(2);
  len.writeUInt16LE(text.length);
  return Buffer.concat([GOLDEN.subarray(0, 51), pub, len, Buffer.from(text)]);
};
// A fake Solana app: derives nothing, signs with one software key, like the device would sign the whole envelope.
const fakeDevice = (k, { tamper = false } = {}) => ({
  getAddress: async () => ({ address: k.pub }),
  signOffchainMessage: async (_p, msg) => ({ signature: sign(null, tamper ? Buffer.concat([msg, Buffer.from("x")]) : msg, k.privateKey) }),
});

test("parses the golden envelope", () => {
  const e = parseEnvelope(GOLDEN);
  assert.equal(e.signerPub, "11".repeat(32));
  assert.equal(e.appDomain, "om.release-delegation/1");
  assert.match(e.text, /^OM release delegation \| version 3 \| expires 2027-01-26T00:00:00Z \| sha256 [0-9a-f]{64}$/);
});

test("rejects every other envelope shape", () => {
  const bad = (f) => assert.throws(() => parseEnvelope(f(Buffer.from(GOLDEN))));
  bad((b) => b.subarray(0, 40));
  bad((b) => { b[0] = 0; return b; });
  bad((b) => { b[16] = 1; return b; }); // header v1
  bad((b) => { b[49] = 1; return b; }); // format 1 (UTF-8, blind sign)
  bad((b) => { b[50] = 2; return b; }); // two signers
  bad((b) => { b.writeUInt16LE(7, 83); return b; });
  bad((b) => Buffer.concat([b, Buffer.from("!")]));
  bad((b) => { b[90] = 0x0a; return b; }); // control char in text
});

test("signs through the injected app, and verifies the signature locally", async () => {
  const k = pair();
  const env = envelopeFor(k.pub);
  const r = await signEnvelope(fakeDevice(k), "44'/501'/0'", env);
  assert.equal(r.pub, k.pub.toString("hex"));
  assert.ok(verifyEd25519(r.pub, env, Buffer.from(r.sigBase64, "base64")));
  assert.equal(r.sigHex.length, 128);
});

test("refuses a payload prepared for another key, and a signature that does not verify", async () => {
  const k = pair();
  await assert.rejects(signEnvelope(fakeDevice(k), "44'/501'/0'", envelopeFor(pair().pub)), /wrong device, path or payload/);
  await assert.rejects(signEnvelope(fakeDevice(k, { tamper: true }), "44'/501'/0'", envelopeFor(k.pub)), /does not verify/);
});

test("derivation paths are Solana hardened paths only", () => {
  assert.equal(checkPath("44'/501'/0'"), "44'/501'/0'");
  assert.equal(checkPath("44'/501'/1'/0'"), "44'/501'/1'/0'");
  for (const p of ["44'/60'/0'", "44/501/0", "m/44'/501'/0'", "44'/501'", "44'/501'/0'/0'/0'"]) assert.throws(() => checkPath(p));
});

test("base58 matches known Solana encodings", () => {
  assert.equal(base58(Buffer.alloc(32)), "11111111111111111111111111111111");
  assert.equal(base58(Buffer.from("00ff", "hex")), "15Q");
});
