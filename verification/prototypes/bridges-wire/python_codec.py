#!/usr/bin/env python3
"""ADR-0264 bridge wire: stand-alone stdlib OM-CBOR-v1 codec and typed validator.

Independent of CBOR libraries and other OM verification code. Signature bytes,
Room policy, MLS and external proofs are NOT authenticated by this prototype.
"""
from __future__ import annotations
import hashlib
import hmac
import json
from pathlib import Path

BASE = Path(__file__).resolve().parents[3]
FIELDS = json.loads((BASE/'spec/registries/om-numeric-fields.json').read_text())
TYPES = json.loads((BASE/'spec/registries/om-object-types.json').read_text())
DOMAINS = json.loads((BASE/'spec/registries/om-domain-separation.json').read_text())
BY_NAME = {v['name']: v for v in TYPES['types']}
NEW_NAMES = ('BridgePrincipal','BridgeControlState','BridgeMapping','InteropTrace',
             'ForeignProvenance','ForeignIdentityLink','ConversationEvent')
CHAINS = {'BridgeControlState':(3,4),'BridgeMapping':(10,11),'ForeignIdentityLink':(10,11)}

class CodecError(ValueError):
    def __init__(self,code,why=''):
        self.code=code
        super().__init__(code+(': '+why if why else ''))

def fail(code,why=''):
    raise CodecError(code,why)

def _header(mt,n):
    if type(n) is not int or not 0<=n<2**64:fail('value_range','CBOR integer/size')
    if n<24:return bytes([(mt<<5)|n])
    if n<256:return bytes([(mt<<5)|24,n])
    if n<65536:return bytes([(mt<<5)|25])+n.to_bytes(2,'big')
    if n<2**32:return bytes([(mt<<5)|26])+n.to_bytes(4,'big')
    return bytes([(mt<<5)|27])+n.to_bytes(8,'big')

def encode(v,depth=0):
    """Canonical CBOR for OM admissible unsigned ints, bstr, tstr, maps, arrays, bool.
    None can be encoded solely to construct explicit null-negative fixtures; it
    never validates in a typed OM field. No float, tag or negative int encoding.
    """
    if depth>32:fail('value_range','nesting depth')
    if v is None:return b'\xf6'
    if v is True:return b'\xf5'
    if v is False:return b'\xf4'
    if type(v) is int:
        if v<0:fail('value_range','negative integer prohibited')
        return _header(0,v)
    if type(v) is bytes:return _header(2,len(v))+v
    if type(v) is str:
        b=v.encode('utf-8');return _header(3,len(b))+b
    if type(v) in (list,tuple):
        if len(v)>4096:fail('value_range','array allocation bound')
        return _header(4,len(v))+b''.join(encode(x,depth+1) for x in v)
    if type(v) is dict:
        if len(v)>4096:fail('value_range','map allocation bound')
        kv=[(encode(k,depth+1),encode(x,depth+1)) for k,x in v.items()]
        kv.sort(key=lambda p:(len(p[0]),p[0]))
        if any(kv[i][0]==kv[i-1][0] for i in range(1,len(kv))):
            fail('non_canonical','duplicate encoded map key')
        return _header(5,len(kv))+b''.join(k+x for k,x in kv)
    fail('value_range','unsupported Python CBOR type')

class _Reader:
    def __init__(self,wire):
        if type(wire) is not bytes:fail('value_range','input must be bytes')
        if len(wire)>1048576:fail('max_bytes','global allocation cap')
        self.wire=wire;self.pos=0
    def take(self,n):
        if n>len(self.wire)-self.pos:fail('non_canonical','truncated CBOR')
        b=self.wire[self.pos:self.pos+n];self.pos+=n;return b
    def readlen(self,a):
        if a<24:return a
        width={24:1,25:2,26:4,27:8}.get(a)
        if width is None:fail('non_canonical','indefinite or unsupported additional-info')
        x=int.from_bytes(self.take(width),'big')
        if x<{1:24,2:256,4:65536,8:2**32}[width]:
            fail('non_canonical','nonshortest integer or length')
        return x
    def item(self,depth=0):
        if depth>32:fail('value_range','decoder nesting')
        first=self.take(1)[0];mt=first>>5;a=first&31
        if mt==7:
            if a in (20,21,22):return {20:False,21:True,22:None}[a]
            fail('non_canonical','float/simple unsupported')
        if mt not in (0,2,3,4,5):fail('non_canonical','negative int, tag, or unsupported type')
        size=self.readlen(a)
        if mt==0:return size
        if mt in (2,3):
            if size>65536:fail('value_range','primitive allocation')
            raw=self.take(size)
            if mt==2:return raw
            try:return raw.decode('utf-8','strict')
            except UnicodeError:fail('non_canonical','bad UTF-8')
        if size>4096:fail('value_range','container allocation')
        if mt==4:return [self.item(depth+1) for _ in range(size)]
        out={};last=None
        for _ in range(size):
            begin=self.pos;k=self.item(depth+1);kb=self.wire[begin:self.pos]
            if last is not None and (len(kb),kb)<=(len(last),last):
                fail('non_canonical','out-of-order or duplicate map key')
            if type(k) not in (int,str,bytes):fail('non_canonical','map-key type')
            if k in out:fail('non_canonical','duplicate interpreted key')
            last=kb;out[k]=self.item(depth+1)
        return out

