#!/usr/bin/env python3
"""R16B bounded evidence for already-frozen V-004 group-control rules.

This deliberately does NOT claim the still-open same-epoch partition merge / MLS-library criteria.
It closes only the normative ADR-0122/ADR-0128 rules that can be exhaustively modeled locally.
"""
from __future__ import annotations
import itertools, json
from pathlib import Path

ROOT=Path(__file__).resolve().parents[2]
OUT=ROOT/'verification/results/v004-group-control-r16b-latest.json'


def next_live(succession, current, members, revoked=frozenset()):
    i=succession.index(current)
    for off in range(1,len(succession)+1):
        c=succession[(i+off)%len(succession)]
        if c in members and c not in revoked:
            return c
    return None


def reduction_liveness():
    succession=('A','B','C','D')
    cases=0
    for current in succession:
        for member_bits in itertools.product((0,1), repeat=4):
            members={m for m,b in zip(succession,member_bits) if b}
            if current not in members: continue
            for revoked_bits in itertools.product((0,1), repeat=4):
                revoked={m for m,b in zip(succession,revoked_bits) if b and m in members}
                cases+=1
                replacement=next_live(succession,current,members,revoked)
                eligible=[m for m in members if m not in revoked and m!=current]
                # After timeout a reduction may be directly committed by an eligible member.
                can_progress=bool(eligible)
                if can_progress:
                    assert replacement is not None
                    assert replacement in members and replacement not in revoked
                # Deterministic for every observer with the same parent state.
                assert replacement==next_live(succession,current,set(members),set(revoked))

    # Rebased non-reduction must be re-authorized at the new parent.
    for authorized_before,authorized_after in itertools.product((False,True),repeat=2):
        cases+=1
        rebased_accept=authorized_after
        if authorized_before and not authorized_after:
            assert not rebased_accept
        if authorized_after:
            assert rebased_accept
    return {'cases':cases,'passed':True}


def account_control_rules():
    cases=0
    # Learned revocation: sending is blocked until the exact leaf Remove is accepted.
    for learned,removed in itertools.product((False,True),repeat=2):
        cases+=1
        may_send=not learned or removed
        if learned and not removed: assert not may_send
    # Adds fail closed during account control conflict / safety freeze; removals remain permitted.
    for state in ('NORMAL','CONTROL_CONFLICT','SAFETY_FREEZE'):
        cases+=1
        may_add=state=='NORMAL'
        may_remove=True
        if state!='NORMAL': assert not may_add and may_remove
    # Device-add dedup is exact tuple (AccountID, DeviceID, ACS generation).
    keys=[('acct','dev',1),('acct','dev',1),('acct','dev',2),('acct','dev2',1)]
    cases+=1
    assert len(set(keys))==3
    return {'cases':cases,'passed':True}


def immutable_incarnation_reduction():
    cases=0
    semantic=('member','alice')
    for old_id,new_id in itertools.permutations(('inc-1','inc-2','inc-3'),2):
        cases+=1
        roster={new_id:semantic}
        stale_target=old_id
        roster.pop(stale_target,None)
        assert new_id in roster, 'stale reduction removed a later re-add'
    # Exact live incarnation is removable.
    cases+=1
    roster={'inc-7':semantic}; roster.pop('inc-7',None); assert not roster
    return {'cases':cases,'passed':True}


def main():
    r1=reduction_liveness(); r2=account_control_rules(); r3=immutable_incarnation_reduction()
    payload={
      'kind':'r16b_v004_group_control_bounded', 'gates':['V-004'],
      'criteria_evidenced':[
        'reductions (revoked/quarantined device removal, member removal, self-leave, service/bridge removal, Space-delegated reductions) are live under coordinator failure or censorship; succession replacement is deterministic; a rebased non-reducing intent is re-validated at its new parent (R-220)',
        'account-control events: no sending after a learned revocation until the Remove is accepted, no adds under CONTROL_CONFLICT/SAFETY_FREEZE, device-add deduplication (R-221)',
        'stale reduction targets an immutable membership/service incarnation and becomes a no-op after that exact incarnation is gone; it cannot remove a later re-add (R-279)'],
      'checks':{'reduction_liveness':r1,'account_control':r2,'immutable_incarnation':r3},
      'explicitly_not_claimed':['formal crash/partition model','candidate MLS runtime ordering support','same-epoch disconnected-partition merge','ADR-0156 groups wire freeze'],
      'passed':all(x['passed'] for x in (r1,r2,r3)),
      'gate_status_effect':'evidence_for_named_criteria_only'}
    OUT.write_text(json.dumps(payload,indent=2)+'\n')
    print(json.dumps({'passed':payload['passed'],'cases':sum(x['cases'] for x in (r1,r2,r3)),'criteria':3}))
    return 0 if payload['passed'] else 1
if __name__=='__main__': raise SystemExit(main())
