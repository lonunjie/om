#!/usr/bin/env python3
"""ADR-0138 design-closure Set 1 structural/bounded checks.

Design evidence only. No TLC/Rust/Wasm/product implementation is run or implied.
"""
from __future__ import annotations
import hashlib, hmac, json
from pathlib import Path
ROOT=Path(__file__).resolve().parents[2]
OUT=ROOT/'verification/results/design-closure-set1-latest.json'

def hkdf_sha256(ikm:bytes, info:bytes, length=32)->bytes:
    # RFC5869 with zero salt; enough to check domain separation/shape, not a production implementation claim.
    prk=hmac.new(b'\x00'*32,ikm,hashlib.sha256).digest()
    out=b''; t=b''; c=1
    while len(out)<length:
        t=hmac.new(prk,t+info+bytes([c]),hashlib.sha256).digest(); out+=t; c+=1
    return out[:length]

def check_queue()->list[str]:
    q={r['id']:r for r in map(json.loads,(ROOT/'machine/architecture_design_queue.jsonl').read_text().splitlines())}
    bad=[]
    for i in ('A-030','A-031','A-036','A-054'):
        if q[i].get('status')!='resolved' or not q[i].get('resolution'): bad.append(f'{i} not resolved with resolution')
    return bad

def check_legacy()->list[str]:
    text=(ROOT/'docs/architecture/identity/ACCOUNT_LIFECYCLE_RETIREMENT_AND_LEGACY.md').read_text()
    need=('om.acs.legacy_policy','verifier-local dormancy','LegacyRetirementPending','provider timestamp','quarantined parent','pre-designated successor')
    bad=[f'legacy design missing {x}' for x in need if x not in text]
    reg=json.loads((ROOT/'spec/registries/om-numeric-fields.json').read_text())
    ex=next((x for x in reg['extensions'] if x['name']=='om.acs.legacy_policy'),None)
    if not ex: bad.append('legacy extension not registered')
    elif ex.get('critical') is not True: bad.append('legacy commitment must be critical so unsupported verifiers reject the containing ACS (ADR-0141)')
    return bad

def check_privacy_and_prekey_freshness()->list[str]:
    a=(ROOT/'docs/architecture/crypto/ASYNC_KEY_MATERIAL.md').read_text()
    s=(ROOT/'docs/architecture/delivery/SEALED_SENDER_DELIVERY_PROFILE.md').read_text()
    g=(ROOT/'docs/architecture/groups/GROUP_CHAT_ARCHITECTURE.md').read_text()
    reg=json.loads((ROOT/'spec/registries/om-numeric-fields.json').read_text())
    ds=json.loads((ROOT/'spec/registries/om-domain-separation.json').read_text())
    akb=reg['structs']['AsyncKeyBundle']
    fields={x['key']:x['name'] for x in akb['fields']}
    reserved={x['key']:x for x in akb.get('reserved',[])}
    labels={x['label']:x for x in ds['labels']}
    bad=[]
    for x in ('relationship-pinned ACS','highest_generation','provider_pool_id','OM-PREKEY-GENERATION-CORE-v1','first-observation'):
        if x.lower() not in a.lower(): bad.append(f'ADR-0140 AsyncKeyBundle v1 semantics missing {x}')
    if 3 in fields or 4 in fields: bad.append('retired provider-visible ACS fields remain active in AsyncKeyBundle registry')
    for k in (3,4):
        if k not in reserved: bad.append(f'AsyncKeyBundle retired key {k} is not reserved')
    if 'OM-PREKEY-GENERATION-CORE-v1' not in labels: bad.append('generation-core domain label missing')
    item=labels.get('OM-PREKEY-SIG-v1',{}).get('construction','')
    for x in ('provider_pool_id','bundle_generation','subject_device_id'):
        if x not in item: bad.append(f'one-time-prekey signature construction missing {x}')
    # Still-later privacy features remain explicit; this run does not pull them forward.
    for x in ('entire existing 1:1 envelope','does not reveal sender AccountID','silent fallback'):
        if x not in s: bad.append(f'sealed sender target missing {x}')
    if 'do not promise cryptographic deniability' not in g: bad.append('group deniability non-goal not explicit')
    return bad

def check_provider_auth()->list[str]:
    text=(ROOT/'spec/protocol/home-api.md').read_text()
    bad=[]
    if '`device_auth`' in text: bad.append('device_auth remains in normative Home API')
    for cap in ('relay_admin_cap','directory_publish_cap','key_publish_cap','recovery_record_read_cap','recovery_record_write_cap','blob_repair_cap'):
        if cap not in text: bad.append(f'missing provider resource cap {cap}')
    if 'AccountID or DeviceCredential' not in text: bad.append('provider resource non-authority rule missing')
    return bad

def check_recovery_caps()->list[str]:
    root=bytes(range(32)); scope=bytes(range(32,64))
    read=hkdf_sha256(root,b'OM-RECOVERY-RECORD-READ-CAP-v1'+scope)
    write=hkdf_sha256(root,b'OM-RECOVERY-RECORD-WRITE-CAP-v1'+scope)
    read2=hkdf_sha256(root,b'OM-RECOVERY-RECORD-READ-CAP-v1'+bytes(reversed(scope)))
    loc=hashlib.sha256(b'OM-RECOVERY-RECORD-LOCATOR-v1'+read).digest()
    bad=[]
    if len({read,write,read2})!=3: bad.append('recovery capability domain/provider separation failed')
    if len(loc)!=32 or loc in (read,write): bad.append('recovery locator is not independent hash-shaped value')
    text=(ROOT/'spec/protocol/home-api.md').read_text()
    if 'GET /v1/recovery/records/{opaque_locator}' not in text: bad.append('RecoveryRecord GET missing')
    if '{account_ref}' in text: bad.append('RecoveryRecord still account-indexed in Home API')
    return bad

def check_metadata()->list[str]:
    d=json.loads((ROOT/'spec/metadata/w0-metadata-exposure-descriptors.json').read_text())
    by={x['role']:x['exposures'] for x in d['descriptors']}
    bad=[]
    for r in ('discovery_publisher','recovery_record_store','blob_store'):
        if by[r]['stable_device_correlator']!='no_by_design': bad.append(f'{r} still claims device-auth correlator')
    for r in ('home_relay_mailbox','t0_sponsored_mailbox','t1_hosted_home'):
        if by[r]['stable_account_id']!='sometimes' or by[r]['recovery_attempt']!='sometimes': bad.append(f'{r} missing recovery fanout disclosure')
    return bad

def main()->int:
    checks={
      'queue':check_queue(),'legacy':check_legacy(),'privacy_and_prekey_freshness':check_privacy_and_prekey_freshness(),
      'provider_auth':check_provider_auth(),'recovery_caps':check_recovery_caps(),'metadata':check_metadata(),
    }
    passed=all(not v for v in checks.values())
    payload={'model':'design_closure_set1','adr':'ADR-0138','passed':passed,'checks':checks,
             'scope_note':'design/specification structural evidence only; no TLC/Rust/Wasm/product build executed',
             'gate_status_effect':'evidence_only_no_gate_automatically_passed'}
    OUT.write_text(json.dumps(payload,indent=2,sort_keys=True)+'\n')
    print(json.dumps({'passed':passed,**{k:(not v) for k,v in checks.items()}},indent=2))
    return 0 if passed else 1
if __name__=='__main__': raise SystemExit(main())
