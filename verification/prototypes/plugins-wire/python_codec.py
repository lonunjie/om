#!/usr/bin/env python3
"""ADR-0266: independently written OM-CBOR-v1/ W4 plugin schema validator.

Python stdlib only. Independent of OM/bridges codecs: this is a wire-format
prototype, not cryptographic verification or a secure plugin host.
"""
from __future__ import annotations
import hashlib
import json
from pathlib import Path

ROOT = Path(__file__).resolve().parents[3]
REG = ROOT / 'spec' / 'registries'
OBJECTS = json.loads((REG / 'om-object-types.json').read_text())
FIELDS = json.loads((REG / 'om-numeric-fields.json').read_text())
DOMAINS = json.loads((REG / 'om-domain-separation.json').read_text())
TYPES = {r['name']:r for r in OBJECTS['types']}
PLUGIN_NAMES = [r['name'] for r in OBJECTS['types'] if r['scope']=='plugins']
NAME_BY_ID = {r['type_id']:r['name'] for r in OBJECTS['types']}
CAPABILITIES = {int(n):v for n,v in FIELDS['enums']['plugin_capability_id'].items()}
ENCODING_LIMIT = 1048576

class CodecError(Exception):
    def __init__(self, code, why):
        self.code = code
        super().__init__(f'{code}: {why}')

def reject(code, why):
    raise CodecError(code, why)

def head(mt, size):
    if size < 0 or size >= 2**64: reject('value_range', 'CBOR length out of range')
    if size < 24: return bytes([(mt<<5)|size])
    if size <= 255: return bytes([(mt<<5)|24,size])
    if size <= 65535: return bytes([(mt<<5)|25])+size.to_bytes(2,'big')
    if size <= 4294967295: return bytes([(mt<<5)|26])+size.to_bytes(4,'big')
    return bytes([(mt<<5)|27])+size.to_bytes(8,'big')

def encode(value, level=0):
    if level>24: reject('value_range','nesting exceeds 24')
    if value is None:return b'\xf6' # emission used ONLY to test null rejection
    if value is True:return b'\xf5'
    if value is False:return b'\xf4'
    if type(value) is int:
        if value<0:reject('value_range','no negative integers')
        return head(0,value)
    if type(value) is bytes:return head(2,len(value))+value
    if type(value) is str:
        b=value.encode('utf-8','strict');return head(3,len(b))+b
    if type(value) in (list,tuple):
        if len(value)>4096:reject('value_range','oversized array')
        return head(4,len(value))+b''.join(encode(x,level+1) for x in value)
    if type(value) is dict:
        if len(value)>4096:reject('value_range','oversized map')
        entries=[(encode(k,level+1),encode(v,level+1)) for k,v in value.items()]
        entries.sort(key=lambda kv:(len(kv[0]),kv[0]))
        if any(entries[i-1][0]==entries[i][0] for i in range(1,len(entries))):
            reject('non_canonical','duplicate map key')
        return head(5,len(entries))+b''.join(k+v for k,v in entries)
    reject('value_range','unsupported object')

class Reader:
    def __init__(self,raw):
        if type(raw) is not bytes:reject('value_range','bytes required')
        if len(raw)>ENCODING_LIMIT:reject('max_bytes','hard global cap')
        self.b=raw;self.i=0
    def take(self,n):
        if n>len(self.b)-self.i:reject('non_canonical','truncated item')
        v=self.b[self.i:self.i+n];self.i+=n;return v
    def argument(self,ai):
        if ai<24:return ai
        width={24:1,25:2,26:4,27:8}.get(ai)
        if width is None:reject('non_canonical','indefinite or reserved additional info')
        v=int.from_bytes(self.take(width),'big')
        if v < {1:24,2:256,4:65536,8:4294967296}[width]:reject('non_canonical','not shortest form')
        return v
    def value(self,depth=0):
        if depth>24:reject('value_range','nesting exceeds 24')
        first=self.take(1)[0];mt=first>>5;ai=first&31
        if mt==7:
            if ai in (20,21):return ai==21
            if ai==22:return None
            reject('non_canonical','float / CBOR simple disallowed')
        if mt not in (0,2,3,4,5):reject('non_canonical','negative/tag/unsupported type')
        n=self.argument(ai)
        if mt==0:return n
        if mt in (2,3):
            if n>65536:reject('value_range','primitive allocation bound')
            b=self.take(n)
            if mt==2:return b
            try:return b.decode('utf-8','strict')
            except UnicodeError:reject('non_canonical','invalid UTF-8')
        if n>4096:reject('value_range','container allocation bound')
        if mt==4:return [self.value(depth+1) for _ in range(n)]
        out={};last=None
        for _ in range(n):
            begin=self.i;k=self.value(depth+1);rawkey=self.b[begin:self.i]
            if last is not None and (len(rawkey),rawkey)<=(len(last),last):
                reject('non_canonical','unsorted or duplicate map key')
            if type(k) not in (str,int,bytes):reject('non_canonical','unsupported map key')
            if k in out:reject('non_canonical','duplicate semantic key')
            out[k]=self.value(depth+1);last=rawkey
        return out

