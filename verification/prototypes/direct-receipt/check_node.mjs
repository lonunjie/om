// ADR-0194 cross-check: recomputes every expected value from the vectors' inputs with node:crypto (hmac, hkdf) and a
// second, separately written CBOR encoder, and checks every negative differs. Usage: node check_node.mjs VECTORS.json
import { createHash, createHmac, hkdfSync } from "node:crypto";
import { readFileSync } from "node:fs";

const v = JSON.parse(readFileSync(process.argv[2], "utf8"));
const h = (s) => Buffer.from(s, "hex");
const sha = (b) => createHash("sha256").update(b).digest();
const hmac = (k, d) => createHmac("sha256", k).update(d).digest();
const hd = (maj, n) => {
  if (n < 24) return Buffer.from([(maj << 5) | n]);
  if (n < 256) return Buffer.from([(maj << 5) | 24, n]);
  if (n < 65536) return Buffer.from([(maj << 5) | 25, n >> 8, n & 255]);
  const b = Buffer.alloc(5); b[0] = (maj << 5) | 26; b.writeUInt32BE(n, 1); return b;
};
const U = (n) => hd(0, n);
const Bs = (b) => Buffer.concat([hd(2, b.length), b]);
const T = (s) => Buffer.concat([hd(3, Buffer.byteLength(s)), Buffer.from(s)]);
const A = (...xs) => Buffer.concat([hd(4, xs.length), ...xs]);
const L = v.labels;
const dsHash = (label, body) => sha(A(T(label), Bs(body)));
const hk = (salt, ikm, info) => Buffer.from(hkdfSync("sha256", ikm, salt, info, 32));
function tag(key, digest, blob, tid, total, posTid, ct) {
  const k = hk(digest, key, A(T(L.kdf), Bs(blob)));
  const hpos = dsHash(L.hash, A(Bs(posTid), Bs(ct)));
  const macin = A(T(L.mac), Bs(tid), Bs(blob), U(total), Bs(digest), Bs(hpos));
  return { k, hpos, macin, tag: hmac(k, macin) };
}
let bad = 0, n = 0;
const eq = (name, got, want) => { n++; if (got.toString("hex") !== want) { console.error("MISMATCH", name); bad++; } };
for (const c of v.constructions) {
  const ct = h(c.ciphertext), dig = dsHash(L.ciphertext_digest, ct);
  eq(c.id + " digest", dig, c.ciphertext_digest);
  if (ct.length !== c.total_bytes) { console.error("LENGTH", c.id); bad++; }
  const r = tag(h(c.content_key), dig, h(c.blob_id), h(c.transfer_id), ct.length, h(c.transfer_id), ct);
  eq(c.id + " k_rcpt", r.k, c.k_rcpt); eq(c.id + " h_pos", r.hpos, c.h_pos);
  eq(c.id + " mac_input", r.macin, c.mac_input); eq(c.id + " tag", r.tag, c.receipt_tag);
  const frame = Buffer.concat([Buffer.from([0xa4, 0x00, 0x01, 0x01, 0x08, 0x02]), Bs(h(c.transfer_id)), Buffer.from([0x0f]), Bs(r.tag)]);
  eq(c.id + " frame", frame, c.frame);
}
for (const x of v.negatives) {
  const ct = h(x.ciphertext);
  const r = tag(h(x.content_key), h(x.ciphertext_digest), h(x.blob_id), h(x.transfer_id), x.total_bytes, h(x.pos_transfer_id), ct);
  eq("negative " + x.id + " tag", r.tag, x.receipt_tag);
  n++; if (!x.differs || x.receipt_tag === v.constructions.find((c) => c.id === x.against).receipt_tag) { console.error("SAME TAG", x.id); bad++; }
}
n++; if (v.resumed.receipt_tag !== v.constructions.find((c) => c.id === v.resumed.against).receipt_tag) { console.error("RESUMED"); bad++; }

// ---- the LanFrameV1 frames: a second strict decoder (shortest-form integers, definite lengths, ascending integer keys,
// no trailing bytes) driven by spec/registries/om-numeric-fields.json, independent of the Rust codecs.
const REGISTRY = JSON.parse(readFileSync(new URL("../../../spec/registries/om-numeric-fields.json", import.meta.url), "utf8"));
const FRAME = REGISTRY.structs.LanFrameV1.fields;
class Reject extends Error {}
const fail = (c) => { throw new Reject(c); };
function item(b, o) {
  if (o.p >= b.length) fail("truncated");
  const ib = b[o.p++], maj = ib >> 5, ai = ib & 31;
  let n;
  if (ai < 24) n = BigInt(ai);
  else if (ai > 27) fail("non_canonical");
  else {
    const w = [1, 2, 4, 8][ai - 24];
    if (o.p + w > b.length) fail("truncated");
    n = 0n; for (let i = 0; i < w; i++) n = (n << 8n) | BigInt(b[o.p++]);
    if (n < [24n, 256n, 65536n, 4294967296n][ai - 24]) fail("nonminimal_int");
  }
  if (maj === 0) return { t: "u", v: n };
  if (maj === 2 || maj === 3) {
    if (o.p + Number(n) > b.length) fail("truncated");
    const s = b.subarray(o.p, o.p + Number(n)); o.p += Number(n);
    return { t: maj === 2 ? "b" : "s", v: s };
  }
  if (maj === 5) {
    const m = []; let last = -1n;
    for (let i = 0n; i < n; i++) {
      const k = item(b, o);
      if (k.t !== "u") fail("unknown_field");
      if (k.v <= last) fail("unsorted_keys");
      last = k.v;
      m.push([k, item(b, o)]);
    }
    return { t: "m", v: m };
  }
  if (maj === 7 && ai === 22) return { t: "null" };
  if (maj === 7 && (ai === 20 || ai === 21)) return { t: "bool" };
  fail("wrong_type");
}
function checkFrame(buf) {
  const o = { p: 0 };
  const top = item(buf, o);
  if (o.p !== buf.length) fail("trailing_bytes");
  if (top.t === "null") fail("null_forbidden");
  if (top.t !== "m") fail("wrong_type");
  for (const [k, x] of top.v) {
    const f = FRAME.find((f) => BigInt(f.key) === k.v);
    if (!f) fail("unknown_field");
    if (x.t === "null") fail("null_forbidden");
    const ty = f.type;
    if ("uint" in ty) { if (x.t !== "u") fail("wrong_type"); if (x.v < BigInt(ty.uint[0]) || (ty.uint[1] !== null && x.v > BigInt(ty.uint[1]))) fail("value_range"); }
    else if ("bstr" in ty) { if (x.t !== "b") fail("wrong_type"); if (x.v.length < ty.bstr[0] || x.v.length > ty.bstr[1]) fail("value_range"); }
    else if ("enum" in ty) { if (x.t !== "u") fail("wrong_type"); if (!(x.v.toString() in REGISTRY.enums[ty.enum])) fail("value_range"); }
    else throw new Error("unhandled type " + JSON.stringify(ty));
  }
  for (const f of FRAME) if (f.req && !top.v.some(([k]) => k.v === BigInt(f.key))) fail("missing_field");
  return top;
}
for (const fv of v.structs) {
  n++;
  let got = "accept";
  try { checkFrame(h(fv.hex)); } catch (e) { if (e instanceof Reject) got = e.message; else throw e; }
  const want = fv.expect === "accept" ? "accept" : fv.error;
  if (got !== want) { console.error("FRAME", fv.id, "got", got, "want", want); bad++; }
}
console.error(`direct-receipt node check: ${n} checks, ${bad} failures`);
process.exit(bad ? 1 : 0);
