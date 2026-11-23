#!/usr/bin/env python3
"""Standalone stdlib OM-CBOR-v1 encoder/decoder and W4 service wire validator.

Registry-driven shapes; semantic checks are independently implemented below.
Signatures are checked for STRUCTURE/role only, NOT cryptographically verified.
"""
from __future__ import annotations
import hashlib
import json
from pathlib import Path

ROOT = Path(__file__).resolve().parents[3]
FIELDS = json.loads((ROOT / 'spec/registries/om-numeric-fields.json').read_text())
TYPES = json.loads((ROOT / 'spec/registries/om-object-types.json').read_text())
DOMAINS = json.loads((ROOT / 'spec/registries/om-domain-separation.json').read_text())
BY_NAME = {t['name']: t for t in TYPES['types']}
NEW_NAMES = ('ServicePrincipal', 'ServiceControlState', 'ServiceDefinition',
             'ServiceInstallation', 'RepresentationGrant', 'ContextDisclosureBundle',
             'ConversationEvent')
HASH_CHAIN = {'ServiceControlState': (3, 4), 'ServiceInstallation': (14, 15),
              'RepresentationGrant': (14, 15)}

def bs(a,b): return {'bstr':[a,b]}
def ts(a,b): return {'tstr':[a,b]}

class CodecError(ValueError):
    def __init__(self, code: str, detail: str = ''):
        self.code = code
        super().__init__(code + (': ' + detail if detail else ''))

def fail(code: str, detail: str = ''):
    raise CodecError(code, detail)

def head(major: int, n: int) -> bytes:
    if not 0 <= n < 2**64: fail('value_range', 'CBOR integer/length')
    if n < 24: return bytes([(major << 5) | n])
    if n < 256: return bytes([(major << 5) | 24, n])
    if n < 65536: return bytes([(major << 5) | 25]) + n.to_bytes(2, 'big')
    if n < 2**32: return bytes([(major << 5) | 26]) + n.to_bytes(4, 'big')
    return bytes([(major << 5) | 27]) + n.to_bytes(8, 'big')

def encode(v, depth=0):
    if depth > 32: fail('value_range', 'max CBOR depth')
    if v is None: return b'\xf6'
    if v is False: return b'\xf4'
    if v is True: return b'\xf5'
    if type(v) is int: return head(0, v) if v >= 0 else fail('value_range', 'negative integer')
    if type(v) is bytes: return head(2, len(v)) + v
    if type(v) is str:
        b = v.encode('utf-8'); return head(3, len(b)) + b
    if type(v) in (tuple, list):
        if len(v) > 4096: fail('value_range', 'array too long')
        return head(4, len(v)) + b''.join(encode(x, depth+1) for x in v)
    if type(v) is dict:
        if len(v) > 4096: fail('value_range', 'map too long')
        pairs = [(encode(k, depth+1), encode(val, depth+1)) for k, val in v.items()]
        pairs.sort(key=lambda kv: (len(kv[0]), kv[0]))
        if any(pairs[i][0] == pairs[i+1][0] for i in range(len(pairs)-1)):
            fail('non_canonical', 'duplicate CBOR map key')
        return head(5, len(pairs)) + b''.join(k+val for k,val in pairs)
    fail('value_range', 'unsupported CBOR type ' + type(v).__name__)