def decode(raw):
    r=Reader(raw);v=r.value()
    if r.i!=len(raw):reject('non_canonical','trailing bytes')
    if encode(v)!=raw:reject('non_canonical','noncanonical bytes')
    return v

def diagnostic(value):
    if type(value) is bytes:return {'$bytes':value.hex()}
    if type(value) is list:return [diagnostic(x) for x in value]
    if type(value) is dict:return {str(k):diagnostic(v) for k,v in value.items()}
    return value

def from_diagnostic(value):
    if type(value) is list:return [from_diagnostic(x) for x in value]
    if type(value) is dict:
        if set(value)=={'$bytes'}:return bytes.fromhex(value['$bytes'])
        return {int(k) if isinstance(k,str) and k.isdecimal() else k:from_diagnostic(v) for k,v in value.items()}
    return value

def check_type(v,t,path,wave):
    if v is None:reject('null_forbidden',path)
    if 'uint' in t:
        low,high=t['uint']
        if type(v) is not int or v<low or (high is not None and v>high):reject('value_range',path)
    elif 'bool' in t:
        if type(v) is not bool:reject('value_range',path)
    elif 'bstr' in t:
        low,high=t['bstr']
        if type(v) is not bytes or not low<=len(v)<=high:reject('value_range',path)
    elif 'tstr' in t:
        low,high=t['tstr']
        if type(v) is not str or not low<=len(v.encode('utf-8'))<=high:reject('value_range',path)
    elif 'enum' in t:
        en=t['enum'];allowed=FIELDS['enums'][en]
        if type(v) is not int or str(v) not in allowed:reject('value_range',path+' unregistered enumeration')
        if int(FIELDS['enum_value_waves'].get(en,{}).get(str(v),'W0')[1:])>int(wave[1:]):
            reject('unknown_field',path+' later-wave enum')
    elif 'ref' in t:check_struct(v,t['ref'],wave)
    elif 'set' in t or 'array' in t:
        kind='set' if 'set' in t else 'array';low,high=t['n']
        if type(v) is not list or not low<=len(v)<=high:reject('value_range',path+' array count')
        for idx,el in enumerate(v):check_type(el,t[kind],f'{path}[{idx}]',wave)
        if kind=='set':
            codes=[encode(x) for x in v]
            if any(codes[i-1]>=codes[i] for i in range(1,len(codes))):
                reject('set_order',path+' unsorted/duplicate set')
    elif 'ext' in t:
        if type(v) is not dict or not 1<=len(v)<=128:reject('value_range',path+' extension count')
        for key,item in v.items():
            if type(key) is not str or not 1<=len(key.encode())<=128:reject('value_range','extension id')
            check_struct(item,'ExtEntry',wave)
            if key.startswith('om.critical.') or item[0]:reject('unknown_field','unsupported critical extension')
    else:reject('value_range','unrecognized registry descriptor')

def check_struct(v,name,wave='W4'):
    if type(v) is not dict or any(type(k) is not int or k<0 for k in v):
        reject('value_range',name+' must be uint-keyed map')
    spec=FIELDS['structs'][name];defs={x['key']:x for x in spec['fields']}
    allowed=set(defs)|({0,1,23} if spec.get('top_level') else set())
    if set(v)-allowed:reject('unknown_field',f'{name} unknown keys {set(v)-allowed}')
    if spec.get('top_level'):
        if v.get(0)!=TYPES[name]['type_id'] or v.get(1)!=1:reject('value_range','wrong object tag/version')
        if 23 in v:check_type(v[23],{'ext':None},name+'.extensions',wave)
    for key,field in defs.items():
        if key not in v:
            if field['req']:reject('missing_field',name+'.'+field['name'])
            continue
        if int(field.get('wave','W0')[1:])>int(wave[1:]):reject('unknown_field','future wave field')
        check_type(v[key],field['type'],name+'.'+field['name'],wave)