def decode(wire):
    r=_Reader(wire);obj=r.item()
    if r.pos!=len(wire):fail('non_canonical','trailing data')
    if encode(obj)!=wire:fail('non_canonical','not deterministic CBOR')
    return obj

def norm(v):
    """Diagnostic JSON representation of bstr as {$bytes:hex} and uint map keys."""
    if type(v) is list:return [norm(x) for x in v]
    if type(v) is dict:
        if set(v)=={'$bytes'}:return bytes.fromhex(v['$bytes'])
        if set(v)=={'$map'}:return {norm(k):norm(x) for k,x in v['$map']}
        return {(int(k) if type(k) is str and k.isdecimal() else k):norm(x) for k,x in v.items()}
    return v

def jsonable(v):
    if type(v) is bytes:return {'$bytes':v.hex()}
    if type(v) is list:return [jsonable(x) for x in v]
    if type(v) is dict:
        if all(type(k) is int for k in v):return {str(k):jsonable(x) for k,x in v.items()}
        return {'$map':[[jsonable(k),jsonable(x)] for k,x in v.items()]}
    return v

def _struct(v,name,wave='W4'):
    if type(v) is not dict or any(type(k) is not int or k<0 for k in v):
        fail('value_range',name+' must use unsigned numeric map keys')
    if name not in FIELDS['structs']:fail('value_range','unregistered struct '+name)
    spec=FIELDS['structs'][name]
    if 'union' in spec:
        rule=spec['union'];dest=rule['variants'].get(str(v.get(rule['on'])))
        if dest is None:fail('value_range','unsupported union variant')
        return _struct(v,dest,wave)
    fields={f['key']:f for f in spec['fields']}
    top=spec.get('top_level',False)
    allowed=set(fields)|({0,1,23} if top else set())
    if set(v)-allowed:fail('unknown_field',name+': '+str(sorted(set(v)-allowed)))
    if top:
        if v.get(0)!=BY_NAME[name]['type_id'] or v.get(1)!=1:
            fail('value_range',name+' type/version')
        if 23 in v:_validate_type(v[23],{'ext':None},name+'.extensions',wave)
    for k,f in fields.items():
        if k not in v:
            if f['req']:fail('missing_field',name+'.'+f['name'])
            continue
        if int(f.get('wave','W0')[1:])>int(wave[1:]):
            fail('unknown_field',name+'.'+f['name']+' added in '+f['wave'])
        t=f['type']
        if 'dispatch' in t:
            ds=t['dispatch']
            sel=next((a['key'] for a in fields.values() if a['name']==ds['on']),None)
            which=ds['table'].get(str(v.get(sel)),ds['default'])
            if which=='opaque_map':
                body=v[k]
                if type(body) is not dict or not body:fail('value_range','empty/invalid opaque event body')
                if {type(x) for x in body} not in ({int},{str}):fail('value_range','mixed body key types')
                encode(body)
            else:_struct(v[k],which,wave)
        else:_validate_type(v[k],t,name+'.'+f['name'],wave)

