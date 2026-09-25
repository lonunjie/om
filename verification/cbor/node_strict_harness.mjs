#!/usr/bin/env node
// Feasibility prototype for V-005. Not product code. Not for production.
//
// Node.js harness for the W0 object vectors (verification/prototypes/v005-encoding).
// - exact positive bytes: produced by the EXISTING, UNMODIFIED node_canonical.mjs in this
//   directory (run as a subprocess on a temporary fixtures file);
// - strict decoding, rejection and object validation: implemented here from RFC 8949 §3/§4.2.1
//   and spec/registries/*.json, independently of the Rust and Python code.
//
// usage: node node_strict_harness.mjs VECTORS.json > results.json
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import crypto from 'node:crypto';
import { execFileSync } from 'node:child_process';
import { fileURLToPath } from 'node:url';

const HERE = path.dirname(fileURLToPath(import.meta.url));
const REG = process.env.OM_REG || path.join(HERE, '..', '..', 'spec', 'registries');  // OM_REG: additive later-wave overlay (feasibility checks)
const readJson = (p) => JSON.parse(fs.readFileSync(p, 'utf8'));
const FIELDS = readJson(path.join(REG, 'om-numeric-fields.json'));
const TYPES = Object.fromEntries(readJson(path.join(REG, 'om-object-types.json')).types.map((t) => [t.name, t]));
const PROFILE = readJson(path.join(REG, 'om-cbor-profile.json'));
const LIM = PROFILE.limits;
// known extensions: the CBOR profile list plus registered key-23 extensions in om-numeric-fields (ADR-0126)
// registered key-23 extension -> {critical, value struct}
const EXT_SCHEMA = new Map((FIELDS.extensions || []).filter((e) => e.value && e.value.ref).map((e) => [e.name, { critical: e.critical === true, ref: e.value.ref }]));
const KNOWN_EXT = new Set([...PROFILE.extensions.known_extensions, ...(FIELDS.extensions || []).map((e) => e.name)]);
// CryptoProfile id -> AsyncKeyBundle field keys that profile requires (om-crypto-profiles required_bundle_fields)
const CRYPTO = readJson(path.join(REG, 'om-crypto-profiles.json'));
const BUNDLE_REQUIRED = new Map(CRYPTO.profiles.filter((p) => p.required_bundle_fields).map((p) => [BigInt(p.id),
  p.required_bundle_fields.map((n) => FIELDS.structs.AsyncKeyBundle.fields.find((f) => f.name === n).key)]));

// CryptoProfile id -> exact MessageRequest HPKE enc / RequestKey public-key lengths (ADR-0135 §5)
const HPKE_SIZES = new Map(CRYPTO.profiles.filter((p) => p.message_request_hpke && p.message_request_hpke.enc_bytes)
  .map((p) => [BigInt(p.id), { enc: p.message_request_hpke.enc_bytes, pk: p.message_request_hpke.public_key_bytes }]));

class Reject extends Error { constructor(c) { super(c); this.cls = c; } }
const fail = (c) => { throw new Reject(c); };

// ---------------------------------------------------------------- values
// {t:'u', v:BigInt} {t:'n', v:BigInt (the n in -1-n)} {t:'b', v:Buffer} {t:'s', v:string}
// {t:'a', v:[...]} {t:'m', v:[[k,x],...]} {t:'bool', v} {t:'null'}

