#!/usr/bin/env python3
"""ADR-0149 V-005 registry/object-scope/fallback hardening evidence."""
from __future__ import annotations
from pathlib import Path
import json, subprocess, sys
ROOT=Path(__file__).resolve().parents[2]
OUT=ROOT/'verification/results/extensions-adr0149-hardening-latest.json'
REG=ROOT/'spec/registries/om-numeric-fields.json'
CBOR=ROOT/'spec/registries/om-cbor-profile.json'
VEC=ROOT/'verification/vectors/adr0148-v1-extension-vectors.json'

def main():
    r=json.loads(REG.read_text()); c=json.loads(CBOR.read_text()); vectors=json.loads(VEC.read_text())
    ex={x['name']:x for x in r['extensions']}
    structs=r['structs']
    expected={
      'om.voice-note.v1':('ConversationEvent',False,'VoiceNoteV1Extension'),
      'om.location-card.v1':('ConversationEvent',False,'LocationCardV1Extension'),
    }
    structural={}
    for name,(owner,critical,ref) in expected.items():
        e=ex.get(name,{})
        structural[f'{name}:registered']=bool(e)
        structural[f'{name}:owner']=e.get('object')==owner
        structural[f'{name}:criticality']=e.get('critical') is critical
        structural[f'{name}:schema_ref']=e.get('value')=={'ref':ref} and ref in structs
        structural[f'{name}:cbor_mirror']=name in c['extensions']['known_extensions']
    structural['registry_rule_enforces_owner_criticality_schema']=all(x in c['extensions']['rule'] for x in ['owning object','exact criticality','registered payload schema'])
    structural['voice_payload_integer_keys']= [f['key'] for f in structs['VoiceNoteV1Extension']['fields']]==[0,1]
    structural['location_payload_integer_keys']= [f['key'] for f in structs['LocationCardV1Extension']['fields']]==[0,1,2,3]

    # Re-run the independent Python/Node vector checker and inspect required adversarial cases.
    cp=subprocess.run([sys.executable,str(ROOT/'verification/bounded/extensions_adr0148.py')],cwd=ROOT,capture_output=True,text=True)
    base=json.loads((ROOT/'verification/results/extensions-adr0148-latest.json').read_text()) if cp.returncode==0 else {}
    ids={x['id']:x for x in vectors}
    required_ids=['voice-wrong-object','location-wrong-object','voice-critical-forbidden','location-critical-forbidden',
                  'voice-unknown-payload-key','location-mismatched-nonempty-fallback','unknown-noncritical','unknown-critical','future-critical-cleared-bit']
    adversarial={
      'base_cross_impl_passes':cp.returncode==0 and base.get('passed') is True,
      'all_required_vectors_present':all(x in ids for x in required_ids),
      'cross_impl_all_vectors_agree':base.get('python_node_agreement')==len(vectors),
      'mismatched_nonempty_fallback_is_not_object_rejection':ids['location-mismatched-nonempty-fallback']['expected_verdict']=='ignore_extension',
      'cross_object_misuse_ignores_extension_not_object':all(ids[x]['expected_verdict']=='ignore_extension' for x in ['voice-wrong-object','location-wrong-object']),
      'wrong_criticality_is_object_rejection':all(ids[x]['expected_verdict']=='reject_object' for x in ['voice-critical-forbidden','location-critical-forbidden']),
      'future_critical_prefix_is_registry_independent':ids['future-critical-cleared-bit']['critical'] is False and ids['future-critical-cleared-bit']['expected_verdict']=='reject_object' and ids['future-critical-cleared-bit']['expected_error']=='unknown_critical_extension',
    }
    passed=all(structural.values()) and all(adversarial.values())
    out={'kind':'adr0149_extension_registry_hardening','design_evidence_only':True,'structural':structural,'adversarial':adversarial,
         'vector_count':len(vectors),'passed':passed,
         'note':'Registry tuple + independent Python/Node amended ADR-0148 vector evidence. Under ADR-0155/0157 a wire-criticality mismatch fails the containing object, while object/schema/semantic failures of these non-critical registrations ignore only the extension.'}
    OUT.write_text(json.dumps(out,indent=2)+'\n');print(json.dumps(out,indent=2));return 0 if passed else 1
if __name__=='__main__':raise SystemExit(main())