def _validate_type(x,t,where,wave):
    if x is None:fail('null_forbidden',where)
    if 'uint' in t:
        lo,hi=t['uint']
        if type(x) is not int or x<lo or (hi is not None and x>hi):fail('value_range',where)
    elif 'bool' in t:
        if type(x) is not bool or (t['bool'] is not None and x!=t['bool']):fail('value_range',where)
    elif 'bstr' in t:
        lo,hi=t['bstr']
        if type(x) is not bytes or not lo<=len(x)<=hi:fail('value_range',where)
    elif 'tstr' in t:
        lo,hi=t['tstr']
        if type(x) is not str or not lo<=len(x.encode('utf-8'))<=hi:fail('value_range',where)
    elif 'enum' in t:
        en=t['enum'];vals=FIELDS['enums'][en]
        if type(x) is not int or str(x) not in vals:fail('value_range',where+' enum')
        added=FIELDS['enum_value_waves'].get(en,{}).get(str(x),'W0')
        if int(added[1:])>int(wave[1:]):fail('value_range','future enum value')
    elif 'ref' in t:_struct(x,t['ref'],wave)
    elif 'set' in t or 'array' in t:
        kind='set' if 'set' in t else 'array';lo,hi=t['n']
        if type(x) is not list or not lo<=len(x)<=hi:fail('value_range',where+' count')
        for i,item in enumerate(x):_validate_type(item,t[kind],where+f'[{i}]',wave)
        if kind=='set':
            keys=[encode(v) for v in x]
            if any(keys[i-1]>=keys[i] for i in range(1,len(keys))):fail('set_order',where)
    elif 'ext' in t:
        if type(x) is not dict or not 1<=len(x)<=128:fail('value_range','extension map')
        for key,value in x.items():
            if type(key) is not str or not 1<=len(key.encode('utf-8'))<=128:
                fail('value_range','extension identifier')
            _struct(value,'ExtEntry',wave)
            if key.startswith('om.critical.') or value[0]:
                fail('unknown_field','unknown critical extension: '+key)
    elif 'any' in t:encode(x)
    else:fail('value_range','unsupported registry type '+str(t))

def object_digest(name,body):
    label=BY_NAME[name]['hash']
    if not label:fail('value_range','no registered object hash')
    return {0:1,1:hashlib.sha256(encode([label,encode(body)])).digest()}

def origin_fingerprint(room_secret,origin_namespace,stable_origin_ref):
    """No public Room identifier can substitute for the 32-byte secret."""
    if type(room_secret) is not bytes or len(room_secret)!=32:
        fail('value_range','fingerprint requires 32-byte private Room secret')
    if type(origin_namespace) is not str or not 1<=len(origin_namespace.encode())<=128:
        fail('value_range','origin namespace')
    if type(stable_origin_ref) is not str or not 1<=len(stable_origin_ref.encode())<=512:
        fail('value_range','origin event ref')
    return hmac.new(room_secret,encode(['OM-INTEROP-FP-MAC-v1',origin_namespace,stable_origin_ref]),hashlib.sha256).digest()

def _chain(name,body,prior):
    if name not in CHAINS:return
    vkey,hkey=CHAINS[name]
    version=body[vkey]
    if version==1 and hkey in body:fail('forked_chain','genesis must omit predecessor')
    if version>1 and hkey not in body:fail('missing_field','non-genesis must have predecessor')
    if prior is None:return
    # prior is authenticated external input, never merely an untrusted decoded state.
    _struct(prior,name)
    if version<=prior[vkey]:
        if version==prior[vkey] and object_digest(name,body)!=object_digest(name,prior):
            fail('forked_chain','same-version conflicting head')
        fail('stale_chain','replay or rollback')
    if version!=prior[vkey]+1:fail('stale_chain','skipped predecessor')
    if body[hkey]!=object_digest(name,prior):fail('forked_chain','predecessor hash mismatch')
    stable={'BridgeControlState':(2,), 'BridgeMapping':(2,3,4,5,6),
            'ForeignIdentityLink':(2,3,4)}[name]
    if any(body[k]!=prior[k] for k in stable):fail('forked_chain','changed incarnation identity')
    if name=='BridgeControlState':
        old={e[0]:e for e in prior[7]};fresh={e[0]:e for e in body[7]}
        for eid,endpoint in old.items():
            if eid not in fresh:fail('stale_chain','endpoint dropped without revoked tombstone')
            if endpoint[1]==3 and fresh[eid][1]!=3:
                fail('stale_chain','revoked endpoint resurrection')
            if endpoint[1]==2 and fresh[eid][1]==1:
                fail('stale_chain','quarantined endpoint must use new incarnation')
            if any(fresh[eid][k]!=endpoint[k] for k in (2,3)):
                fail('stale_chain','reusing endpoint incarnation ID with changed profile/public key')
    elif name=='BridgeMapping' and prior[13]==2 and body[13]!=2:
        fail('stale_chain','revoked mapping resurrection')
    elif name=='ForeignIdentityLink' and prior[9]==2 and body[9]!=2:
        fail('stale_chain','revoked identity link resurrection')