def digest(name,body):
    label=TYPES[name]['hash']
    if label is None:reject('value_range','no hash label')
    return {0:1,1:hashlib.sha256(encode([label,encode(body)])).digest()}

def confirmation_digest(call):
    if call.get(9) is None:reject('missing_field','confirmation needs exact destination')
    raw=encode([call[2],call[3],call[4],call[5],call[7],call[8],call[9][1]])
    return {0:1,1:hashlib.sha256(encode(['OM-PLUGIN-CONFIRMATION-HASH-v1',raw])).digest()}

def signature(role=1, signer=b'\x11'*16):
    return {0:signer,1:1,2:role,3:b'\x22'*64} # STRUCTURAL FAKE, not cryptography

def wrap(name,body,signatures=None):
    if TYPES[name]['signed'] is None:return encode(body)
    sigs=signatures if signatures is not None else [signature(role=3 if name=='PluginCapabilityGrant' else 1,
                                                            signer=body.get(8,b'\x11'*16) if name=='PluginCapabilityGrant' else b'\x11'*16)]
    return encode({1:encode(body),2:sorted(sigs,key=encode)})

def same_digest(x,y):return type(x) is dict and x==y

def check_chain(name,b,prior):
    versions={'PluginControlState':(3,4),'PluginCapabilityGrant':(12,13)}
    if name not in versions:return
    verkey,prevkey=versions[name];v=b[verkey]
    if v==1 and prevkey in b:reject('forked_chain','genesis may not have predecessor')
    if v>1 and prevkey not in b:reject('missing_field','non-genesis requires predecessor')
    if prior is None:return
    check_struct(prior,name)
    if v<=prior[verkey]:
        if v==prior[verkey] and b!=prior:reject('forked_chain','same version alternate state')
        reject('stale_chain','rollback/replay')
    if v!=prior[verkey]+1:reject('stale_chain','skip version')
    if b[prevkey]!=digest(name,prior):reject('forked_chain','incorrect previous hash')
    if name=='PluginCapabilityGrant':
        if any(b[k]!=prior[k] for k in (2,3,4,5,8,9,11)):
            reject('forked_chain','grant incarnation/scope/issuer mutated')
        if b[6]!=prior[6] or b[7]!=prior[7]:reject('forked_chain','grant temporal expansion')
        if prior[10]==2 and b[10]!=2:reject('stale_chain','grant resurrection')
    if name=='PluginControlState':
        if b[2]!=prior[2]:reject('forked_chain','plugin ID change')
        oldkeys={x[0]:x for x in prior[5]};newkeys={x[0]:x for x in b[5]}
        for kid,old in oldkeys.items():
            if kid not in newkeys:reject('stale_chain','publisher key tombstone removed')
            new=newkeys[kid]
            if new[1]!=old[1] or new[2]!=old[2]:reject('forked_chain','publisher key reuse')
            if old[3]==2 and new[3]!=2:reject('stale_chain','publisher key resurrected')
        oldreleases={x[0]:x for x in prior[7]};newreleases={x[0]:x for x in b[7]}
        for seq,old in oldreleases.items():
            if seq not in newreleases:reject('stale_chain','release tombstone removed')
            new=newreleases[seq]
            if any(new.get(k)!=old.get(k) for k in (0,1,2,3,4,6,7)):
                reject('forked_chain','immutable release edited')
            if old[5]==2 and new[5]!=2:reject('stale_chain','revoked release resurrected')
        if max(newreleases)>max(oldreleases)+1:reject('stale_chain','release sequence jump')
        for seq in set(newreleases)-set(oldreleases):
            if seq!=max(oldreleases)+1:reject('stale_chain','release must append')
            old=oldreleases[max(oldreleases)];new=newreleases[seq]
            if new.get(7)!=old[1]:reject('forked_chain','release predecessor mismatch')
            if not set(new[4])<=set(old[4]):reject('capability_escalation','new continuity ceiling exceeds preceding release')