// ---------------------------------------------------------------- strict decoder
function decode(buf, maxBytes) {
  if (buf.length > maxBytes) fail('size_limit');
  let pos = 0;
  const need = (n) => { if (BigInt(buf.length - pos) < n) fail('truncated'); };
  const u8 = () => { need(1n); return buf[pos++]; };
  function arg(major, ai) {
    const nc = major <= 1 ? 'noncanonical_int' : 'noncanonical_length';
    if (ai < 24) return BigInt(ai);
    let width, min;
    if (ai === 24) { width = 1; min = 24n; }
    else if (ai === 25) { width = 2; min = 0x100n; }
    else if (ai === 26) { width = 4; min = 0x10000n; }
    else if (ai === 27) { width = 8; min = 0x100000000n; }
    else if (ai <= 30) fail('malformed');
    else fail(major >= 2 && major <= 5 ? 'indefinite_length' : 'malformed');
    need(BigInt(width));
    let n = 0n;
    for (let i = 0; i < width; i++) n = (n << 8n) | BigInt(buf[pos++]);
    if (n < min) fail(nc);
    return n;
  }
  function item(depth) {
    const ib = u8();
    const major = ib >> 5, ai = ib & 31;
    if (major === 6) fail('tag_forbidden');
    if (major === 7) {
      if (ai === 20) return { t: 'bool', v: false };
      if (ai === 21) return { t: 'bool', v: true };
      if (ai === 22) return { t: 'null' };
      if (ai >= 25 && ai <= 27) fail('float_forbidden');
      if (ai >= 28 && ai <= 30) fail('malformed');
      if (ai === 31) fail('indefinite_length');
      fail('invalid_simple');
    }
    const n = arg(major, ai);
    if (major === 0) return { t: 'u', v: n };
    if (major === 1) return { t: 'n', v: n };
    if (major === 2 || major === 3) {
      if (n > BigInt(LIM.max_string_bytes)) fail('size_limit');
      need(n);
      const s = buf.subarray(pos, pos + Number(n)); pos += Number(n);
      if (major === 2) return { t: 'b', v: Buffer.from(s) };
      let text;
      try { text = new TextDecoder('utf-8', { fatal: true }).decode(s); } catch { fail('invalid_utf8'); }
      return { t: 's', v: text };
    }
    const d = depth + 1;
    if (d > LIM.max_depth) fail('depth_limit');
    if (n > BigInt(LIM.max_container_items)) fail('count_limit');
    const count = Number(n);
    if (major === 4) {
      const a = [];
      for (let i = 0; i < count; i++) a.push(item(d));
      return { t: 'a', v: a };
    }
    const m = [];
    let prev = null, keyMajor = null;
    for (let i = 0; i < count; i++) {
      const ks = pos;
      const k = item(d);
      const ke = pos;
      if (k.t !== 'u' && k.t !== 's') fail('key_type');
      if (keyMajor === null) keyMajor = k.t; else if (keyMajor !== k.t) fail('key_type');
      if (prev) {
        const c = Buffer.compare(buf.subarray(ks, ke), prev);
        if (c === 0) fail('duplicate_key');
        if (c < 0) fail('unsorted_keys');
      }
      prev = buf.subarray(ks, ke);
      m.push([k, item(d)]);
    }
    return { t: 'm', v: m };
  }
  const v = item(0);
  if (pos !== buf.length) fail('trailing_bytes');
  return v;
}

// ---------------------------------------------------------------- canonical encoder (re-encode + DS inputs)
function head(major, n) {
  n = BigInt(n);
  const m = major << 5;
  if (n < 24n) return Buffer.from([m | Number(n)]);
  if (n < 0x100n) return Buffer.from([m | 24, Number(n)]);
  if (n < 0x10000n) { const b = Buffer.alloc(3); b[0] = m | 25; b.writeUInt16BE(Number(n), 1); return b; }
  if (n < 0x100000000n) { const b = Buffer.alloc(5); b[0] = m | 26; b.writeUInt32BE(Number(n), 1); return b; }
  const b = Buffer.alloc(9); b[0] = m | 27; b.writeBigUInt64BE(n, 1); return b;
}
function encode(v) {
  switch (v.t) {
    case 'u': return head(0, v.v);
    case 'n': return head(1, v.v);
    case 'b': return Buffer.concat([head(2, v.v.length), v.v]);
    case 's': { const b = Buffer.from(v.v, 'utf8'); return Buffer.concat([head(3, b.length), b]); }
    case 'a': return Buffer.concat([head(4, v.v.length), ...v.v.map(encode)]);
    case 'm': {
      const ps = v.v.map(([k, x]) => [encode(k), encode(x)]).sort((a, b) => Buffer.compare(a[0], b[0]));
      return Buffer.concat([head(5, ps.length), ...ps.flat()]);
    }
    case 'bool': return Buffer.from([v.v ? 0xf5 : 0xf4]);
    case 'null': return Buffer.from([0xf6]);
  }
  throw new Error('bad value');
}
const U = (n) => ({ t: 'u', v: BigInt(n) });
const get = (m, k) => { const p = m.v.find(([kk]) => kk.t === 'u' && kk.v === BigInt(k)); return p ? p[1] : undefined; };
const has = (m, k) => get(m, k) !== undefined;