class Decoder:
    def __init__(self, data):
        if type(data) is not bytes: fail('value_range', 'input must be bytes')
        if len(data) > 1048576: fail('max_bytes', 'input exceeds 1 MiB predecode cap')
        self.buf = data; self.pos = 0

    def take(self,n):
        if n > len(self.buf)-self.pos: fail('non_canonical', 'truncated CBOR')
        v = self.buf[self.pos:self.pos+n]; self.pos += n
        return v

    def length(self,add):
        if add < 24: return add
        widths = {24:1, 25:2, 26:4, 27:8}
        if add not in widths: fail('non_canonical', 'indefinite or reserved CBOR header')
        n = int.from_bytes(self.take(widths[add]), 'big')
        mins = {24:24, 25:256, 26:65536, 27:2**32}
        if n < mins[add]: fail('non_canonical', 'non-shortest length or integer')
        return n

    def item(self, depth=0):
        if depth > 32: fail('value_range', 'CBOR nesting')
        start = self.pos
        ib = self.take(1)[0]
        major,add = ib >> 5, ib & 31
        if major in (0,2,3,4,5):
            n = self.length(add)
        elif major == 7:
            if add == 20: return False
            if add == 21: return True
            if add == 22: return None
            fail('non_canonical', 'float/simple not in OM-CBOR-v1')
        else:
            fail('non_canonical', 'negative integer, tag or unsupported major type')
        if major == 0: return n
        if major in (2,3):
            if n > 65536: fail('value_range', 'primitive length allocation cap')
            blob = self.take(n)
            if major == 2: return blob
            try: return blob.decode('utf-8', 'strict')
            except UnicodeDecodeError: fail('non_canonical', 'invalid UTF-8')
        if n > 4096: fail('value_range', 'collection allocation cap')
        if major == 4: return [self.item(depth+1) for _ in range(n)]
        m={}; last=None
        for _ in range(n):
            kstart=self.pos
            key=self.item(depth+1)
            kb=self.buf[kstart:self.pos]
            if last is not None and (len(last),last) >= (len(kb),kb):
                fail('non_canonical', 'map keys duplicate or not deterministic CBOR order')
            last=kb
            if type(key) not in (str,int,bytes): fail('non_canonical', 'invalid map key')
            if key in m: fail('non_canonical', 'duplicate decoded map key')
            m[key]=self.item(depth+1)
        return m

def decode(data: bytes):
    d=Decoder(data); value=d.item()
    if d.pos != len(data): fail('non_canonical', 'trailing CBOR bytes')
    if encode(value) != data: fail('non_canonical', 're-encoding mismatch')
    return value

def norm(v):
    """JSON diagnostic notation: {$bytes:hex}, decimal integer map keys."""
    if type(v) is list: return [norm(x) for x in v]
    if type(v) is dict:
        if set(v)=={'$bytes'}: return bytes.fromhex(v['$bytes'])
        if set(v)=={'$map'}: return {norm(k):norm(x) for k,x in v['$map']}
        return {(int(k) if k.isdecimal() else k):norm(x) for k,x in v.items()}
    return v

def jsonable(v):
    if type(v) is bytes: return {'$bytes':v.hex()}
    if type(v) is list: return [jsonable(x) for x in v]
    if type(v) is dict:
        if all(type(k) is int for k in v): return {str(k):jsonable(x) for k,x in v.items()}
        return {'$map':[[jsonable(k),jsonable(x)] for k,x in v.items()]}
    return v

def _map(v, label='map'):
    if type(v) is not dict or any(type(k) is not int or k<0 for k in v):
        fail('value_range', label+' must be numeric-key map')

def _typed(v, spec, where='', wave='W4'):
    if v is None: fail('null_forbidden',where)
    if 'uint' in spec:
        lo,hi=spec['uint']
        if type(v) is not int or v<lo or (hi is not None and v>hi): fail('value_range',where)
    elif 'bool' in spec:
        if type(v) is not bool or (spec['bool'] is not None and v!=spec['bool']): fail('value_range',where)
    elif 'bstr' in spec:
        lo,hi=spec['bstr']
        if type(v) is not bytes or not lo<=len(v)<=hi: fail('value_range',where)
    elif 'tstr' in spec:
        lo,hi=spec['tstr']
        if type(v) is not str or not lo<=len(v.encode('utf-8'))<=hi: fail('value_range',where)
    elif 'enum' in spec:
        e=spec['enum']; vals=FIELDS['enums'][e]
        if type(v) is not int or str(v) not in vals: fail('value_range',where+' enum')
        introduced=FIELDS['enum_value_waves'].get(e,{}).get(str(v),'W0')
        if int(introduced[1:])>int(wave[1:]): fail('value_range',where+' unavailable at '+wave)
    elif 'ref' in spec: _struct(v, spec['ref'],wave)
    elif 'array' in spec or 'set' in spec:
        set_mode='set' in spec; which='set' if set_mode else 'array'
        lo,hi=spec['n']
        if type(v) is not list or not lo<=len(v)<=hi: fail('value_range',where+' count')
        for i,x in enumerate(v): _typed(x,spec[which],where+f'[{i}]',wave)
        if set_mode:
            ordered=[encode(x) for x in v]
            if any(ordered[i]>=ordered[i+1] for i in range(len(ordered)-1)): fail('set_order',where)
    elif 'dispatch' in spec:
        fail('value_range', 'dispatch needs parent selector')
    elif 'ext' in spec: _extensions(v,where)
    elif 'any' in spec:
        encode(v) # must be CBOR representable, may be extension any
    else: fail('value_range','unsupported registry type '+repr(spec))

