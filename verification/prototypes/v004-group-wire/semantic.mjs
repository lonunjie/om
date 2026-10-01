// V-004 group wire freeze (ADR-0176 section 3): Node semantic checker. Feasibility evidence, not product code.
// node semantic.mjs SEMANTIC.json > results.json
// Written from the rule list in ADR-0176 section 3, table-driven, without reading semantic.py.
import fs from 'node:fs';

const map = (v) => new Map(v.$map.map(([k, x]) => [k, x]));
const hex = (v) => v.$bytes;
const digestHex = (v) => hex(map(v).get(1));
const principalKey = (p) => { const m = map(p); return `${m.get(0)}:${hex(m.get(1))}`; };
const dup = (xs) => new Set(xs).size !== xs.length;

// operation -> {reducing: 'always'|'never'|'depends', kinds: allowed authorization kinds}
const OPS = {
  1: { reducing: 'never', kinds: [1, 2] }, 2: { reducing: 'always', kinds: [1, 2] }, 3: { reducing: 'always', kinds: [3] },
  4: { reducing: 'never', kinds: [1, 4], head: true }, 5: { reducing: 'always', kinds: [4], head: true }, 6: { reducing: 'never', kinds: [1] },
  7: { reducing: 'never', kinds: [1] }, 8: { reducing: 'depends', kinds: [1, 2] }, 9: { reducing: 'depends', kinds: [1, 2] },
  10: { reducing: 'never', kinds: [1] }, 11: { reducing: 'always', kinds: [1, 3] }, 12: { reducing: 'never', kinds: [5] },
  13: { reducing: 'depends', kinds: [1] }, 14: { reducing: 'depends', kinds: [1] }, 15: { reducing: 'depends', kinds: [1] },
  16: { reducing: 'always', kinds: [3] },
};

// Each rule returns a reason string when violated. Rules run in order; the first violation is the verdict.
const stateRules = [
  (s) => ((s.get(3) === 1) !== !s.has(4) ? 'version_previous_mismatch' : null),
  (s) => (dup(s.get(6).map((m) => principalKey(map(m).get(0)))) ? 'membership_principal_duplicate' : null),
  (s) => (dup(s.get(6).map((m) => hex(map(m).get(1)))) ? 'membership_incarnation_duplicate' : null),
  (s) => (dup((s.get(7) ?? []).map((r) => hex(map(r).get(0)))) ? 'role_definition_duplicate' : null),
  (s) => {
    const defined = new Set((s.get(7) ?? []).map((r) => hex(map(r).get(0))));
    return s.get(6).some((m) => (map(m).get(2) ?? []).some((r) => !defined.has(hex(r)))) ? 'role_unknown' : null;
  },
  (s) => (s.get(6).some((m) => { const mm = map(m); return (map(mm.get(0)).get(0) === 1) !== (mm.get(3) === 1); }) ? 'endpoint_policy_kind_mismatch' : null),
  (s) => (dup(s.get(10).map(hex)) ? 'succession_duplicate' : null),
  (s) => { const p = map(s.get(9)); return (p.get(2) === 2) !== p.has(3) ? 'history_window_mismatch' : null; },
  (s) => (dup((s.get(11) ?? []).map((i) => hex(map(i).get(0)))) ? 'invite_join_ref_duplicate' : null),
  (s) => {
    const members = new Map(s.get(6).map((m) => { const mm = map(m); return [principalKey(mm.get(0)), hex(mm.get(1))]; }));
    for (const raw of s.get(11) ?? []) {
      const i = map(raw);
      if (i.has(5) && i.get(6) > i.get(5)) return 'invite_uses_exceed';
      if (members.get(principalKey(i.get(1))) !== hex(i.get(2))) return 'invite_issuer_not_member';
    }
    return null;
  },
  (s) => {
    const members = new Set(s.get(6).map((m) => principalKey(map(m).get(0))));
    return map(s.get(8)).get(0).some((p) => !members.has(principalKey(p))) ? 'governance_principal_not_member' : null;
  },
  (s) => { const g = map(s.get(8)); return g.get(1) > g.get(0).length ? 'governance_threshold_exceeds' : null; },
];

const intentRules = [
  (i, op, a) => { const r = OPS[op].reducing; const flag = i.get(7); return (r === 'always' && !flag) || (r === 'never' && flag) ? 'reducing_flag_mismatch' : null; },
  (i, op) => (OPS[op].kinds.includes(map(i.get(9)).get(0)) ? null : 'authorization_kind_not_allowed'),
  (i, op, a) => (op === 1 && a.has(4) && map(i.get(9)).get(0) !== 1 ? 'authorization_kind_not_allowed' : null),
  (i, op) => {
    const z = map(i.get(9)); const kind = z.get(0);
    const wantHead = kind === 4 || OPS[op].head === true;
    return z.has(1) !== wantHead || z.has(2) !== (kind === 2) || z.has(3) !== (kind === 5) ? 'authorization_field_mismatch' : null;
  },
  (i, op, a) => (op === 12 && hex(map(i.get(9)).get(3)) !== hex(a.get(0)) ? 'join_ref_mismatch' : null),
  (i, op, a) => (op === 14 && a.size === 0 ? 'security_policy_empty' : null),
  (i, op, a) => (op === 14 && a.has(2) && a.has(1) && a.get(1) !== 2 ? 'history_window_mismatch' : null),
  (i, op, a) => (op === 15 && a.size === 0 ? 'governance_set_empty' : null),
  (i, op, a) => (op === 15 && a.has(1) && dup(a.get(1).map(hex)) ? 'succession_duplicate' : null),
  (i, op, a) => (op === 16 && hex(a.get(0)) === hex(a.get(1)) ? 'coordinator_same_device' : null),
  (i, op, a) => (op === 10 && map(a.get(0)).get(6) !== 0 ? 'invite_uses_nonzero' : null),
];