// ---------------------------------------------------------------- registry-driven validation
const COMMON = FIELDS.common_top_level_fields;
function structDef(name) {
  const s = FIELDS.structs[name];
  if (s.union) return s;
  return { top: !!s.top_level, fields: (s.top_level ? [...s.fields, ...COMMON] : s.fields).slice().sort((a, b) => a.key - b.key) };
}
const inRange = (n, [a, b]) => n >= BigInt(a) && (b === null || n <= BigInt(b));

function checkType(x, ty, parent, pfields) {
  if (x.t === 'null' && !('any' in ty)) fail('null_forbidden');
  if ('uint' in ty) { if (x.t !== 'u') fail('wrong_type'); if (!inRange(x.v, ty.uint)) fail('value_range'); return; }
  if ('bool' in ty) { if (x.t !== 'bool') fail('wrong_type'); if (ty.bool !== null && ty.bool !== x.v) fail('value_range'); return; }
  if ('bstr' in ty) { if (x.t !== 'b') fail('wrong_type'); if (!inRange(BigInt(x.v.length), ty.bstr)) fail('value_range'); return; }
  if ('tstr' in ty) {
    if (x.t !== 's') fail('wrong_type');
    if (!inRange(BigInt(Buffer.byteLength(x.v, 'utf8')), ty.tstr)) fail('value_range'); return;
  }
  if ('enum' in ty) { if (x.t !== 'u') fail('wrong_type'); if (!(x.v.toString() in FIELDS.enums[ty.enum])) fail('value_range'); return; }
  if ('ref' in ty) return checkStruct(x, ty.ref, null);
  if ('array' in ty || 'set' in ty) {
    if (x.t !== 'a') fail('wrong_type');
    if (!inRange(BigInt(x.v.length), ty.n)) fail('value_range');
    for (const e of x.v) checkType(e, ty.array || ty.set, null, []);
    if ('set' in ty) {
      const enc = x.v.map(encode);
      for (let i = 1; i < enc.length; i++) if (Buffer.compare(enc[i - 1], enc[i]) >= 0) fail('set_order');
    }
    return;
  }
  if ('dispatch' in ty) {
    const f = pfields.find((f) => f.name === ty.dispatch.on);
    const sel = get(parent, f.key);
    const target = sel && sel.t === 'u' ? ty.dispatch.table[sel.v.toString()] : undefined;
    if (target) return checkStruct(x, target, null);
    if (x.t !== 'm') fail('wrong_type'); // opaque later-wave body
    return;
  }
  if ('signed' in ty) {
    if (x.t !== 'b') fail('wrong_type');
    const t = TYPES[ty.signed];
    return checkEnvelope(decode(x.v, t.max_bytes), t);
  }
  if ('any' in ty) return;
  if ('ext' in ty) {
    if (x.t !== 'm') fail('wrong_type');
    if (x.v.length === 0) fail('value_range');
    for (const [k, e] of x.v) {
      if (k.t !== 's') fail('wrong_type');
      const len = Buffer.byteLength(k.v, 'utf8');
      if (len < 1 || len > 128) fail('value_range');
      checkStruct(e, 'ExtEntry', null);
      const crit = get(e, 0);
      if (crit.v === true && !KNOWN_EXT.has(k.v)) fail('unknown_critical_extension');
      const reg = EXT_SCHEMA.get(k.v);
      if (reg) {
        if (crit.v !== reg.critical) fail('value_range');
        const val = get(e, 1);
        if (val === undefined) fail('missing_field');
        checkStruct(val, reg.ref, null);
      }
    }
    return;
  }
  throw new Error('unknown registry type ' + JSON.stringify(ty));
}

