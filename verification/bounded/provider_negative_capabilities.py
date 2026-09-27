#!/usr/bin/env python3
"""ADR-0137 C1-C6: provider negative-capability conformance probes.

These checks do *not* claim a malicious operator cannot run arbitrary extra software. They verify the canonical OM
role interfaces/credentials do not themselves supply end-user ACS signing authority, E2EE content keys, referral
issuer private keys, or equivalent authority; role colocation is only a union of observations/service methods.
"""
from __future__ import annotations
import json, re, sys
from copy import deepcopy
from pathlib import Path
ROOT=Path(__file__).resolve().parents[2]
sys.path.insert(0,str(ROOT/'verification/bounded'))
from acs_authorization_table import authorize,state  # noqa:E402
import privacy_pass_issuance as ppi  # noqa:E402
from abuse_w0_rules import IntroGate  # noqa:E402

OUT=ROOT/'verification/results/provider-negative-capabilities-latest.json'
DOC=ROOT/'docs/architecture/providers/IMPLEMENTATION_INTERFACES.md'
REG=ROOT/'spec/registries/om-object-types.json'

FORBIDDEN=(r'acs.*sign.*(?:key|private)',r'controller.*(?:key|private)',r'e2ee.*(?:content.*key|private)',
           r'content.*key',r'issuer.*private',r'recovery.*root.*(?:key|private)',r'device.*sign.*private')
ROLES=('RelayProvider','BlobStore','DirectoryProvider','IntroGate','RecoveryStore','StoragePool','PushProvider / PushGateway')

def role_block(text:str,role:str)->str:
    # Canonical conceptual interfaces use either `Role` or `trait Role` followed by indented methods until blank.
    keys=[role]
    if ' / ' in role: keys += role.split(' / ')
    for key in keys:
        m=re.search(rf'(?m)^(?:trait\s+)?{re.escape(key)}(?:\s*\{{)?\s*$([\s\S]*?)(?:\n\s*\n|\n\}})',text)
        if m: return m.group(0)
    return ''

def interface_surface(text:str)->list[str]:
    bad=[]
    for role in ROLES:
        b=role_block(text,role)
        if not b:
            bad.append(f'canonical interface block missing for {role}'); continue
        for pat in FORBIDDEN:
            if re.search(pat,b,re.I): bad.append(f'{role} surface contains forbidden key/authority material matching {pat}')
    return bad

def contract_presence(text:str)->list[str]:
    bad=[]
    required={
      'RelayProvider / mailbox Home':('decrypt E2EE payloads','sign AccountControlState transitions'),
      'DirectoryProvider':('authorize controllers/devices','sign as the discovered account'),
      'RecoveryStore':('independently authorize an AccountControlState transition','controller authority'),
      'IntroGate':("mint the recipient's issuer private-key output",'decrypt sealed MessageRequest plaintext'),
      'BlobStore / `StoragePool`':('obtain blob content keys','AccountID/ConversationID'),
      'PushProvider / gateway':('message plaintext/content keys','conversation authority'),
    }
    for row,phrases in required.items():
        line=next((x for x in text.splitlines() if x.startswith('|') and row.replace('`','') in x.replace('`','')),None)
        if not line: bad.append(f'negative-capability contract row missing: {row}'); continue
        for ph in phrases:
            if ph not in line: bad.append(f'{row} contract missing: {ph}')
    return bad

def recovery_transport_not_constitutional_vote(mutate=False)->list[str]:
    p=state(['d1','d2','d3'],gen=1)
    c=state(['d1','d2','d3','d4'],keys={'d1':1,'d2':1,'d3':1,'d4':1},gen=2)
    # `recovery_auth` in RecoveryStore is a service/fanout credential. It is not RecoveryPolicy constitutional vote.
    ctx={'signers':[],'recovery_store_credential':True}
    if mutate:
        ctx['recovery']={'elapsed':2,'delay':2,'veto':False}  # illegal authority laundering mutation
    verdict,_=authorize(p,c,ctx)
    return [f'RecoveryStore transport credential became ACS/recovery authority ({verdict})'] if verdict=='accept' else []

