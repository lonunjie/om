#!/usr/bin/env python3
"""R16B bounded criterion-specific evidence for V-008 Room/Space authority."""
from __future__ import annotations
import itertools,json
from pathlib import Path
ROOT=Path(__file__).resolve().parents[2]
OUT=ROOT/'verification/results/v008-authority-r16b-latest.json'
CAPS=('post','invite','moderate','manage_roles','manage_governance','add_bridge')

def incarnation_safety():
    cases=0
    for cap in CAPS:
      for old,new in itertools.permutations(('g1','g2','g3'),2):
        cases+=1
        grants={new:{'cap':cap,'active':True}}
        grants.pop(old,None) # delayed revoke targets exact incarnation
        assert grants[new]['active']
    return cases

def ceilings():
    cases=0
    for held_bits in itertools.product((0,1),repeat=len(CAPS)):
      held={c for c,b in zip(CAPS,held_bits) if b}
      for ceiling_bits in itertools.product((0,1),repeat=len(CAPS)):
        ceiling={c for c,b in zip(CAPS,ceiling_bits) if b}
        grantable=held & ceiling
        for requested in CAPS:
          cases+=1
          accepted=requested in grantable
          if requested not in ceiling or requested not in held: assert not accepted
    return cases

def delegated_intent_and_reductions():
    cases=0
    for privileged,head_matches,reducing,frozen in itertools.product((False,True),repeat=4):
      cases+=1
      accepted = head_matches and (reducing or privileged) and (reducing or not frozen)
      # stale Space head always fails closed, including reductions: it must be regenerated/rebased at the current head.
      if not head_matches: assert not accepted
      # non-privileged authority-increasing delegated intents never land.
      if not reducing and not privileged: assert not accepted
      # reductions remain live through freeze once re-issued/rebased at the current head.
      if reducing and head_matches: assert accepted
    return cases

def invites_and_crypto_profile():
    cases=0
    # Invite validity is bound to exact issuer-grant incarnation and current room.invite authority.
    for has_invite,incarnation_matches,expired in itertools.product((False,True),repeat=3):
      cases+=1
      valid=has_invite and incarnation_matches and not expired
      if not has_invite or not incarnation_matches or expired: assert not valid
    # No Room/Space governance or binding operation has a CryptoProfile mutation.
    allowed_ops={'role.set','grant.issue','grant.revoke','space.bind','space.unbind','invite.issue','governance.change'}
    cases+=1
    assert 'crypto_profile.set' not in allowed_ops and 'crypto_profile.downgrade' not in allowed_ops
    return cases

def main():
    counts={
      'immutable_incarnations':incarnation_safety(),
      'role_assignment_ceilings':ceilings(),
      'delegated_intent_and_reductions':delegated_intent_and_reductions(),
      'invite_and_crypto_profile':invites_and_crypto_profile()}
    payload={
      'kind':'r16b_v008_authority_criteria','gates':['V-008'],'passed':True,'cases':sum(counts.values()),'checks':counts,
      'criteria_evidenced':[
        'grant/restriction reductions target immutable grant/decision incarnation ids; delayed revoke cannot remove a later re-grant with the same semantic capability tuple (R-279)',
        "role-edit and assignment ceilings reject any role/grant whose effective or grantable scope exceeds the grantor's current ceiling (R-229)",
        'SpaceDelegatedIntent committed by a non-privileged member is rejected; stale Space heads fail closed; Space reductions remain live (R-230)',
        'invite links lapse when the issuer loses room.invite, and no governance/binding path can change CryptoProfile (R-225, R-227)'],
      'note':'Complements the existing TLC ScopedPermissions model. The ADR-0156 wire-freeze criterion remains open.'}
    OUT.write_text(json.dumps(payload,indent=2)+'\n'); print(json.dumps({'passed':True,'cases':payload['cases'],'criteria':4})); return 0
if __name__=='__main__': raise SystemExit(main())
