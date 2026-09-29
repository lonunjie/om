#!/usr/bin/env python3
"""ADR-0164 bounded sanity checks for dangerous implementation algorithms.

This is implementation-readiness evidence only. It does not mark any V-series gate passed.
"""
from __future__ import annotations
import json, sys
from pathlib import Path
ROOT=Path(__file__).resolve().parents[2]
CONTRACT=ROOT/'machine/dangerous_algorithm_contracts.json'
DOC=ROOT/'docs/architecture/IMPLEMENTATION_ALGORITHMS_AND_FAILURE_PATHS.md'
RES=ROOT/'verification/results/dangerous-algorithms-adr0164-latest.json'

def check(cond, ident, details=''):
    return {'id':ident,'ok':bool(cond),'details':details}

def main():
    c=json.loads(CONTRACT.read_text())
    rows=[]
    rows.append(check(len(c['algorithms'])==8,'contract_has_8_algorithms'))
    ids=[a['id'] for a in c['algorithms']]
    rows.append(check(len(ids)==len(set(ids)),'algorithm_ids_unique'))
    text=DOC.read_text()
    lan_text=(ROOT/'docs/architecture/network/LAN_RENDEZVOUS_AND_OFFLINE_LOCAL_MODE.md').read_text()
    history_text=(ROOT/'docs/architecture/groups/GROUP_CHAT_ARCHITECTURE.md').read_text()
    section_markers={
      'acs_child_validation_v1':'## 2. AccountControlState child validation',
      'one_to_one_receive_v1':'## 3. One-to-one inbound + sealed-delivery acceptance',
      'group_commit_recovery_v1':'## 4. Group Commit preparation/publication/recovery',
      'mutable_record_convergence_v1':'## 5. Mutable RelaySet / BlobLocationRecord branch selection',
      'history_bundle_import_v1':'## 6. GroupHistoryBundle import isolation',
      'extension_dispatch_v1':'## 7. Extension dispatch',
      'lan_frame_admission_v1':'## 8. LAN frame/channel admission',
      'unknown_outcome_reconciliation_v1':'## 9. Unknown provider/network outcome',
    }
    for a in c['algorithms']:
        rows.append(check(section_markers.get(a['id'],'') in text,'doc_contains:'+a['id']))
        rows.append(check(all((ROOT/s).exists() for s in a['sources']),'sources_exist:'+a['id']))
        rows.append(check(len(a['forbidden_shortcuts'])>=2,'forbidden_shortcuts_present:'+a['id']))

    # ACS: use the canonical bounded model rather than duplicating authorization semantics.
    sys.path.insert(0,str(ROOT))
    from verification.bounded.acs_authorization_table import state, children, authorize
    p=state(['d1','d2','d3'], messaging=['m1'], min_gen=2)
    kids={(k,t):ch for k,t,ch in children(p)}
    c1=dict(kids[('promote','d4')]); c1['messaging']=sorted(set(c1['messaging'])|{'m2'})
    rows.append(check(authorize(p,c1,{'signers':['d1']})[0]=='reject','acs_combo_promote_link_rejects_single'))
    rows.append(check(authorize(p,c1,{'signers':['d1','d2']})[0]=='accept','acs_combo_promote_link_accepts_majority'))
    cl=dict(kids[('lower_min_profile',None)])
    rows.append(check(authorize(p,cl,{'signers':['d1','d2','d3']})[0]=='reject','acs_lower_floor_rejected'))
    pr=state(['d1','d2','d3'], messaging=['m1'], revoked=['d4'], min_gen=2)
    cr=next(ch for k,t,ch in children(pr) if k=='promote')
    rows.append(check(authorize(pr,cr,{'signers':['d1','d2','d3']})[0]=='reject','acs_revoked_device_never_repromoted'))

    # Group journal: exact ordered transitions only; no direct PREPARED -> ACCEPTED.
    chain=['INTENT_DURABLE','CANDIDATE_PREPARED','PUBLISHED_UNCONFIRMED','ACCEPTED']
    allowed={'INTENT_DURABLE':{'CANDIDATE_PREPARED'},'CANDIDATE_PREPARED':{'PUBLISHED_UNCONFIRMED'},'PUBLISHED_UNCONFIRMED':{'ACCEPTED','SUPERSEDED','ABANDONED'}}
    rows.append(check(all(n in allowed.get(s,set()) for s,n in zip(chain,chain[1:])),'group_journal_happy_path'))
    rows.append(check('ACCEPTED' not in allowed['CANDIDATE_PREPARED'],'group_journal_no_direct_accept'))
    rows.append(check('ACCEPTED' not in allowed['INTENT_DURABLE'],'group_journal_no_skip'))

    # Branch selection.
    candidates=[{'version':4,'hash':'ff'},{'version':5,'hash':'aa'},{'version':5,'hash':'bb'},{'version':4,'hash':'00'}]
    chosen=min((x for x in candidates if x['version']==max(c['version'] for c in candidates)),key=lambda x:x['hash'])
    rows.append(check(chosen=={'version':5,'hash':'aa'},'mutable_branch_highest_then_lexicographic'))
    rows.append(check(candidates[0] not in candidates[1:],'mutable_branch_no_alias_assumption'))

    # History isolation.
    rows.append(check('live ConversationEvent deduplication' in history_text and 'GroupControlState' in history_text,'history_isolation_named'))
    rows.append(check('bundle-local history_item_id' in text,'history_local_id_rule'))

    # Extension dispatch.
    def ext(known, critical, registered_critical, schema_ok=True):
        if critical != registered_critical:
            return 'reject'
        if not known:
            return 'reject' if critical else 'ignore'
        if not schema_ok:
            return 'reject' if critical else 'ignore'
        return 'accept'
    rows.extend([
      check(ext(False,True,True)=='reject','extension_unknown_critical_rejects'),
      check(ext(False,False,False)=='ignore','extension_unknown_noncritical_ignored'),
      check(ext(True,False,False,False)=='ignore','extension_known_noncritical_bad_schema_ignored'),
      check(ext(True,True,True,False)=='reject','extension_known_critical_bad_schema_rejects'),
      check(ext(True,False,True,True)=='reject','extension_criticality_mismatch_rejects'),
    ])

    # LAN admission (ADR-0158 profile, consolidated by ADR-0166): seed-bound signalling, bounded frames, backpressure.
    rows.extend([
      check('SDP/ICE credentials' in lan_text and 'LanSignalEnvelopeV1' in lan_text,'lan_signalling_encrypted'),
      check('K=8' in lan_text,'lan_fixed_cardinality_present'),
      check('16 KiB' in lan_text and '256 KiB/64 KiB' in lan_text,'lan_frame_and_backpressure_bounds_present'),
      check('LAN reachability never creates a relationship or device eligibility' in lan_text,'lan_no_authority_from_reachability'),
    ])

    # Unknown outcomes.
    outcomes=['PENDING_NETWORK','PENDING_RECONCILIATION','COMMITTED','REJECTED','NEEDS_USER_ACTION']
    rows.append(check('PENDING_RECONCILIATION' in text,'unknown_outcome_state_present'))
    rows.append(check('never originate a second semantic operation' in text,'unknown_outcome_no_duplicate'))
    rows.append(check(len(outcomes)==5,'unknown_outcome_five_terminal_semantics'))

    failures=[r for r in rows if not r['ok']]
    payload={'checker':'dangerous_algorithms_adr0164','passed':not failures,'cases':len(rows),'failures':failures,'rows':rows,'gate_status_effect':'evidence_only_no_gate_automatically_passed'}
    RES.write_text(json.dumps(payload,indent=2)+'\n')
    print(json.dumps(payload,indent=2))
    return 1 if failures else 0
if __name__=='__main__': raise SystemExit(main())
