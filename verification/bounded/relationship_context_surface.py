#!/usr/bin/env python3
"""ADR-0137 / F-11: structural non-bypass check for inbound 1:1 relationship binding.

The Rust prototype is intentionally inspected as source because RelationshipContext is an API-shape invariant:
every public Device receive/decrypt boundary must require it, verify_initiator must execute before session lookup
or initial-handshake work, and persisted peers must rebuild the binding index. Named behavioral selftests cover
new-session, established-session and reload negatives.  In environments without a Rust toolchain this still gives
executable structural evidence; the Rust selftest remains the dynamic implementation evidence when Cargo is present.
"""
from __future__ import annotations
import json
import re
from pathlib import Path

ROOT=Path(__file__).resolve().parents[2]
SRC=ROOT/'verification/prototypes/v001-crypto/rust/core/src'
OUT=ROOT/'verification/results/relationship-context-surface-latest.json'

REQUIRED_TESTS={
 'initiator_unknown_identity_key_rejected_nothing_consumed',
 'initiator_revoked_device_rejected',
 'initiator_quarantined_device_rejected',
 'initiator_stale_credential_rejected_after_rotation',
 'initiator_ambiguous_same_account_identity_key_rejected',
 'initiator_pinned_to_relationship_route',
 'initiator_established_session_wrong_route_rejected_state_unchanged',
 'initiator_reloaded_established_session_wrong_route_rejected_before_commit',
 'initiator_reloaded_established_session_correct_route_accepted_and_committed',
}

def inspect(store:str,binding:str,otk:str)->list[str]:
    bad=[]
    # Public Device ingress API: both methods require a non-optional borrowed RelationshipContext.
    for name in ('receive','decrypt'):
        m=re.search(rf'pub fn {name}<[^>]+>\s*\((.*?)\)\s*->',store,re.S)
        if not m: bad.append(f'missing public Device::{name}'); continue
        sig=m.group(1)
        if 'relationship: &RelationshipContext' not in sig: bad.append(f'Device::{name} lacks mandatory &RelationshipContext')
        if 'Option<RelationshipContext' in sig or 'Option<&RelationshipContext' in sig: bad.append(f'Device::{name} makes relationship optional')
    if re.search(r'pub fn (?:decrypt|receive)_on_route',store): bad.append('legacy route helper remains public')
    # Binding must occur before session lookup / accept_initial, so a wrong route cannot touch ratchet/prekey state.
    body=store.split('pub fn decrypt<',1)[1].split('\n    /// Bob:',1)[0]
    vi=body.find('binding::verify_initiator')
    for marker in ('self.sessions.get_mut', 'self.accept_initial'):
        i=body.find(marker)
        if vi < 0 or i < 0 or vi > i: bad.append(f'verify_initiator does not precede {marker}')
    # RelationshipContext has exactly the trusted peer AccountID and no optional/default escape hatch.
    rc=binding.split('pub struct RelationshipContext',1)[1].split('}',1)[0]
    if 'peer_account_id: [u8; 32]' not in rc or 'Option<' in rc: bad.append('RelationshipContext peer AccountID is absent or optional')
    # Resolver is exact-one, account-scoped and active-only.
    for token in ('*a == relationship.peer_account_id','[] => Err(Error::Binding','[(_, a, d, true)]','_ => Err(Error::Binding("initiator IK_A matches more than one credential")'):
        if token not in binding: bad.append(f'verify_initiator missing fail-closed predicate: {token}')
    # Persist/reload must retain trusted peer heads and rebuild derived device lookup.
    if '16,' not in store or 'self.peers' not in store: bad.append('trusted peer relationship state is not serialized')
    if '.reindexed()' not in store or 'self.reindex_peers()?' not in store: bad.append('reload does not rebuild peer-device index')
    for t in REQUIRED_TESTS:
        if f'"{t}"' not in otk: bad.append(f'missing behavioral selftest {t}')
    return bad

def main()->int:
    store=(SRC/'store.rs').read_text(); binding=(SRC/'binding.rs').read_text(); otk=(SRC/'otk.rs').read_text()
    problems=inspect(store,binding,otk)
    mutations={
      'optional_relationship': inspect(store.replace('relationship: &RelationshipContext','relationship: Option<&RelationshipContext>',1),binding,otk),
      'binding_after_session_lookup': inspect(store.replace('binding::verify_initiator(&sender_ik, &self.peer_devices, relationship)?;\n        match (self.sessions.get_mut(sender), init) {','match (self.sessions.get_mut(sender), init) {\n        binding::verify_initiator(&sender_ik, &self.peer_devices, relationship)?;',1),binding,otk),
      'missing_reload_test': inspect(store,binding,otk.replace('initiator_reloaded_established_session_wrong_route_rejected_before_commit','mutation_removed_test')),
    }
    mutation_caught={k:bool(v) for k,v in mutations.items()}
    ok=not problems and all(mutation_caught.values())
    payload={'gate':'V-001','invariants':['I2','I3','I4','I5'],'passed':ok,'problems':problems,
             'public_inbound_entry_points':['Device::receive','Device::decrypt'],
             'required_behavioral_selftests':sorted(REQUIRED_TESTS),'mutations_caught':mutation_caught,
             'mutation_problem_samples':{k:v[:2] for k,v in mutations.items()},
             'dynamic_rust_execution':'not performed by this script; run the v001 Rust selftest when Cargo/Rust are available',
             'gate_status_effect':'evidence_only_no_gate_automatically_passed'}
    OUT.write_text(json.dumps(payload,indent=2,sort_keys=True)+'\n')
    print(json.dumps({'passed':ok,'problems':problems,'mutations_caught':mutation_caught}))
    return 0 if ok else 1
if __name__=='__main__': raise SystemExit(main())
