#!/usr/bin/env python3
from pathlib import Path
import json, sys
root=Path(__file__).resolve().parents[1]
contract=json.loads((root/'contracts/om-messaging-contract-v0.json').read_text())
errors=[]
objs=contract.get('objects',{})
for required in ['draft','message','attachment','voice-note','scheduled-message','notification']:
    if required not in objs: errors.append(f'missing object {required}')
msg=objs.get('message',{})
for state in ['queued','failed','retrying','delivered','read']:
    if state not in msg.get('states',[]): errors.append(f'missing message state {state}')
if not msg.get('receipt_states_require_domain_evidence'): errors.append('receipt states must require domain evidence')
if not msg.get('retry_preserves_logical_id'): errors.append('retry must preserve logical message identity')
if objs.get('scheduled-message',{}).get('is_sent_state') is not False: errors.append('scheduled must not equal sent')
if objs.get('voice-note',{}).get('remote_transcription_default') is not False: errors.append('remote transcription must not default on')
inv=set(contract.get('invariants',[]))
for x in ['scheduled_is_not_sent','queued_is_not_failed','accepted_local_transport_is_not_delivered','background_sync_is_optional_optimization_not_correctness_dependency','voice_recording_and_omnibar_voice_are_distinct']:
    if x not in inv: errors.append(f'missing invariant {x}')
actions={a['id']:a for a in contract.get('actions',[])}
for x in ['send-message','retry-message','retry-attachment','record-voice-note','schedule-message','notification-reply']:
    if x not in actions: errors.append(f'missing action {x}')
if errors:
    print('FAIL')
    for e in errors: print('-',e)
    sys.exit(1)
print(f"PASS: messaging contract {contract['schema']} with {len(objs)} objects and {len(actions)} actions")