def discovery_not_account_authority(mutate=False)->list[str]:
    reg=json.loads(REG.read_text())['types']
    by={x['name']:x for x in reg}
    dev=deepcopy(by['DeviceCredential']); acs=deepcopy(by['AccountControlState']); desc=deepcopy(by['ContactDescriptor'])
    if mutate:
        desc['type_id']=dev['type_id']; desc['signed']['label']=dev['signed']['label']
    bad=[]
    if len({dev['type_id'],acs['type_id'],desc['type_id']})!=3: bad.append('ContactDescriptor aliases an ACS/DeviceCredential type id')
    labels={dev['signed']['label'],acs['signed']['label'],desc['signed']['label']}
    if len(labels)!=3: bad.append('ContactDescriptor reuses an ACS/DeviceCredential signature domain')
    if desc['signed']['roles']!=['intro_identity']: bad.append('ContactDescriptor signature role is not intro_identity-only')
    return bad

def introgate_public_material_cannot_mint(mutate=False)->list[str]:
    issuer=ppi._fixed_issuer(); gate=IntroGate(overlap=3); gate.rotate(issuer.spki,1,now=0)
    kid=issuer.key_id
    if mutate:
        # Negative control: accidentally supplying the private issuer key to IntroGate must be detected.
        pk,epoch,ret=gate.keys[kid]; gate.keys[kid]=(issuer.sk,epoch,ret)
    bad=[]
    if any(hasattr(v[0],'sign') for v in gate.keys.values()): bad.append('IntroGate key surface contains signing/private-key capability')
    if not mutate:
        ch=b'negative-capability-introgate'
        forged=b'\x00\x02'+b'\x22'*32+__import__('hashlib').sha256(ch).digest()+kid+b'\x00'*256
        if gate.admit(forged,ch,0)!='reject:bad_signature': bad.append('IntroGate public verification material minted/admitted a forged token')
    return bad

def colocation_is_union_only(mutate=False)->list[str]:
    # Role-local service powers named by the canonical interfaces. None is an AccountControl signing capability.
    powers={
      'mailbox':{'put','subscribe','ack','rotate'},
      'directory':{'resolve','search','publish','revoke','transparency_proof'},
      'recovery_store':{'put_recovery_record','fanout_recovery_request','drain_recovery_requests'},
    }
    joined=set().union(*powers.values())
    if mutate: joined.add('sign_account_control_state')
    forbidden={'sign_account_control_state','decrypt_e2ee_content','mint_referral_issuer_token'}
    got=sorted(joined & forbidden)
    return [f'role colocation synthesized cryptographic authority: {got}'] if got else []

def main()->int:
    text=DOC.read_text()
    checks={
      'negative_contracts_present':(contract_presence(text),contract_presence(text.replace('sign AccountControlState transitions','observe AccountControlState transitions',1))),
      'provider_interface_key_surface':(interface_surface(text),interface_surface(text.replace('fn ack(&self, token: DeliveryToken) -> Result<()>;','fn ack(&self, token: DeliveryToken, acs_signing_private_key: ControllerPrivateKey) -> Result<()>;',1))),
      'recovery_transport_not_constitutional_vote':(recovery_transport_not_constitutional_vote(),recovery_transport_not_constitutional_vote(True)),
      'directory_alias_not_device_or_acs_authority':(discovery_not_account_authority(),discovery_not_account_authority(True)),
      'introgate_public_material_cannot_mint':(introgate_public_material_cannot_mint(),introgate_public_material_cannot_mint(True)),
      'colocation_union_only':(colocation_is_union_only(),colocation_is_union_only(True)),
    }
    detail={k:{'problems':a,'mutation_caught':bool(b),'mutation_problems':b[:3]} for k,(a,b) in checks.items()}
    passed=all(not x['problems'] and x['mutation_caught'] for x in detail.values())
    payload={'invariants':['C1','C2','C3','C4','C5','C6'],'passed':passed,'checks':detail,
             'scope_note':'Conformance of canonical role interfaces/credentials, not a claim about arbitrary code run by a malicious host.',
             'gate_status_effect':'evidence_only_no_gate_automatically_passed'}
    OUT.write_text(json.dumps(payload,indent=2,sort_keys=True)+'\n')
    print(json.dumps({'passed':passed,**{k:(not v['problems'],v['mutation_caught']) for k,v in detail.items()}}))
    return 0 if passed else 1
if __name__=='__main__': raise SystemExit(main())