def _trace(trace):
    path=trace[2];hops=trace[3];limit=trace[4]
    if len(path)!=hops or hops>limit:fail('value_range','trace hop/path mismatch or limit exceeded')
    if len(set(path))!=len(path):fail('loop_detected','repeated bridge in path')
    if len(set(trace[5]))!=len(trace[5]):fail('value_range','duplicate transformation ID')

def _prov(prov):
    flags=prov[7]
    if 1 in flags and len(flags)!=1:fail('value_range','exact cannot coexist with loss flags')
    if 3 not in prov and 8 not in flags:
        fail('missing_field','unknown foreign actor needs foreign_actor_unverified')
    if 9 in prov:
        _trace(prov[9])
        if 8 not in prov:fail('missing_field','interop trace requires fingerprint')
        if prov[8]!=prov[9][1]:fail('provenance_mismatch','fingerprint and trace disagree')

def _event_kind_body(body):
    kind=body[8];p=body[11]
    if kind in (7,11,12,13,14):
        req,allowed={7:({0},{0,1}),11:({0,1},{0,1}),12:({0,1},{0,1}),
                     13:({0,1},{0,1}),14:({0},{0,1})}[kind]
        if not req<=set(p):fail('missing_field','event kind body')
        if set(p)-allowed:fail('unknown_field','event kind body')
        _validate_type(p[0],{'bstr':[16,16]},'event target','W4')
        if kind in (11,12) or (kind==7 and 1 in p):
            _validate_type(p[1],{'tstr':[1,256]},'event text','W4')
        if kind==13:_validate_type(p[1],{'uint':[1,3]},'topic state','W4')
        if kind==14 and 1 in p:_validate_type(p[1],{'bstr':[16,16]},'topic','W4')
    if kind==1 and 0 not in p and not p[1]:
        fail('value_range','bridged empty message.create')
    if kind in (15,16,17,18) and 14 in body:
        fail('value_range','later W1 event kind body unavailable in supplied CDDL')

def _special(name,b,*,sigs=None,prior=None,now=None,control=None,mapping=None,receiver_wave='W4'):
    if name=='BridgePrincipal':return
    if name=='InteropTrace':_trace(b[2]);return
    if name=='ForeignProvenance':_prov(b[2]);return
    if name=='ConversationEvent':
        p=b[5];_event_kind_body(b)
        if 13 in b and 14 in b:fail('wrong_role','bridge and service overlays are exclusive')
        if 14 in b:
            if p[0]!=3 or len(p[1])!=32:fail('wrong_role','foreign import must be bridge-authored')
            f=b[14];_prov(f)
            if 8 not in f or 9 not in f:fail('missing_field','imported event must carry fingerprint and trace')
            trace=f[9]
            if not trace[2] or trace[2][-1]!=p[1]:
                fail('provenance_mismatch','trace must end with bridge author')
            if trace[3]>=trace[4]:fail('loop_detected','hop limit exhausted before import')
            if control is not None:
                if control[2]!=p[1] or b[6] not in {e[0] for e in control[7] if e[1]==1}:
                    fail('wrong_role','no active endpoint in BridgeControlState')
            if mapping is not None:
                if mapping[3]!=p[1] or mapping[4][1]!=b[4] or mapping[13]!=1 or mapping[7] not in (2,3):
                    fail('wrong_role','inbound Room mapping not authorized')
                if f[0]!=mapping[5] or f[1]!=mapping[6]:
                    fail('provenance_mismatch','foreign namespace/scope not mapped')
        elif p[0]==3:fail('missing_field','bridge-authored event needs foreign provenance')
        if p[0] in (2,3) and len(p[1])!=32:fail('value_range','ServiceID/BridgeID length')
        return
    if name=='BridgeControlState':
        if b[6]>len(b[5]):fail('value_range','approval threshold exceeds controllers')
        if len({e[0] for e in b[7]})!=len(b[7]):fail('set_order','reused endpoint incarnation')
        if len({e[0] for e in b[8]})!=len(b[8]):fail('set_order','duplicate adapter ID')
        if len({e[1] for e in b[8]})!=len(b[8]):fail('set_order','duplicate namespace adapter')
        for a in b[8]:
            cap=a[2]
            if 5 in cap:
                if len({limit[0] for limit in cap[5]})!=len(cap[5]):
                    fail('set_order','duplicate semantic size limit')
                if any(limit[0] not in cap[0] for limit in cap[5]):
                    fail('value_range','size limit claims undeclared semantic')
        if sigs is not None and len({s[0] for s in sigs})<b[6]:
            fail('wrong_role','signature threshold not structurally met')
    if name=='BridgeMapping':
        policy=b[9]
        if policy[0] and not policy[2]:fail('wrong_role','relay requires metadata preservation policy')
        if not policy[2] and b[7] in (1,3):
            fail('wrong_role','export with unprotected metadata path')
        if control is not None:
            if b[3]!=control[2] or b[14]!={0:control[3],1:object_digest('BridgeControlState',control)}:
                fail('wrong_role','mapping not bound to live bridge control head')
            adapters=[a for a in control[8] if a[1]==b[5]]
            if not adapters:fail('wrong_role','mapping namespace lacks adapter')
            if (policy[0] or (b[7] in (1,3) and policy[2])) and not adapters[0][2][1]:
                fail('wrong_role','outbound/relay adapter must preserve trace')
            if any(x not in adapters[0][2][0] for x in b[8][1]):
                fail('wrong_role','capabilities exceed adapter declaration')
    if name=='ForeignIdentityLink':
        if (b[5]==5)!=(12 in b):fail('missing_field' if b[5]==5 else 'value_range','attesting BridgeID restricted to bridge proof')
        if 13 in b and b[13]>=b[8]:fail('value_range','link validity interval')
        if now is not None and b[9]==1 and now>=b[8]:fail('stale_chain','expired identity link')
    _chain(name,b,prior)

