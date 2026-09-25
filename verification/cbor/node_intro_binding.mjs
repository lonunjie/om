// SPDX-License-Identifier: Apache-2.0
// Feasibility prototype for V-005. Not product code. Not for production.
// Independent Node checker for the R-280 reciprocal IntroIdentityBinding sequences (ADR-0131 rule 8) in the V-005
// vector file. Written from the ADR, not from gen/intro_binding.py.
//   node node_intro_binding.mjs VECTORS.json   -> JSON summary; exit 1 on mismatch
import { readFileSync } from 'node:fs';
import { createHash } from 'node:crypto';

const TEST_SIG = Buffer.from('insecure_test_signature');

// minimal decoder for canonical input (every object here is also a codec-checked V-005 vector)
function decode(buf) {
  let i = 0;
  const item = () => {
    const ib = buf[i++]; const major = ib >> 5; const ai = ib & 31;
    let n = ai;
    if (ai >= 24) { const w = 1 << (ai - 24); n = 0; for (let k = 0; k < w; k++) n = n * 256 + buf[i++]; }
    switch (major) {
      case 0: return n;
      case 2: { const b = buf.subarray(i, i + n); i += n; return b; }
      case 3: { const s = buf.subarray(i, i + n).toString('utf8'); i += n; return s; }
      case 4: return Array.from({ length: n }, item);
      case 5: { const m = new Map(); for (let k = 0; k < n; k++) { const key = item(); m.set(key, item()); } return m; }
      case 7: return { 20: false, 21: true, 22: null }[n];
      default: throw new Error('unsupported');
    }
  };
  const v = item();
  if (i !== buf.length) throw new Error('trailing');
  return v;
}

const head = (major, n) => n < 24 ? Buffer.from([major << 5 | n]) : n < 256 ? Buffer.from([major << 5 | 24, n])
  : Buffer.from([major << 5 | 25, n >> 8, n & 255]);
const tstr = (s) => Buffer.concat([head(3, Buffer.byteLength(s)), Buffer.from(s)]);
const bstr = (b) => Buffer.concat([head(2, b.length), b]);
// DS-HASH(label, body) = SHA-256(OM-CBOR [label, body])
const dsHash = (label, body) => createHash('sha256').update(Buffer.concat([head(4, 2), tstr(label), bstr(body)])).digest();

const eqDigest = (d, hex) => d instanceof Map && d.get(0) === 1 && Buffer.isBuffer(d.get(1)) && d.get(1).toString('hex') === hex;

function verifies(envHex, ctx) {
  const acsBody = Buffer.from(ctx.peer_acs_body, 'hex');
  if (dsHash('OM-ACS-HASH-v1', acsBody).toString('hex') !== ctx.trusted_peer_acs_hash) return false;
  const acs = decode(acsBody);
  const env = decode(Buffer.from(envHex, 'hex'));
  const sigs = env.get(2);
  if (sigs.length !== 1) return false;
  const [sig] = sigs;
  if (sig.get(2) !== 3 || !sig.get(3).equals(TEST_SIG)) return false;
  const ids = (k) => (acs.get(k) || []).map((b) => b.toString('hex'));
  const signer = sig.get(0).toString('hex');
  const active = [...ids(5), ...ids(7)].includes(signer);
  const barred = [...ids(6), ...ids(8)].includes(signer);
  if (!active || barred) return false;
  const body = decode(env.get(1));
  return body.get(0) === 21 && body.get(2).equals(acs.get(2)) && body.get(3).toString('hex') === ctx.peer_intro_expected
    && eqDigest(body.get(4), ctx.request_key_digest) && body.get(5) === acs.get(3) && eqDigest(body.get(6), ctx.trusted_peer_acs_hash);
}

function run(seq) {
  const requester = seq.perspective === 'requester';
  let state = requester ? 'ACCEPTED_UNBOUND' : 'NONE';
  let ownSent = false; let peerOk = false;
  return seq.steps.map((st) => {
    let result;
    if (st.op === 'accept_request') { state = 'ACCEPTED_UNBOUND'; result = 'ok'; }
    else if (st.op === 'send_own_binding') {
      if (requester && !peerOk) result = 'refused';
      else { ownSent = true; result = 'ok'; }
    } else if (!requester && !ownSent) result = 'rejected';
    else if (verifies(st.binding, seq.context)) { peerOk = true; result = 'ok'; }
    else result = 'rejected';
    if (ownSent && peerOk) state = 'ACTIVE';
    return { result, state };
  });
}

const doc = JSON.parse(readFileSync(process.argv[2], 'utf8'));
const seqs = doc.intro_binding_sequences;
const bad = seqs.filter((s) => JSON.stringify(run(s)) !== JSON.stringify(s.expect)).map((s) => s.id);
console.log(JSON.stringify({ implementation: 'node', sequences: seqs.length,
  reach_active: seqs.filter((s) => s.expect.at(-1).state === 'ACTIVE').length, mismatches: bad }));
process.exit(bad.length ? 1 : 0);