def _extensions(v,where):
    if type(v) is not dict or not 1<=len(v)<=128:
        fail('value_range', where+' extensions nonempty string-key map')
    for ext_id,entry in v.items():
        if type(ext_id) is not str or not 1<=len(ext_id.encode())<=128:
            fail('value_range',where+' extension ID')
        _struct(entry,'ExtEntry')
        if ext_id.startswith('om.critical.') or entry[0]:
            fail('unknown_field','unsupported critical extension '+ext_id)


def _struct(v, name, wave='W4'):
    _map(v,name)
    meta=FIELDS['structs'][name]
    if 'union' in meta:
        un=meta['union']
        variant=un['variants'].get(str(v.get(un['on'])))
        if variant is None: fail('value_range',name+' unknown union discriminator')
        return _struct(v,variant,wave)
    fields={f['key']:f for f in meta['fields']}
    allowed=set(fields)
    top=meta.get('top_level',False)
    if top: allowed.update((0,1,23))
    extra=set(v)-allowed
    if extra: fail('unknown_field',name+': '+str(sorted(extra)))
    if top:
        actual_type=BY_NAME.get(name,{}).get('type_id')
        if v.get(0)!=actual_type or v.get(1)!=1:
            fail('value_range',name+' object_type/object_version')
        if 23 in v:_typed(v[23],{'ext':None},name+'.extensions',wave)
    for f in fields.values():
        key=f['key'];label=name+'.'+f['name']
        if key not in v:
            if f['req']: fail('missing_field',label)
            continue
        ts=f['type']
        if 'dispatch' in ts:
            disp=ts['dispatch'];selector=next((g['key'] for g in fields.values() if g['name']==disp['on']),None)
            kind=v.get(selector)
            bodytype=disp['table'].get(str(kind),disp['default'])
            if bodytype=='opaque_map':
                if type(v[key]) is not dict or not v[key]:fail('value_range',label+' opaque body')
                keys=list(v[key]); kinds={type(k) for k in keys}
                if kinds not in ({int},{str}):fail('value_range',label+' mixed/nonstring keys')
                encode(v[key])
            else: _struct(v[key],bodytype,wave)
        else:_typed(v[key],ts,label,wave)

def _digest(value):
    return type(value) is dict and set(value)=={0,1} and value[0]==1 and type(value[1]) is bytes and len(value[1])==32

def object_digest(name, body):
    label=BY_NAME[name]['hash']
    if label is None: fail('value_range','unsigned identity without hash label')
    digest=hashlib.sha256(encode([label,encode(body)])).digest()
    return {0:1,1:digest}