function checkStruct(x, name, topId) {
  const s = structDef(name);
  if (x.t === 'null') fail('null_forbidden');
  if (x.t !== 'm') fail('wrong_type');
  if (s.union) {
    const d = get(x, s.union.on);
    if (d === undefined) fail('missing_field');
    if (d.t === 'null') fail('null_forbidden');
    if (d.t !== 'u') fail('wrong_type');
    const variant = s.union.variants[d.v.toString()];
    if (!variant) fail('value_range');
    return checkStruct(x, variant, null);
  }
  for (const [k, val] of x.v) {
    if (k.t !== 'u') fail('unknown_field');
    const f = s.fields.find((f) => BigInt(f.key) === k.v);
    if (!f) fail('unknown_field');
    if (s.top && f.key === 0) {
      if (val.t === 'null') fail('null_forbidden');
      if (val.t !== 'u') fail('wrong_type');
      if (val.v !== BigInt(topId)) fail('wrong_object_type');
    } else if (s.top && f.key === 1) {
      if (val.t === 'null') fail('null_forbidden');
      if (val.t !== 'u') fail('wrong_type');
      if (val.v !== 1n) fail('unsupported_version');
    } else checkType(val, f.type, x, s.fields);
  }
  for (const f of s.fields) if (f.req && !has(x, f.key)) fail('missing_field');
  if (name === 'AsyncKeyBundle') {
    const prof = get(x, 5);
    for (const k of BUNDLE_REQUIRED.get(prof.v) || []) if (!has(x, k)) fail('missing_field');
  }
  const pin = (pk, bk, which) => {
    const s = has(x, pk) && has(x, bk) ? HPKE_SIZES.get(get(x, pk).v) : undefined;
    if (s && get(x, bk).v.length !== s[which]) fail('value_range');
  };
  if (name === 'MessageRequest') pin(4, 5, 'enc');
  if (name === 'RequestKey') pin(0, 1, 'pk');
}

function checkEnvelope(v, t) {
  const env = structDef('SignedEnvelope');
  if (v.t === 'null') fail('null_forbidden');
  if (v.t !== 'm') fail('wrong_type');
  for (const [k, val] of v.v) {
    const f = k.t === 'u' ? env.fields.find((f) => BigInt(f.key) === k.v) : undefined;
    if (!f) fail('unknown_field');
    if (f.key === 1) {
      if (val.t === 'null') fail('null_forbidden');
      if (val.t !== 'b') fail('wrong_type');
      checkStruct(decode(val.v, t.max_bytes), t.name, t.type_id);
    } else checkType(val, f.type, v, env.fields);
  }
  for (const f of env.fields) if (f.req && !has(v, f.key)) fail('missing_field');
}

function checkTop(name, bytes) {
  const t = TYPES[name];
  const v = decode(bytes, t.max_bytes);
  if (t.signed) checkEnvelope(v, t); else checkStruct(v, t.name, t.type_id);
  return v;
}

// ---------------------------------------------------------------- derived signature/hash inputs
const T = (s) => ({ t: 's', v: s });
const Bv = (b) => ({ t: 'b', v: b });
const dsSig = (label, profile, role, signer, body) => encode({ t: 'a', v: [T(label), profile, role, Bv(signer), Bv(body)] });
const dsHash = (label, body) => encode({ t: 'a', v: [T(label), Bv(body)] });

