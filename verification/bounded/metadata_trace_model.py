#!/usr/bin/env python3
"""V-015 trace model: W0 provider-visible flows checked against metadata descriptors.

Includes ADR-0138 provider-resource authentication, ADR-0142 mandatory sealed delivery for established 1:1 traffic,
and RecoveryRequest fan-out disclosure. Routine provider-admin capabilities do not expose DeviceCredential identity;
recovery fan-out separately exposes AccountID/recovery-attempt metadata to the participating mailbox Home. Colocated
Home-class roles are one observer for privacy paths.
"""
from __future__ import annotations
import itertools, json
from copy import deepcopy
from pathlib import Path
ROOT=Path(__file__).resolve().parents[2]
DESC=ROOT/'spec/metadata/w0-metadata-exposure-descriptors.json'
OUT=ROOT/'verification/results/metadata-trace-latest.json'
HOME_CLASS={"home_relay_mailbox","t0_sponsored_mailbox","t1_hosted_home","key_material_provider","blob_store","introgate","discovery_publisher","recovery_record_store"}

def msg(role,*fields,ip=True): return (role,set(fields)|({"client_network_address"} if ip else set()))

def flows(recipient_home:str,privacy_path:bool,multi_device:bool,push_on:bool):
    ip=not privacy_path
    env=("message_timing","ciphertext_size")
    send=[msg(recipient_home,*env,ip=ip),msg(recipient_home,*env,ip=True)]
    if multi_device: send.append(msg(recipient_home,"message_timing","ciphertext_size","device_count",ip=True))
    if push_on: send.append(("push_provider",{"push_timing","stable_device_correlator"}|({"device_count"} if multi_device else set())))
    if recipient_home=="t0_sponsored_mailbox": send=[(r,f|{"sponsor_relationship"}) if r==recipient_home else (r,f) for r,f in send]
    if recipient_home=="t1_hosted_home": send=[(r,f|{"membership_or_roster"}) if r==recipient_home else (r,f) for r,f in send]
    yield "send_1to1",send
    yield "first_contact",[msg("discovery_publisher","query_plaintext",ip=ip),msg("introgate","message_timing",ip=ip),msg("key_material_provider","contact_graph_edge","stable_device_correlator",ip=ip)]
    yield "descriptor_publish",[msg("discovery_publisher","query_plaintext",ip=ip)]
    yield "attachment",[msg("blob_store","blob_size",ip=ip),msg("blob_store","blob_size","message_timing",ip=True)]
    yield "blob_repair_cap",[msg("blob_store","blob_size",ip=ip)]
    yield "recovery_record_access",[msg("recovery_record_store","recovery_attempt",ip=ip),("platform_credential_manager",{"recovery_attempt"})]
    yield "recovery_fanout",[msg(recipient_home,"stable_account_id","recovery_attempt",ip=ip)]
    yield "pwa_load",[msg("clienthost_origin",ip=True)]
    yield "lan_rendezvous",[msg("lan_observer","lan_advertisement",ip=True)]

def evaluate(desc):
    homes=["home_relay_mailbox","t0_sponsored_mailbox","t1_hosted_home"]
    violations=[]; observed_any={r:set() for r in desc}; cases=0
    for home,privacy,multi,push in itertools.product(homes,(False,True),(False,True),(False,True)):
      for name,obs in flows(home,privacy,multi,push):
        cases+=1; per_role={}
        for role,fields in obs: per_role.setdefault(role,set()).update(fields)
        for role,fields in per_role.items():
          if role not in desc: violations.append(f"{name}: role {role} has no descriptor"); continue
          observed_any[role]|=fields
          for f in sorted(fields):
            if desc[role].get(f,"no_by_design")=="no_by_design": violations.append(f"under-claim: {role} observes {f} in {name} ({home}, privacy={privacy})")
          # Descriptor frequency is scoped to the operation family that can carry the field.
          # RecoveryRequest fan-out is a control-plane operation, not a 1:1 message envelope;
          # its explicit AccountID/recovery-attempt disclosure must not make envelope-only
          # `always` fields (sender IK / DR-header shape / message ciphertext metadata) mandatory.
          if not privacy and name != "recovery_fanout":
            for f,v in desc[role].items():
              if v=="always" and f not in fields and f!="client_network_address": violations.append(f"over-claim: {role} marks {f} always but {name} ({home}) does not reveal it")
          if "conversation_id" in fields: violations.append(f"§20 invariant: {role} sees ConversationID in {name}")
          if "stable_account_id" in fields and name != "recovery_fanout": violations.append(f"§20 invariant: {role} sees AccountID outside declared recovery fan-out in {name}")
        if privacy:
          # Same-operator Home colocation: joining any of its role logs cannot make the first privacy-path hop reveal IP.
          colocated=[(r,f) for r,f in obs if r in HOME_CLASS]
          if any("client_network_address" in f for r,f in colocated[:1]): violations.append(f"privacy path leaks sender address to colocated Home observer in {name}")
    unused=[f"{r}.{f}" for r,e in desc.items() for f,v in e.items() if v=="always" and f not in observed_any[r]]
    violations += [f"over-claim: {x} never observed in any flow" for x in unused]
    return cases,violations,observed_any

def main()->int:
    spec=json.loads(DESC.read_text()); desc={d['role']:d['exposures'] for d in spec['descriptors']}
    cases,violations,_=evaluate(desc)
    mutations={}
    # Recovery fan-out is the one W0 mailbox operation that intentionally exposes AccountID/recovery-attempt.
    for role in ("home_relay_mailbox","t0_sponsored_mailbox","t1_hosted_home"):
      d=deepcopy(desc); d[role]['stable_account_id']='no_by_design'; d[role]['recovery_attempt']='no_by_design'
      _,v,_=evaluate(d); mutations[role]=[x for x in v if role in x and ('stable_account_id' in x or 'recovery_attempt' in x)]
    # Provider-resource administration must not reintroduce DeviceCredential correlation.
    d=deepcopy(desc); d['discovery_publisher']['stable_device_correlator']='always'
    _,v,_=evaluate(d); mutations['provider_auth_no_device_id']=[x for x in v if 'stable_device_correlator' in x or 'over-claim' in x]
    # ADR-0142 regression: claiming clear sender/device or DR-header visibility at mailbox Homes must contradict the trace.
    d=deepcopy(desc)
    for role in ("home_relay_mailbox","t0_sponsored_mailbox","t1_hosted_home"):
      d[role]['stable_device_correlator']='always'; d[role]['ratchet_header_shape']='always'
    _,v,_=evaluate(d); mutations['adr0142_clear_sender_regression']=[x for x in v if 'stable_device_correlator' in x or 'ratchet_header_shape' in x or 'over-claim' in x]
    mutation_caught={r:bool(v) for r,v in mutations.items()}
    passed=not violations and all(mutation_caught.values())
    payload={'model':'metadata_trace_w0','gates':['V-015'],'cases':cases,'roles':sorted(desc),'violations':violations,'passed':passed,
             'security_mutations_caught':mutation_caught,'security_mutation_samples':{r:v[:2] for r,v in mutations.items()},
             'gate_status_effect':'evidence_only_no_gate_automatically_passed'}
    OUT.write_text(json.dumps(payload,indent=1)+'\n')
    print(json.dumps({'cases':cases,'roles':len(desc),'passed':passed,'violations':violations[:8],'security_mutations_caught':mutation_caught},indent=1))
    return 0 if passed else 1
if __name__=='__main__': raise SystemExit(main())