def special(name,b,*,sigs=None,prior=None,prior_release=None,prior_release_record=None,control=None,manifest=None,pending_call=None,grants=(),now=None,tainted=False,policy=None,
            channel=None,origin_bound=False,receiver_wave='W4'):
    if name=='PluginPackageManifest':
        if b[3]==1 and 5 in b:reject('forked_chain','genesis release has predecessor')
        if b[3]>1 and 5 not in b:reject('missing_field','release predecessor absent')
        comps=b[6];ids=[x[1] for x in comps]
        if len(ids)!=len(set(ids)):reject('set_order','duplicate component name')
        ent=b[8];entry=[(x[0],x[1]) for x in ent]
        if len(entry)!=len(set(entry)):reject('set_order','duplicate entrypoint')
        limit=[x[0] for x in b[11]]
        if len(limit)!=len(set(limit)):reject('set_order','duplicate resource kind')
        if 14 not in b[9] and b[10]:reject('value_range','network origins requested without network capability')
        if prior_release is not None:
            check_struct(prior_release,'PluginPackageManifest')
            if b[2]!=prior_release[2] or b[3]!=prior_release[3]+1:
                reject('stale_chain','manifest rollback/skipped release')
            if b[5]!=digest('PluginPackageManifest',prior_release):reject('forked_chain','manifest predecessor mismatch')
        if control is not None:
            check_struct(control,'PluginControlState')
            if control[2]!=b[2]:reject('forked_chain','manifest/control plugin mismatch')
            releases=[r for r in control[7] if r[0]==b[3]]
            if len(releases)!=1 or releases[0][5]!=1:reject('release_not_authorized','unknown/revoked release')
            r=releases[0]
            if r[1]!=digest(name,b) or r[2]!=digest(name,b):reject('release_not_authorized','manifest hash mismatch')
            if r[3]!=b[9] or r[6]!=b[4]:reject('release_not_authorized','manifest contents mismatch')
    if name=='PluginControlState':
        if b[6]>sum(x[3]==1 for x in b[5]):reject('value_range','threshold exceeds active keys')
        if len({x[0] for x in b[5]})!=len(b[5]):reject('set_order','duplicate publisher key id')
        releases=b[7];seq=[r[0] for r in releases]
        if len(set(seq))!=len(seq):reject('set_order','duplicate release sequence')
        if sorted(seq)!=list(range(1,max(seq)+1)):reject('stale_chain','release sequence gap')
        if len({r[1][1] for r in releases})!=len(releases):reject('set_order','duplicate release digest')
        for r in releases:
            if r[1]!=r[2]:reject('value_range','release id must equal manifest hash')
            if not set(r[4])<=set(r[3]):reject('capability_escalation','continuity ceiling not declared')
            if r[0]==1 and 7 in r:reject('forked_chain','release genesis has previous id')
            if r[0]>1 and 7 not in r:reject('missing_field','release missing predecessor id')
        releases_by_seq={r[0]:r for r in releases}
        for idx in range(2,max(seq)+1):
            new=releases_by_seq[idx];old=releases_by_seq[idx-1]
            if new[7]!=old[1]:reject('forked_chain','release chain predecessor wrong')
            if not set(new[4])<=set(old[4]):reject('capability_escalation','auto ceiling silently expanded')
    if name=='PluginCapabilityGrant':
        if b[7]<=b[6]:reject('value_range','expiry must exceed issued_at')
        scope=b[5]
        if len(scope[1])!={1:16,2:32,3:16}[scope[0]]:reject('value_range','scope ID length')
        if scope[0]==1 and scope[1]!=b[2]:reject('value_range','personal scope installation mismatch')
        if now is not None and (now<b[6] or now>=b[7]):reject('grant_expired','grant not currently valid')
        if b[10]!=1 and prior is None:pass # revocation tombstone can decode
        if sigs and any(s[0]!=b[8] for s in sigs):reject('wrong_role','grant must be issued by named device')
    if name=='PluginCapabilityCall':
        scope=b[7]
        if len(scope[1])!={1:16,2:32,3:16}[scope[0]]:reject('value_range','scope type-id mismatch')
        eg=b.get(9);cap=b[5]
        if eg is not None:
            if cap not in (11,14,17):reject('value_range','egress request only on send/network/service')
            if eg[0]==3 and 3 not in eg:reject('missing_field','one-shot requires selected context')
            if eg[0] in (2,4) and 2 not in eg:reject('missing_field','policy/export egress requires conversation kind')
        if cap in (14,17) and eg is None:reject('missing_field','outbound capability must declare exact egress intent')
        if cap in (14,17) and eg[0]==1:reject('value_range','outbound capability cannot claim no egress')
        if manifest is not None:
            check_struct(manifest,'PluginPackageManifest')
            if manifest[2]!=b[2] or cap not in manifest[9]:
                reject('capability_escalation','capability not declared by installed release')
        if grants is not None:
            matching=[];expired=False
            for g in grants:
                check_struct(g,'PluginCapabilityGrant')
                if (g[2],g[3],g[4],g[5],digest('PluginCapabilityGrant',g))==(
                     b[3],b[2],cap,b[7],b[6]):
                    if g[10]!=1 or (now is not None and not g[6]<=now<g[7]):expired=True
                    else:matching.append(g)
            if not matching:reject('grant_expired' if expired else 'capability_not_granted','missing active bound capability grant')
            if now is None:reject('grant_expired','capability broker requires trusted current time')
            if prior_release is not None and manifest is not None:
                if prior_release_record is None:reject('missing_field','prior continuity ceiling is required for update')
                check_struct(prior_release_record,'PluginReleaseRecord')
                if prior_release_record[1]!=digest('PluginPackageManifest',prior_release):
                    reject('forked_chain','prior continuity ceiling not from prior release')
                if cap not in prior_release_record[4]:
                    if not any(g[9]==digest('PluginPackageManifest',manifest) for g in matching):
                        reject('capability_escalation','capability outside prior ceiling needs newly release-approved device grant')
        if tainted and cap in (14,17):
            if policy is None or not policy.get('allow',False):reject('tainted_egress_denied','plaintext-tainted outbound channel')
            if policy.get('conversation_kind')==1 and eg[0] in (2,4) and not policy.get('both_consented',False):
                reject('bilateral_consent_required','1:1 requires both participants')
            if eg[0]==3 and not policy.get('per_action_confirmed',False):
                reject('user_confirmation_required','selected context per-action confirmation')
            if eg[0]==4 and not policy.get('export_authorized',False):
                reject('tainted_egress_denied','continuous export requires bridge authorization')
        if cap==11 and b[7][0]!=2:reject('value_range','send requires conversation-scoped grant')
        if cap==11 and 9 not in b:reject('missing_field','send requires exact destination in egress_request')
        if cap==11 and b[9][0]!=1:reject('value_range','send is confirmed account action, not generic egress')
        if cap==11 and (policy is None or not policy.get('per_action_confirmed',False)):
            reject('user_confirmation_required','every send requires host exact-content user confirmation')
    if name=='PluginUiTreeUpdate':
        ids=[n[0] for n in b[6]]
        if len(set(ids))!=len(ids):reject('value_range','duplicate UI node id')
        nodes={n[0]:n for n in b[6]}
        roots=0
        for n in b[6]:
            curr=n;seen=set();depth=0
            if 2 not in n:roots+=1
            while 2 in curr:
                if curr[0] in seen:reject('value_range','UI cycle')
                seen.add(curr[0]);depth+=1
                if depth>8:reject('value_range','UI depth>8')
                if curr[2] not in nodes:reject('value_range','UI absent parent')
                curr=nodes[curr[2]]
        if roots!=1:reject('value_range','UI must have one host-bound root')
    if name in ('PluginHostRequest','PluginHostResponse'):
        if channel is not None and b[4]!=channel:reject('origin_channel_mismatch','unbound channel identifier')
        if not origin_bound:reject('origin_channel_mismatch','origin and channel not verified externally')
        kinds={1:'PluginCapabilityCall',2:'PluginUiTreeUpdate',3:'PluginEvent'} if name=='PluginHostRequest' else {1:'PluginCallResult',2:'PluginHostPolicyDecision',3:'PluginError'}
        inner=kinds[b[7]]
        if inner in PLUGIN_NAMES:
            peek=decode(b[8])
            if type(peek) is not dict or peek.get(0)!=TYPES[inner]['type_id']:
                reject('value_range','envelope payload type differs from discriminator')
            x=validate_wire(inner,b[8],receiver_wave=receiver_wave,manifest=manifest,prior_release=prior_release,prior_release_record=prior_release_record,grants=grants,now=now,tainted=tainted,policy=policy,
                            channel=channel,origin_bound=origin_bound)
            if inner!='PluginError' and (x[2]!=b[2] or x[3]!=b[3] or x[4]!=b[5]):reject('value_range','nested identity/invocation mismatch')
            if inner=='PluginError' and x[2]!=b[5]:reject('value_range','nested error invocation mismatch')
        else:
            x=decode(b[8]);check_struct(x,inner,receiver_wave)
            if x[0]!=b[5]:reject('value_range','nested response invocation mismatch')
            if inner=='PluginHostPolicyDecision':
                if x[4] and 8 not in x:reject('missing_field','confirmed action must bind exact bytes')
                if x[4]:
                    if pending_call is None:reject('missing_field','confirmed action needs trusted pending call')
                    check_struct(pending_call,'PluginCapabilityCall')
                    if (x[8]!=confirmation_digest(pending_call) or x[0]!=pending_call[4]
                            or x[1]!=pending_call[5]):
                        reject('confirmation_mismatch','host confirmation does not bind exact pending send')
                if x[3] and x[2] and x[6] in (2,4) and 9 not in x:
                    reject('missing_field','permitted tainted egress requires policy evidence reference')
                if x[3] and x[2] and x[6] in (2,4) and x.get(7)==1 and not x[5]:
                    reject('bilateral_consent_required','host policy decision lacks both participants')
                if x[3] and x[1]==11 and not x[4]:reject('user_confirmation_required','host may not say send allowed unconfirmed')
                if x[3] and x[2] and x[6]==3 and not x[4]:reject('user_confirmation_required','one-shot requires confirmation')
            if inner=='PluginCallResult' and (policy is None or not policy.get('host_result_authorized',False)):
                # The authenticated host is the only party allowed to emit a result.
                pass