function derived(name, top, bytes) {
  const t = TYPES[name];
  const out = [];
  let body = bytes;
  if (t.signed) {
    body = get(top, 1).v;
    for (const s of get(top, 2).v) out.push({ kind: 'sig_input', hex: dsSig(t.signed.label, get(s, 1), get(s, 2), get(s, 0).v, body).toString('hex') });
  }
  if (t.hash) {
    const hi = dsHash(t.hash, body);
    out.push({ kind: 'hash', input_hex: hi.toString('hex'), sha256: crypto.createHash('sha256').update(hi).digest('hex') });
  }
  if (name === 'AsyncKeyBundle') {
    const b = decode(body, 1 << 20);
    for (let k = 7; k <= 11; k++) {
      const x = get(b, k);
      if (!x) continue;
      for (const it of x.t === 'a' ? x.v : [x]) {
        if (!has(it, 3)) continue;
        const stripped = { t: 'm', v: it.v.filter(([kk]) => kk.v !== 3n) };
        out.push({ kind: 'sig_input', hex: dsSig('OM-PREKEY-SIG-v1', get(b, 5), U(3), get(b, 2).v, encode(stripped)).toString('hex') });
      }
    }
  }
  return out;
}

// ---------------------------------------------------------------- vectors
function expandHex(h) {
  if (typeof h === 'string') return Buffer.from(h, 'hex');
  return Buffer.concat(h.map((p) => (typeof p === 'string' ? Buffer.from(p, 'hex') : Buffer.alloc(p.len, parseInt(p.fill, 16)))));
}
const safeInts = (j) => JSON.stringify(j, (k, v) => { if (typeof v === 'number' && !Number.isSafeInteger(v)) throw new Error('unsafe'); return v; });

function main(file) {
  const doc = readJson(file);
  // exact bytes from the unmodified original encoder, for every positive value it can represent
  const fixtures = [];
  const raw = JSON.parse(fs.readFileSync(file, 'utf8'), (k, v, ctx) => (typeof v === 'number' && ctx && !Number.isSafeInteger(v) ? { $unsafe: ctx.source } : v));
  for (const v of raw.vectors) {
    if (v.expect !== 'accept') continue;
    let ok = true;
    try { safeInts(v.value); } catch { ok = false; }
    if (JSON.stringify(v.value).includes('$unsafe')) ok = false;
    if (ok) fixtures.push({ id: v.id + '#value', value: v.value });
    if (ok && v.body_value) fixtures.push({ id: v.id + '#body', value: v.body_value });
  }
  const tmp = path.join(fs.mkdtempSync(path.join(os.tmpdir(), 'v005-')), 'fixtures.json');
  fs.writeFileSync(tmp, JSON.stringify(fixtures));
  const orig = JSON.parse(execFileSync(process.execPath, [path.join(HERE, 'node_canonical.mjs'), tmp], { encoding: 'utf8', maxBuffer: 1 << 28 }));
  fs.rmSync(path.dirname(tmp), { recursive: true });

  const out = {};
  let skippedUnsafe = 0;
  for (const v of doc.vectors) {
    const bytes = expandHex(v.hex);
    try {
      const top = checkTop(v.object, bytes);
      const r = { verdict: 'accept', error: null, reencode_equal: Buffer.compare(encode(top), bytes) === 0 };
      if (v.value !== undefined) {
        const h = orig[v.id + '#value'];
        if (h === undefined) { r.value_encode_equal = null; skippedUnsafe++; } else r.value_encode_equal = h === bytes.toString('hex');
      }
      if (v.body_value !== undefined) {
        const h = orig[v.id + '#body'];
        r.body_value_encode_equal = h === undefined ? null : h === get(top, 1).v.toString('hex');
      }
      r.derived = derived(v.object, top, bytes);
      out[v.id] = r;
    } catch (e) {
      if (!(e instanceof Reject)) throw e;
      out[v.id] = { verdict: 'reject', error: e.cls };
    }
  }
  out['#meta'] = { original_encoder_skipped_unsafe_int_vectors: skippedUnsafe };
  process.stdout.write(JSON.stringify(out) + '\n');
}

main(process.argv[2]);