def _check_chain(name,body,prior=None):
    if name not in HASH_CHAIN:return
    version_key,prev_key=HASH_CHAIN[name]
    v=body[version_key]
    if v==1 and prev_key in body:fail('forked_chain',name+' genesis must omit predecessor')
    if v>1 and prev_key not in body:fail('missing_field',name+' non-genesis predecessor')
    if prior is None:return
    expected_version=prior[version_key]+1
    if v == prior[version_key]:
        if object_digest(name,body) != object_digest(name,prior):
            fail('forked_chain',name+' conflicting record at same version')
        fail('stale_chain',name+' duplicate previously accepted head')
    if v < expected_version: fail('stale_chain',name+' replay/rollback')
    if v > expected_version: fail('stale_chain',name+' skipped unverified predecessor')
    if body[prev_key]!=object_digest(name,prior): fail('forked_chain',name+' wrong predecessor hash')
    for key in ({'ServiceControlState':2,'ServiceInstallation':2,'RepresentationGrant':2}[name],):
        if body[key]!=prior[key]:fail('forked_chain',name+' changed stable incarnation')
    if name=='ServiceControlState':
        old={e[0]:e for e in prior[7]}; new={e[0]:e for e in body[7]}
        for endpoint,e in old.items():
            if e[1]==3 and (endpoint not in new or new[endpoint][1]!=3):
                fail('stale_chain','revoked endpoint tombstone dropped or resurrected')
            if e[1]==2 and endpoint in new and new[endpoint][1]==1:
                fail('stale_chain','quarantined endpoint reopened without new incarnation')
            if e[1] in (1,2) and endpoint not in new:
                fail('stale_chain','endpoint deletion requires a revoked tombstone')
    else:
        if prior[11 if name=='ServiceInstallation' else 12]==2 and body[11 if name=='ServiceInstallation' else 12]!=2:
            fail('stale_chain','revoked grant/installation resurrected')

def _special(name,body,signatures,receiver_wave='W4',prior=None,now=None,grant=None):
    if name=='ConversationEvent':
        if receiver_wave=='W0' and 13 in body:fail('w0_receiver_rejection','event key 13 reserved/critical to W0')
        p=body[5]
        if p[0]==2 and len(p[1])!=32:fail('value_range','ServiceID must have 32 bytes')
        if 13 in body:
            if p[0]!=2:fail('value_range','represented event author must be ServicePrincipal')
            if grant is not None:
                if (grant[3]!=body[13][0] or grant[4]!=p[1] or
                    object_digest('RepresentationGrant',grant)!=body[13][1] or grant[12]!=1):
                    fail('wrong_role','invalid service representation binding')
                action={1:'message.send',2:'message.edit',3:'message.delete',4:'reaction.set',5:'receipt.update',6:'pin.set',7:'moderation.redact',8:'poll.create',9:'poll.vote',10:'poll.close',11:'topic.create',12:'topic.update',13:'topic.status.set',14:'message.topic.set'}.get(body[8])
                if action is not None and action not in grant[6]:
                    fail('wrong_role','representation grant does not allow event action')
        if body[8] not in range(1,19):fail('value_range','unknown event kind')
        if body[8] in (7,11,12,13,14):
            b=body[11]
            sets={7:({0},{0,1}),11:({0,1},{0,1}),12:({0,1},{0,1}),13:({0,1},{0,1}),14:({0},{0,1})}
            required,valid=sets[body[8]]
            if not required<=set(b):fail('missing_field','typed existing event body')
            if set(b)-valid:fail('unknown_field','typed existing event body')
            _typed(b[0],bs(16,16),'typed event target')
            if body[8] in (11,12) or (body[8]==7 and 1 in b):
                _typed(b[1],ts(1,256),'typed event text')
            if body[8]==13: _typed(b[1],{'uint':[1,3]},'topic status')
            if body[8]==14 and 1 in b: _typed(b[1],bs(16,16),'topic assignment')
        if body[8]==1 and 0 not in body[11] and not body[11][1]:
            fail('value_range','empty service message.create')
        if body[8] in (15,16,17,18) and 13 in body:
            fail('value_range','W1 group event body not present in supplied event CDDL')
        return
    if name=='ServicePrincipal':return
    if name=='ServiceControlState':
        if body[6]>len(body[5]):fail('value_range','approval_threshold > controllers')
        ep=[e[0] for e in body[7]]
        if len(set(ep))!=len(ep):fail('set_order','duplicate endpoint_id')
        if signatures is not None:
            signers={s[0] for s in signatures}
            if len(signers)<body[6]:fail('wrong_role','insufficient distinct controller signatures')
    elif name=='ServiceDefinition':
        ids=[e[0] for e in body[6]]
        if len(set(ids))!=len(ids):fail('set_order','duplicate command_id')
    elif name=='ServiceInstallation':
        scope=body[4][0]; mode=body[6]
        if mode==3 and scope!=2:fail('value_range','full member only in Room scope')
        if mode==2 and scope!=1:fail('value_range','local personal only in personal scope')
        if mode==3 and 13 not in body:fail('missing_field','full member state_head')
        if mode!=3 and 12 in body:fail('value_range','trigger_filter only on full member')
        if body[9][0][0]!=1 or body[10][0]!=1:fail('wrong_role','installation approver and creator must be independent account principals')
        if signatures is not None:
            expected=3 if scope==1 else 1
            if any(s[2]!=expected for s in signatures):fail('wrong_role','installation scope signing role')
    elif name=='RepresentationGrant':
        if 10 in body and 11 in body and body[10]>=body[11]:fail('value_range','grant invalid validity interval')
        if body[12]==1 and signatures is not None:
            if any(s[2]!=1 or s[0]!=body[8] for s in signatures):
                fail('wrong_role','grant issuance must be by named account control device')
        if body[12]==2 and signatures is not None:
            if any(s[2] not in (1,3) for s in signatures):fail('wrong_role','grant revocation signer')
        if 'message.read_other_participant' in body[6] and 13 not in body:
            fail('missing_field','counterparty notice evidence')
        forbidden={'account.control','account.recovery','device.enroll','key.export','representation.redelegate'}
        if any(a in forbidden for a in body[6]):fail('wrong_role','representation may not acquire account authority')
    elif name=='ContextDisclosureBundle':
        if now is not None and body[8]<=now:fail('value_range','disclosure expired')
        if len(encode(body))>65536:fail('max_bytes','disclosure body exceeds bound')
    _check_chain(name,body,prior)