def validate_body(name,body,*,sigs=None,receiver_wave='W4',prior=None,prior_release=None,prior_release_record=None,control=None,
                  manifest=None,pending_call=None,grants=(),now=None,tainted=False,policy=None,channel=None,origin_bound=False):
    if name not in PLUGIN_NAMES:reject('value_range','plugin objects only')
    if receiver_wave!='W4':reject('w0_receiver_rejection','W4 top-level type unknown to W0')
    check_struct(body,name,receiver_wave)
    check_chain(name,body,prior)
    if sigs is not None:
        signed=TYPES[name]['signed']
        allowed={int(k) for k,v in FIELDS['enums']['sig_role'].items() if v in signed['roles']}
        if any(s[2] not in allowed for s in sigs):reject('wrong_role','wrong signature role')
        if len({s[0] for s in sigs})!=len(sigs):reject('wrong_role','duplicate signer identity')
        if name in ('PluginPackageManifest','PluginControlState'):
            trusted=prior if name=='PluginControlState' else control
            if trusted is not None:
                active={k[0] for k in trusted[5] if k[3]==1}
                if any(s[0] not in active for s in sigs):reject('wrong_role','publisher signer not active')
                if len(sigs)<trusted[6]:reject('wrong_role','publisher threshold not reached')
    special(name,body,sigs=sigs,prior=prior,prior_release=prior_release,prior_release_record=prior_release_record,control=control,manifest=manifest,pending_call=pending_call,grants=grants,now=now,
            tainted=tainted,policy=policy,channel=channel,origin_bound=origin_bound,receiver_wave=receiver_wave)
    return body

