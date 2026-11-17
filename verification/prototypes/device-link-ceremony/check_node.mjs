// ADR-0192 cross-check: recomputes every expected value from the vectors' inputs with node:crypto (hmac, hkdf,
// chacha20-poly1305 over an HChaCha20 subkey) and a second, separately written CBOR encoder. Usage: node check_node.mjs VECTORS.json
import { createHash, createHmac, createCipheriv, hkdfSync } from "node:crypto";
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
const info = (p) => A(T(L.kdf), T(p));
const hk = (salt, ikm, inf) => Buffer.from(hkdfSync("sha256", ikm, salt, inf, 32));
const macin = (role, body) => A(T(L.mac), U(role), Bs(body));
const rotl = (x, n) => ((x << n) | (x >>> (32 - n))) >>> 0;
function hchacha(key, n16) {
  const s = new Uint32Array(16);
  [0x61707865, 0x3320646e, 0x79622d32, 0x6b206574].forEach((c, i) => (s[i] = c));
  for (let i = 0; i < 8; i++) s[4 + i] = key.readUInt32LE(i * 4);
  for (let i = 0; i < 4; i++) s[12 + i] = n16.readUInt32LE(i * 4);
  const q = (a, b, c, d) => {
    s[a] = (s[a] + s[b]) >>> 0; s[d] = rotl(s[d] ^ s[a], 16);
    s[c] = (s[c] + s[d]) >>> 0; s[b] = rotl(s[b] ^ s[c], 12);
    s[a] = (s[a] + s[b]) >>> 0; s[d] = rotl(s[d] ^ s[a], 8);
    s[c] = (s[c] + s[d]) >>> 0; s[b] = rotl(s[b] ^ s[c], 7);
  };
  for (let i = 0; i < 10; i++) {
    q(0, 4, 8, 12); q(1, 5, 9, 13); q(2, 6, 10, 14); q(3, 7, 11, 15);
    q(0, 5, 10, 15); q(1, 6, 11, 12); q(2, 7, 8, 13); q(3, 4, 9, 14);
  }
  const o = Buffer.alloc(32);
  [0, 1, 2, 3, 12, 13, 14, 15].forEach((j, i) => o.writeUInt32LE(s[j], i * 4));
  return o;
}
function seal(key, nonce, aad, pt) {
  const sub = hchacha(key, nonce.subarray(0, 16));
  const n12 = Buffer.concat([Buffer.alloc(4), nonce.subarray(16)]);
  const c = createCipheriv("chacha20-poly1305", sub, n12, { authTagLength: 16 });
  c.setAAD(aad);
  const ct = Buffer.concat([c.update(pt), c.final()]);
  return Buffer.concat([ct, c.getAuthTag()]);
}
let bad = 0;
const eq = (name, got, want) => { if (got.toString("hex") !== want) { console.error("MISMATCH", name); bad++; } };
const i = v.inputs, e = v.expected;
const ctx = A(U(2), U(i.expiry_s), Bs(h(i.home_uri)), Bs(h(i.a_cap)), Bs(h(i.b_recv)));
const id = dsHash(L.kdf, ctx);
const secret = h(i.secret);
eq("ctx", ctx, e.ctx); eq("ceremony_id", id, e.ceremony_id);
eq("prk", hmac(id, secret), e.prk);
const K = {};
for (const p of ["req", "resp", "code", "bind"]) { K[p] = hk(id, secret, info(p)); eq("k_" + p, K[p], e["k_" + p]); }
const ad = (r) => A(T(L.aead_ad), Bs(id), U(r));
eq("ad_req", ad(1), e.ad_req); eq("ad_resp", ad(2), e.ad_resp);
const m = hmac(K.code, macin(1, h(i.request)));
eq("code_mac", m, e.code_mac);
const code = String(Number(m.readBigUInt64BE(0) % 1000000n)).padStart(6, "0");
if (code !== e.code) { console.error("MISMATCH code"); bad++; }
const rd = sha(h(i.request)), nd = sha(h(i.notice));
eq("request_digest", rd, e.request_digest); eq("notice_digest", nd, e.notice_digest);
const bnd = (r, ap, sh, n, ro, hi) => hmac(K.bind, macin(2, A(Bs(r), Bs(ap), Bs(sh), Bs(n), U(ro), U(hi))));
eq("binding", bnd(rd, h(i.approver), h(i.state_hash), nd, i.roster_count, i.history_count), e.binding);
eq("sealed_req", seal(K.req, h(i.nonce), ad(1), h(i.plaintext)), e.sealed_req);
eq("sealed_resp", seal(K.resp, h(i.nonce), ad(2), h(i.plaintext)), e.sealed_resp);
eq("mailbox", dsHash(L.mailbox, h(i.b_recv)).subarray(0, 16), e.mailbox);
for (const c of v.code_digits) {
  const d = String(Number(h(c.m).readBigUInt64BE(0) % 1000000n)).padStart(6, "0");
  if (d !== c.digits || d.slice(0, 3) + " " + d.slice(3) !== c.grouped) { console.error("MISMATCH digits", c.m); bad++; }
}
for (const c of v.context_changes) {
  if (c.k_req === e.k_req || c.k_resp === e.k_resp || c.k_code === e.k_code || c.k_bind === e.k_bind || (c.name !== "secret" && c.ceremony_id === e.ceremony_id)) { console.error("NOT DIFFERENT", c.name); bad++; }
}
const n = v.negatives;
for (const [k, val] of Object.entries(n)) if (k.startsWith("binding_") || k.startsWith("sealed_") || k === "code_mac_replayed_as_binding_differs") { if (val === e.binding || val === e.sealed_req) { console.error("NOT DIFFERENT", k); bad++; } }
if (n.reencoded_request_code_mac === e.code_mac) { console.error("NOT DIFFERENT reencoded"); bad++; }
// ADR-0195 continuation manifest and tag.
const mdig = (st, cr) => dsHash(L.manifest, A(A(...st.map(Bs)), A(...cr.map(Bs))));
const cbind = (key, tag, dg, ns, nc, nch) => hmac(key, A(T(L.continuation_mac), Bs(A(Bs(tag), Bs(dg), U(ns), U(nc), U(nch)))));
const ct = v.continuation;
const sts = ct.states.map(h), crs = ct.credentials.map(h);
eq("manifest digest", mdig(sts, crs), ct.digest);
eq("cont binding", cbind(K.bind, h(e.binding), h(ct.digest), sts.length, crs.length, ct.chunks), ct.cont_binding);
const cn = ct.negatives;
eq("digest_states_reordered", mdig([sts[1], sts[0], sts[2]], crs), cn.digest_states_reordered);
eq("digest_creds_reordered", mdig(sts, [crs[1], crs[0], ...crs.slice(2)]), cn.digest_creds_reordered);
eq("digest_state_dropped", mdig(sts.slice(0, 2), crs), cn.digest_state_dropped);
eq("digest_cred_dropped", mdig(sts, crs.slice(0, 3)), cn.digest_cred_dropped);
eq("digest_state_moved_to_creds", mdig(sts.slice(0, 2), [sts[2], ...crs]), cn.digest_state_moved_to_creds);
eq("cont_binding_other_digest", cbind(K.bind, h(e.binding), mdig(sts.slice(0, 2), crs), 3, 4, 2), cn.cont_binding_other_digest);
eq("cont_binding_other_states", cbind(K.bind, h(e.binding), h(ct.digest), 2, 4, 2), cn.cont_binding_other_states);
eq("cont_binding_other_creds", cbind(K.bind, h(e.binding), h(ct.digest), 3, 5, 2), cn.cont_binding_other_creds);
eq("cont_binding_other_chunks", cbind(K.bind, h(e.binding), h(ct.digest), 3, 4, 3), cn.cont_binding_other_chunks);
eq("cont_binding_under_code_key", cbind(K.code, h(e.binding), h(ct.digest), 3, 4, 2), cn.cont_binding_under_code_key);
for (const [k, val] of Object.entries(cn)) if (val === ct.digest || val === ct.cont_binding) { console.error("NOT DIFFERENT", k); bad++; }
console.log(bad === 0 ? "device-link-ceremony node cross-check: PASS" : "FAIL");
process.exit(bad ? 1 : 0);
