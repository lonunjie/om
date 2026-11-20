// SPDX-License-Identifier: Apache-2.0
// ADR-0254 golden files, using independent deterministic encoding and crypto.
// node gen_pack_file_vectors.mjs /path/to/@noble/ciphers/aes.js > pack-file-vectors.json
// Use @noble/ciphers 2.4.0. All keys, seeds and salts below are public test fixtures.
import { createHash, createPrivateKey, createPublicKey, hkdfSync, sign } from "node:crypto";
import { createRequire } from "node:module";
import { resolve } from "node:path";
import { pathToFileURL } from "node:url";

const require = createRequire(new URL("../../../apps/web/package.json", import.meta.url));
const { ml_dsa65 } = await import(pathToFileURL(require.resolve("@noble/post-quantum/ml-dsa.js")).href);
const { gcmsiv } = await import(pathToFileURL(resolve(process.argv[2])).href);

const map = (...pairs) => new Map(pairs);
function head(type, size) {
  if (size < 24) return Buffer.from([(type << 5) | size]);
  const n = size <= 0xff ? 1 : size <= 0xffff ? 2 : 4;
  const out = Buffer.alloc(1 + n);
  out[0] = (type << 5) | (n === 1 ? 24 : n === 2 ? 25 : 26);
  out.writeUIntBE(size, 1, n);
  return out;
}
function encode(value) {
  if (typeof value === "number") return head(0, value);
  if (typeof value === "string") {
    const text = Buffer.from(value);
    return Buffer.concat([head(3, text.length), text]);
  }
  if (value instanceof Uint8Array) return Buffer.concat([head(2, value.length), value]);
  if (Array.isArray(value)) return Buffer.concat([head(4, value.length), ...value.map(encode)]);
  const entries = [...value].map(([k, v]) => [encode(k), encode(v)]);
  entries.sort(([a], [b]) => a.length - b.length || Buffer.compare(a, b));
  return Buffer.concat([head(5, entries.length), ...entries.flat()]);
}

const sha = (bytes) => createHash("sha256").update(bytes).digest();
const hash = (label, bytes) => sha(encode([label, bytes]));
const digest = (bytes) => map([0, 1], [1, bytes]);
const packId = Buffer.alloc(32, 0x44);
const image = Buffer.from("UklGRmQAAABXRUJQVlA4TFgAAAAvF8AFABcgEEhs0h9xDYFAYhf0cgsEEozs9rd4/uMPFTBoG8lRtHAO0H2BsCiOxEOo4XkFQkT/J4ClCjQN+LT0VGn55x8Tqr388+95nwbWS/i09JoGoCwF", "base64");
const label = Buffer.from("OM-BLOB-CONTENT-AEAD-AD-v1");
function asset(id) {
  const blobId = Buffer.alloc(32, id);
  const key = Buffer.alloc(32, id + 1);
  const expanded = Buffer.from(hkdfSync("sha256", key, blobId, label, 44));
  const size = Buffer.alloc(8);
  size.writeBigUInt64BE(BigInt(image.length));
  const bytes = gcmsiv(expanded.subarray(0, 32), expanded.subarray(32), Buffer.concat([label, blobId, size])).encrypt(image);
  const d = digest(hash("OM-BLOB-CIPHERTEXT-HASH-v1", bytes));
  const ref = map([0, blobId], [1, "image/webp"], [2, image.length], [3, d], [4, map([0, 1], [1, key])],
    [7, 24], [8, 24], [11, 1], [12, "om.expressive.webp-v1"], [13, "A smiling sticker"], [14, "🙂"]);
  return {
    item: map([0, "hello"], [1, 1], [2, ref], [5, "Hello smile"]),
    blob: map(["digest", d], ["bytes", bytes]),
  };
}
const firstAsset = asset(0x55);
const secondAsset = asset(0x56);
const manifest = (sequence, item, previous) => encode(map(
  [0, 45], [1, 1], [2, packId], [3, sequence], ...(previous ? [[4, digest(previous)]] : []),
  [5, "Little faces"], [7, [1]], [8, [item]], [9, map([0, "Example creator"], [1, ["https://creator.invalid/pack"]])],
  [10, map([0, "CC0-1.0"], [1, "https://creator.invalid/license"])],
));

const ed = createPrivateKey({ key: Buffer.concat([Buffer.from("302e020100300506032b657004220420", "hex"), Buffer.alloc(32, 3)]), format: "der", type: "pkcs8" });
const pq = ml_dsa65.keygen(Buffer.alloc(32, 4));
const publicKey = Buffer.concat([createPublicKey(ed).export({ format: "der", type: "spki" }).subarray(-32), pq.publicKey]);
const kid = hash("OM-CONTENT-PACK-CONTROLLER-KEY-ID-v1", encode([1, publicKey])).subarray(0, 16);
const controller = map([0, kid], [1, 1], [2, publicKey]);
function control(sequence, releaseId, previous) {
  const body = encode(map(
    [0, 46], [1, 1], [2, packId], [3, sequence], ...(previous ? [[4, digest(previous)]] : []),
    [5, [controller]], [6, 1], [7, digest(releaseId)], [8, sequence],
  ));
  const tbs = encode(["OM-CONTENT-PACK-CONTROL-SIG-v1", 1, 1, kid, body]);
  const signature = Buffer.concat([sign(null, tbs, ed), ml_dsa65.sign(tbs, pq.secretKey, { extraEntropy: Buffer.alloc(32, 5) })]);
  const envelope = encode(map([1, body], [2, [map([0, kid], [1, 1], [2, 1], [3, signature])]]));
  return { envelope, hash: hash("OM-CONTENT-PACK-CONTROL-HASH-v1", body) };
}

const releaseId = (release) => hash("OM-CONTENT-PACK-RELEASE-HASH-v1", release);
const first = manifest(1, firstAsset.item);
const second = manifest(2, secondAsset.item, releaseId(first));
const genesis = control(1, releaseId(first));
const update = control(2, releaseId(second), genesis.hash);
function vector(id, sequence, release, states, blob, predecessors) {
  const file = encode(map(
    [1, "om.content-pack-file.v1"], [2, release], [3, states],
    [4, [blob]],
    ...(predecessors ? [[5, predecessors]] : []),
  ));
  return { id, sequence, releaseId: releaseId(release).toString("hex"),
    controlDigest: sha(states.at(-1)).toString("hex"), hex: file.toString("hex") };
}
process.stdout.write(JSON.stringify({ vectors: [
  vector("genesis", 1, first, [genesis.envelope], firstAsset.blob),
  vector("portable-update", 2, second, [genesis.envelope, update.envelope], secondAsset.blob, [first]),
  vector("pinned-update", 2, second, [update.envelope], secondAsset.blob),
] }, null, 2) + "\n");