def validate_wire(name,wire,*,receiver_wave='W4',prior=None,prior_release=None,prior_release_record=None,control=None,
                  manifest=None,pending_call=None,grants=(),now=None,tainted=False,policy=None,channel=None,origin_bound=False):
    if name not in PLUGIN_NAMES:reject('value_range','unknown plugin name')
    if len(wire)>TYPES[name]['max_bytes']:reject('max_bytes',name+' exceeds object cap')
    if receiver_wave!='W4':reject('w0_receiver_rejection','unknown type 59..67')
    x=decode(wire)
    sigs=None
    if TYPES[name]['signed']:
        check_struct(x,'SignedEnvelope',receiver_wave)
        sigs=x[2]
        enc=[encode(s) for s in sigs]
        if enc!=sorted(enc) or len(enc)!=len(set(enc)):reject('set_order','signature sort/duplicate')
        x=decode(x[1])
    return validate_body(name,x,sigs=sigs,receiver_wave=receiver_wave,prior=prior,prior_release=prior_release,prior_release_record=prior_release_record,
                         control=control,manifest=manifest,pending_call=pending_call,grants=grants,now=now,tainted=tainted,policy=policy,
                         channel=channel,origin_bound=origin_bound)

if __name__=='__main__':
    import argparse
    p=argparse.ArgumentParser();p.add_argument('object',choices=PLUGIN_NAMES);p.add_argument('wire_hex');p.add_argument('--w0',action='store_true')
    a=p.parse_args()
    try:print(json.dumps(diagnostic(validate_wire(a.object,bytes.fromhex(a.wire_hex),receiver_wave='W0' if a.w0 else 'W4')),indent=2))
    except CodecError as e:p.error(str(e))
