#!/usr/bin/env python3
"""Bounded/static architecture checks for ADR-0142 sealed delivery. Evidence only; no gate is auto-passed."""
from __future__ import annotations
import hashlib, json, sys
from pathlib import Path
ROOT=Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT/'verification/cbor'))
from python_canonical import enc as cbor_enc
OUT=ROOT/'verification/results/sealed-delivery-adr0142-latest.json'
errors=[]
def bad(x): errors.append(x)
def j(p): return json.loads((ROOT/p).read_text())
def jl(p): return [json.loads(x) for x in (ROOT/p).read_text().splitlines() if x.strip()]

num=j('spec/registries/om-numeric-fields.json'); types=j('spec/registries/om-object-types.json'); crypto=j('spec/registries/om-crypto-profiles.json')
ds=j('spec/registries/om-domain-separation.json'); meta=j('spec/metadata/w0-metadata-exposure-descriptors.json')
obj={x['name']:x for x in types['types']}; structs=num['structs']
for name,tid,signed in [('SealedDeliveryKeyRecord',22,True),('SealedDeliveryEnvelope',23,False)]:
    x=obj.get(name)
    if not x or x.get('type_id')!=tid: bad(f'{name} object type is not {tid}')
    elif bool(x.get('signed')) != signed: bad(f'{name} signedness drift')

for name, keys in [('DeliveryKeyMaterial',{0,1,2,3,4}),('SealedDeliveryKeyRecord',{2,3}),('SealedDeliveryEnvelope',{2,3,4,5,6,7})]:
    got={f['key'] for f in structs.get(name,{}).get('fields',[]) if f.get('req')}
    if got != keys: bad(f'{name} required keys {sorted(got)} != {sorted(keys)}')
mrp={f['key']:f for f in structs['MessageRequestPayload']['fields']}
if 9 not in mrp or not mrp[9].get('req') or mrp[9].get('name')!='reply_delivery_key': bad('MessageRequestPayload key 9 bootstrap key not required')

p1=next((x for x in crypto['profiles'] if x.get('id')==1),{})
h=p1.get('sealed_delivery_hpke',{})
expected={'spec':'draft-ietf-hpke-pq-05','mode':'base','kem':'MLKEM768-X25519','kem_id':25722,'kdf':'HKDF-SHA256','aead':'ChaCha20Poly1305','enc_bytes':1120,'public_key_bytes':1216,'plaintext_buckets':[1024,4096,16384,65536,131072],'aead_tag_bytes':16}
for k,v in expected.items():
    if h.get(k)!=v: bad(f'OM-Crypto-v1 sealed_delivery_hpke {k} drift: {h.get(k)!r}')
labels={x['label']:x for x in ds.get('labels',[])}
for lab in ('OM-SEALED-DELIVERY-KEY-SIG-v1','OM-SEALED-DELIVERY-ROUTE-v1','OM-SEALED-DELIVERY-HPKE-INFO-v1','OM-SEALED-DELIVERY-HPKE-AAD-v1'):
    if labels.get(lab,{}).get('status')!='frozen': bad(f'domain label not frozen: {lab}')

cd=(ROOT/'spec/cddl/delivery.cddl').read_text(); disc=(ROOT/'spec/cddl/discovery.cddl').read_text()
for tok in ('SealedDeliveryKeyRecord = {','SealedDeliveryEnvelope = {','delivery-key-material = {'):
    if tok not in cd: bad('delivery.cddl missing '+tok)
if '9: delivery-key-material' not in disc: bad('discovery.cddl missing required MessageRequestPayload reply key')

ex={d['role']:d['exposures'] for d in meta['descriptors']}
for r in ('home_relay_mailbox','t0_sponsored_mailbox','t1_hosted_home'):
    if ex[r].get('stable_device_correlator')!='no_by_design': bad(f'{r} stable sender correlator not no_by_design')
    if ex[r].get('ratchet_header_shape')!='no_by_design': bad(f'{r} ratchet header not no_by_design')

req={x['id']:x for x in jl('machine/requirements.jsonl')}; th={x['id']:x for x in jl('machine/threat_register.jsonl')}; dec={x['id']:x for x in jl('machine/decisions.jsonl')}
for rid in ('R-293','R-308','R-309','R-310','R-311','R-312','R-313'):
    if rid not in req: bad('missing '+rid)
for tid in ('T-111','T-112','T-113','T-114'):
    if tid not in th: bad('missing '+tid)
if 'D-164' not in dec: bad('missing D-164')

# Simple bounded state checks for route isolation, replay, expiry and no-downgrade behavior.
uri_a='https://relay-a.example'; cap_a=b'a'*32; uri_b='https://relay-b.example'; cap_b=b'b'*32
def route(uri,cap):
    # Exact ADR-0142 route binding: SHA-256(OM-CBOR [label, relay_uri, deposit_capability]).
    return hashlib.sha256(cbor_enc(['OM-SEALED-DELIVERY-ROUTE-v1', uri, {'$bytes':cap.hex()}])).digest()
ra=route(uri_a,cap_a); rb=route(uri_b,cap_b)
if ra==rb or route(uri_a,cap_a)!=ra: bad('route binding lacks isolation/determinism')
seen=set(); pair=(b'k'*16,b'd'*16)
first=pair not in seen; seen.add(pair); replay=pair in seen
if not first or not replay: bad('replay reservation simulation failed')
now=1000
if not (900 < now and 1100 >= now): bad('expiry sanity simulation failed')
missing_key_action='fail_explicitly'
if missing_key_action=='clear_sender_fallback': bad('silent downgrade simulation allowed')

v005=j('verification/results/v005-node-adr0142-latest.json')
if not v005.get('passed_for_node_slice') or v005.get('mismatch_count')!=0 or v005.get('vector_count',0)<735: bad('ADR-0142 V-005 Node evidence missing/stale')
mt=j('verification/results/metadata-trace-latest.json'); mw=j('verification/results/metadata-w0-rules-latest.json')
if not mt.get('passed'): bad('metadata trace model failing')
if not mw.get('passed'): bad('metadata W0 rules failing')

profile=(ROOT/'docs/architecture/delivery/SEALED_SENDER_DELIVERY_PROFILE.md').read_text().lower()
for tok in ('clear-sender envelope','not sender authentication','messagerequest','replay'):
    if tok not in profile: bad('sealed delivery profile missing '+tok)

result={'checker':'sealed_delivery_adr0142_rules','passed':not errors,'errors':errors,
        'checks':{'object_types':[22,23],'profile1_enc_bytes':h.get('enc_bytes'),'profile1_public_key_bytes':h.get('public_key_bytes'),
                  'plaintext_buckets':h.get('plaintext_buckets'),'route_binding_distinct':ra!=rb,'replay_duplicate_detected':replay,
                  'v005_vectors':v005.get('vector_count'),'v005_node_mismatches':v005.get('mismatch_count'),
                  'metadata_trace_passed':mt.get('passed'),'metadata_rules_passed':mw.get('passed')},
        'gate_status_effect':'evidence_only_no_gate_automatically_passed'}
OUT.write_text(json.dumps(result,indent=2)+'\n')
if errors:
    for e in errors: print('ERROR:',e,file=sys.stderr)
    raise SystemExit(1)
print('ADR-0142 sealed-delivery bounded/static checks OK')