def validate_body(name,body,*,receiver_wave='W4',prior=None,now=None,grant=None,signatures=None):
    if name not in NEW_NAMES:fail('value_range','unlisted service object')
    if name=='ConversationEvent' and receiver_wave=='W0' and type(body) is dict and 13 in body:
        fail('w0_receiver_rejection','event key13 is W4 critical')
    _struct(body,name,receiver_wave)
    _special(name,body,signatures,receiver_wave,prior,now,grant)
    return body

def validate_wire(name,wire,*,receiver_wave='W4',prior=None,now=None,grant=None):
    if name not in NEW_NAMES:fail('value_range','unlisted type')
    meta=BY_NAME[name]
    if len(wire)>meta['max_bytes']:fail('max_bytes',name)
    envelope=decode(wire)
    sigs=None
    if meta['signed'] is not None:
        _struct(envelope,'SignedEnvelope',receiver_wave)
        sigs=envelope[2]
        allowed_roles={next(int(k) for k,v in FIELDS['enums']['sig_role'].items() if v==r) for r in meta['signed']['roles']}
        if any(s[2] not in allowed_roles for s in sigs):fail('wrong_role',name+' disallowed Signature.role')
        if len({s[0] for s in sigs})!=len(sigs):fail('wrong_role','duplicate Signature.signer')
        if [encode(s) for s in sigs]!=sorted([encode(s) for s in sigs]):fail('set_order','signature order')
        body=decode(envelope[1])
    else:body=envelope
    validate_body(name,body,receiver_wave=receiver_wave,prior=prior,now=now,grant=grant,signatures=sigs)
    if name=='ConversationEvent' and receiver_wave=='W0' and 13 in body:
        fail('w0_receiver_rejection','reserved key13')
    return body

def make_signature(role=1, signer=None):
    return {0:(bytes([0x10])*16 if signer is None else signer),1:1,2:role,3:b'\x20'*64}

def sign_wire(name,body,*,role=None,signatures=None):
    meta=BY_NAME[name]
    if meta['signed'] is None:return encode(body)
    if signatures is None:
        if role is None:
            role={'ServiceInstallation':3}.get(name,1)
        signatures=[make_signature(role)]
    signatures=sorted(signatures,key=lambda s:encode(s))
    return encode({1:encode(body),2:signatures})

if __name__=='__main__':
    import argparse
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('name',choices=NEW_NAMES);parser.add_argument('hex')
    parser.add_argument('--w0',action='store_true')
    args=parser.parse_args()
    try: print(json.dumps(jsonable(validate_wire(args.name,bytes.fromhex(args.hex),receiver_wave='W0' if args.w0 else 'W4')),indent=2))
    except CodecError as exc: parser.error(str(exc))