def validate_body(name,body,*,receiver_wave='W4',prior=None,now=None,control=None,mapping=None,signatures=None):
    if name not in NEW_NAMES:fail('value_range','object outside bridges-wire scope')
    if name=='ConversationEvent' and receiver_wave=='W0' and type(body) is dict and 14 in body:
        fail('w0_receiver_rejection','key14 unknown critical W0 field')
    if name=='ConversationEvent' and receiver_wave=='W0' and type(body) is dict and 13 in body:
        fail('w0_receiver_rejection','key13 unknown critical W0 field')
    if name!='ConversationEvent' and receiver_wave=='W0':
        fail('w0_receiver_rejection','W4 bridge object type unknown at W0')
    _struct(body,name,receiver_wave)
    _special(name,body,sigs=signatures,prior=prior,now=now,control=control,mapping=mapping,receiver_wave=receiver_wave)
    return body

def validate_wire(name,wire,*,receiver_wave='W4',prior=None,now=None,control=None,mapping=None):
    if name not in NEW_NAMES:fail('value_range','not a supported bridge type')
    if len(wire)>BY_NAME[name]['max_bytes']:fail('max_bytes',name)
    decoded=decode(wire)
    signed=BY_NAME[name]['signed']
    sigs=None
    if signed:
        _struct(decoded,'SignedEnvelope',receiver_wave)
        sigs=decoded[2]
        allowed={int(k) for k,v in FIELDS['enums']['sig_role'].items() if v in signed['roles']}
        if any(sig[2] not in allowed for sig in sigs):fail('wrong_role',name+' signature role')
        if len({s[0] for s in sigs})!=len(sigs):fail('wrong_role','duplicate signer')
        enc=[encode(s) for s in sigs]
        if enc!=sorted(enc):fail('set_order','unsorted signatures')
        body=decode(decoded[1])
    else:body=decoded
    return validate_body(name,body,receiver_wave=receiver_wave,prior=prior,now=now,control=control,
                         mapping=mapping,signatures=sigs)

def make_signature(role=1,signer=b'\x10'*16):
    return {0:signer,1:1,2:role,3:b'\x20'*64}

def sign_wire(name,body,*,signatures=None,role=1):
    if BY_NAME[name]['signed'] is None:return encode(body)
    if signatures is None:signatures=[make_signature(role)]
    return encode({1:encode(body),2:sorted(signatures,key=encode)})

if __name__=='__main__':
    import argparse
    ap=argparse.ArgumentParser(description=__doc__)
    ap.add_argument('object',choices=NEW_NAMES);ap.add_argument('hex')
    ap.add_argument('--w0',action='store_true')
    opt=ap.parse_args()
    try:
        print(json.dumps(jsonable(validate_wire(opt.object,bytes.fromhex(opt.hex),receiver_wave='W0' if opt.w0 else 'W4')),indent=2))
    except CodecError as e:ap.error(str(e))