const bindingRules = [
  (b) => (b.get(3) === 3 && b.has(9) ? 'recovery_has_intent' : null),
  (b) => (b.get(3) === 3 && digestHex(b.get(5)) !== digestHex(b.get(11)) ? 'recovery_state_changed' : null),
  (b) => (b.get(3) === 3 && b.get(4) ? 'recovery_reducing' : null),
  (b) => (b.get(3) !== 3 && !b.has(9) ? 'intent_hash_missing' : null),
  (b) => (b.get(3) !== 3 && digestHex(b.get(5)) === digestHex(b.get(11)) ? 'state_unchanged' : null),
  (b) => (b.get(3) === 2 && !b.get(4) ? 'direct_not_reducing' : null),
  (b) => (b.get(3) !== 3 && b.has(13) ? 'readded_outside_recovery' : null),
  (b) => (b.get(8) !== b.get(7) + 1 ? 'epoch_step' : null),
];

const bundleRules = [
  (b) => (map(b.get(4)).get(0) !== 1 ? 'author_not_account' : null),
  (b) => (map(b.get(6)).get(0) !== 1 ? 'recipient_not_account' : null),
  (b) => (principalKey(b.get(4)) === principalKey(b.get(6)) ? 'recipient_is_author' : null),
  (b) => {
    const seen = new Set();
    for (const raw of b.get(11)) {
      const it = map(raw); const id = hex(it.get(0));
      if (seen.has(id)) return 'item_id_duplicate';
      if (!it.has(4) && !it.has(5)) return 'item_empty';
      if (it.has(6) && !seen.has(hex(it.get(6)))) return 'reply_to_unknown_or_forward';
      seen.add(id);
    }
    return null;
  },
];

const first = (rules, ...args) => { for (const r of rules) { const why = r(...args); if (why) return why; } return null; };

const checkers = {
  state: (v) => first(stateRules, map(v)),
  intent: (v) => { const i = map(v); return first(intentRules, i, i.get(6), map(i.get(8))); },
  binding: (v) => first(bindingRules, map(v)),
  bundle: (v) => first(bundleRules, map(v)),
};

// Bundle admission: ordered gates, then structure, then idempotence. Nothing a bundle says reaches live state.
function admit(room, store, value, signer) {
  const b = map(value); const author = principalKey(b.get(4)); const recipient = principalKey(b.get(6));
  const st = room.states[digestHex(b.get(8))];
  const latest = room.states[room.latest_state];
  const gates = [
    ['signer_not_author_device', () => signer !== hex(b.get(5))],
    ['wrong_conversation', () => hex(b.get(3)) !== room.conversation_id],
    ['wrong_recipient', () => recipient !== room.local.account],
    ['wrong_recipient_incarnation', () => hex(b.get(7)) !== room.local.incarnation],
    ['unknown_state', () => st === undefined],
    ['epoch_mismatch', () => st.mls_epoch !== b.get(9)],
    ['author_not_member', () => !(author in st.members)],
    ['recipient_not_member', () => st.members[recipient] !== hex(b.get(7))],
    ['author_removed', () => latest.members[author] !== st.members[author]],
    ['recipient_removed', () => latest.members[recipient] !== hex(b.get(7))],
    ['author_device_ineligible', () => !(room.eligible_devices[author] ?? []).includes(hex(b.get(5)))],
    ['structure', () => first(bundleRules, b)],
  ];
  for (const [reason, failed] of gates) {
    const r = failed();
    if (r) return ['reject', reason === 'structure' ? first(bundleRules, b) : reason];
  }
  const id = hex(b.get(2)); const nonce = hex(b.get(10));
  if (store.ids.has(id) || store.nonces.has(nonce)) return ['duplicate', null];
  store.ids.add(id); store.nonces.add(nonce);
  for (const raw of b.get(11)) store.history.push({ ns: ['bundle', id, hex(map(raw).get(0))], label: `shared by ${author}` });
  return ['accept', null];
}

function scenario(c) {
  const store = { ids: new Set(), nonces: new Set(), history: [] };
  const live = new Set(c.room.live_events);
  const reducers = JSON.parse(JSON.stringify(c.room.reducers));
  const verdicts = [];
  for (const step of c.steps) {
    if (step.live) {
      const e = step.live;
      if (live.has(e.event_id)) { verdicts.push('duplicate_live'); continue; }
      live.add(e.event_id);
      if (e.kind === 'pin.set') reducers.pins[e.target] = e.present;
      verdicts.push('live');
    } else {
      const [v, why] = admit(c.room, store, step.import, step.signer);
      verdicts.push(why === null ? v : `${v}:${why}`);
    }
  }
  return { verdicts, history_items: store.history.length, bundles_imported: store.ids.size, live_events: [...live].sort(),
           reducers, labels: store.history.map((h) => h.label) };
}

const out = {};
for (const c of JSON.parse(fs.readFileSync(process.argv[2], 'utf8')).cases) {
  if (c.kind === 'scenario') { out[c.id] = scenario(c); continue; }
  const why = checkers[c.kind](c.value);
  out[c.id] = { verdict: why === null ? 'accept' : 'reject', reason: why };
}
const sortKeys = (x) => (Array.isArray(x) ? x.map(sortKeys) : x && typeof x === 'object' ? Object.fromEntries(Object.keys(x).sort().map((k) => [k, sortKeys(x[k])])) : x);
console.log(JSON.stringify(sortKeys(out), null, 1));
