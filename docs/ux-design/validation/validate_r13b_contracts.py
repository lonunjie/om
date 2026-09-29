#!/usr/bin/env python3
import json
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
contract_path = ROOT / "contracts/om-live-groups-security-contract-v0.json"
d = json.loads(contract_path.read_text())
errors=[]
obj=d.get('objects',{})
actions=d.get('actions',[])
ids=[a.get('id') for a in actions]
if len(ids)!=len(set(ids)):
    errors.append('duplicate action ids')
required_objects={'call-session','call-participation','group','space','security-relationship','device-authorization','recovery-operation','provider-route'}
missing=required_objects-set(obj)
if missing: errors.append(f'missing objects: {sorted(missing)}')
required_invariants={
 'route_is_not_security_score',
 'incoming_video_does_not_activate_camera_before_user_action',
 'call_reconnect_preserves_logical_call',
 'space_is_optional',
 'automatic_crypto_check_is_not_real_world_identity',
 'manual_verification_change_requires_review',
 'provider_change_is_not_identity_change',
 'recovery_scopes_remain_distinct',
 'security_critical_actions_cannot_be_zero_query_auto_executed'
}
inv=set(d.get('invariants',[]))
miss=required_invariants-inv
if miss: errors.append(f'missing invariants: {sorted(miss)}')
for aid in ['revoke-device','start-recovery','review-verification-change','add-device']:
    a=next((x for x in actions if x.get('id')==aid),None)
    if not a: errors.append(f'missing action {aid}')
    elif a.get('risk')!='security-critical': errors.append(f'{aid} must be security-critical')
if obj.get('call-session',{}).get('route_is_security_score') is not False:
    errors.append('call route must not be a security score')
if obj.get('space',{}).get('optional') is not True:
    errors.append('Space must remain optional')
if obj.get('security-relationship',{}).get('automatic_crypto_check_is_real_world_identity') is not False:
    errors.append('automatic crypto check must not claim real-world identity')
if obj.get('device-authorization',{}).get('revocation_implies_remote_data_erasure') is not False:
    errors.append('device revocation must not imply remote erase')
if obj.get('recovery-operation',{}).get('generic_recover_all_claim_allowed') is not False:
    errors.append('recovery scopes must remain distinct')
if errors:
    print('R13B CONTRACT: FAIL')
    for e in errors: print('-',e)
    raise SystemExit(1)
print(f'R13B CONTRACT: PASS — {len(obj)} semantic objects, {len(actions)} actions, {len(inv)} invariants')
