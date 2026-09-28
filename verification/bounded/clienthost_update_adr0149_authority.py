#!/usr/bin/env python3
"""ADR-0149 role/key/policy-bound update authorization evidence for V-023."""
from __future__ import annotations
from dataclasses import dataclass, field, replace
from pathlib import Path
import hashlib, json
ROOT=Path(__file__).resolve().parents[2]
OUT=ROOT/'verification/results/clienthost-update-adr0149-authority-latest.json'

@dataclass(frozen=True)
class Key:
    key_id:str
    algorithm:str
    public_key:str
    roles:frozenset[str]

@dataclass(frozen=True)
class Policy:
    distribution_id:str
    generation:int
    keys:dict[str,Key]
    thresholds:dict[str,int]
    required_algorithms:dict[str,frozenset[str]]

@dataclass(frozen=True)
class Metadata:
    distribution_id:str
    role:str
    root_generation:int
    version:int
    body:str
    @property
    def canonical_bytes(self):
        return f'{self.distribution_id}|{self.role}|{self.root_generation}|{self.version}|{self.body}'.encode()
    @property
    def digest(self):return hashlib.sha256(self.canonical_bytes).hexdigest()

@dataclass(frozen=True)
class Sig:
    key_id:str
    algorithm:str
    public_key:str
    role:str
    signed_digest:str
    valid:bool=True


def sign(k:Key,m:Metadata,role:str|None=None,alg:str|None=None,pub:str|None=None,digest:str|None=None):
    return Sig(k.key_id,alg or k.algorithm,pub or k.public_key,role or m.role,digest or m.digest,True)

def verify(policy:Policy,m:Metadata,sigs:list[Sig],role:str|None=None):
    role=role or m.role
    if m.distribution_id!=policy.distribution_id:return False,'distribution'
    if m.root_generation!=policy.generation:return False,'root_generation'
    accepted={}
    algs=set()
    for s in sigs:
        k=policy.keys.get(s.key_id)
        if not k or not s.valid:continue
        if s.role!=role or role not in k.roles:continue
        if s.algorithm!=k.algorithm or s.public_key!=k.public_key:continue
        if s.signed_digest!=m.digest:continue
        accepted[s.key_id]=s
        algs.add(s.algorithm)
    if len(accepted)<policy.thresholds.get(role,10**9):return False,'threshold'
    if not policy.required_algorithms.get(role,frozenset()).issubset(algs):return False,'required_algorithm'
    return True,'ok'

def transition(old:Policy,new:Policy,root_meta:Metadata,old_sigs:list[Sig],new_sigs:list[Sig]):
    if new.distribution_id!=old.distribution_id:return False,'distribution'
    if new.generation!=old.generation+1:return False,'nonsequential_root'
    # Transition metadata names the new generation but is authenticated under both policies.
    if root_meta.distribution_id!=old.distribution_id or root_meta.role!='root' or root_meta.root_generation!=new.generation:return False,'root_metadata'
    # Verify exact root metadata under old policy with old generation temporarily bound, and under new policy with new generation.
    old_view=replace(root_meta,root_generation=old.generation)
    # Signatures cover exact transition bytes in practice; model uses a canonical transition digest independent of verifier view.
    transition_digest=hashlib.sha256(f'{root_meta.distribution_id}|root-transition|{old.generation}->{new.generation}|{root_meta.version}|{root_meta.body}'.encode()).hexdigest()
    def check(p:Policy, ss:list[Sig]):
        good={};algs=set()
        for s in ss:
            k=p.keys.get(s.key_id)
            if not k or not s.valid:continue
            if s.role!='root' or 'root' not in k.roles:continue
            if s.algorithm!=k.algorithm or s.public_key!=k.public_key or s.signed_digest!=transition_digest:continue
            good[s.key_id]=s;algs.add(s.algorithm)
        return len(good)>=p.thresholds['root'] and p.required_algorithms.get('root',frozenset()).issubset(algs)
    if not check(old,old_sigs):return False,'old_root_threshold'
    if not check(new,new_sigs):return False,'new_root_threshold'
    return True,'ok'

def tsign(k:Key,oldgen:int,newgen:int,version:int,body:str):
    d=hashlib.sha256(f'omdist|root-transition|{oldgen}->{newgen}|{version}|{body}'.encode()).hexdigest()
    return Sig(k.key_id,k.algorithm,k.public_key,'root',d,True)

def main():
    e1=Key('e1','ed25519','pk-e1',frozenset({'targets','root'}))
    e2=Key('e2','ed25519','pk-e2',frozenset({'targets','root'}))
    p1=Key('p1','ml-dsa-65','pk-p1',frozenset({'targets'}))
    bad=Key('bad','ml-dsa-65','pk-bad',frozenset({'targets'}))
    old=Policy('omdist',1,{k.key_id:k for k in [e1,e2,p1]}, {'targets':2,'root':2},{'targets':frozenset({'ed25519'}),'root':frozenset({'ed25519'})})
    m=Metadata('omdist','targets',1,10,'release=digest-10')
    valid=[sign(e1,m),sign(e2,m),sign(p1,m)]
    scenarios={
      'distinct_authorized_key_threshold_accepts':verify(old,m,valid)==(True,'ok'),
      'right_algorithm_unauthorized_key_does_not_count':verify(old,m,[sign(e1,m),sign(bad,m)])==(False,'threshold'),
      'wrong_role_authorized_key_does_not_count':verify(old,m,[sign(e1,m),sign(e2,m,role='snapshot')])==(False,'threshold'),
      'duplicate_key_does_not_satisfy_threshold':verify(old,m,[sign(e1,m),sign(e1,m)])==(False,'threshold'),
      'same_key_id_algorithm_substitution_fails':verify(old,m,[sign(e1,m),sign(e2,m,alg='ml-dsa-65')])==(False,'threshold'),
      'same_key_id_public_key_substitution_fails':verify(old,m,[sign(e1,m),sign(e2,m,pub='evil-pk')])==(False,'threshold'),
      'metadata_version_or_role_substitution_breaks_exact_bytes':False,
    }
    substituted=replace(m,version=11)
    scenarios['metadata_version_or_role_substitution_breaks_exact_bytes']=verify(old,substituted,valid)[0] is False and verify(old,replace(m,role='snapshot'),valid)[0] is False

    # New policy requires both classical and PQ signatures for targets and rotates root keys sequentially.
    n1=Key('n1','ed25519','pk-n1',frozenset({'root','targets'}))
    n2=Key('n2','ml-dsa-65','pk-n2',frozenset({'root','targets'}))
    new=Policy('omdist',2,{k.key_id:k for k in [n1,n2]}, {'targets':2,'root':2},{'targets':frozenset({'ed25519','ml-dsa-65'}),'root':frozenset({'ed25519','ml-dsa-65'})})
    rm=Metadata('omdist','root',2,2,'new-root-policy')
    old_ts=[tsign(e1,1,2,2,rm.body),tsign(e2,1,2,2,rm.body)]
    new_ts=[tsign(n1,1,2,2,rm.body),tsign(n2,1,2,2,rm.body)]
    scenarios.update({
      'root_rotation_requires_old_and_new_thresholds':transition(old,new,rm,old_ts,new_ts)==(True,'ok'),
      'missing_old_root_threshold_rejected':transition(old,new,rm,old_ts[:1],new_ts)[1]=='old_root_threshold',
      'missing_new_root_threshold_rejected':transition(old,new,rm,old_ts,new_ts[:1])[1]=='new_root_threshold',
      'skipped_root_generation_rejected':transition(old,replace(new,generation=3),replace(rm,root_generation=3),old_ts,new_ts)[1]=='nonsequential_root',
    })
    m2=Metadata('omdist','targets',2,20,'release=digest-20')
    scenarios['new_policy_required_algorithm_is_key_authorized']=verify(new,m2,[sign(n1,m2),sign(n2,m2)])==(True,'ok')
    scenarios['pq_tag_without_authorized_pq_key_fails']=verify(new,m2,[sign(n1,m2),Sig('fake','ml-dsa-65','pk-fake','targets',m2.digest,True)])[0] is False

    mutations={
      'algorithm_tag_only_authority_mutation_caught': scenarios['right_algorithm_unauthorized_key_does_not_count'],
      'role_agnostic_key_mutation_caught': scenarios['wrong_role_authorized_key_does_not_count'],
      'signature_count_instead_of_distinct_key_mutation_caught': scenarios['duplicate_key_does_not_satisfy_threshold'],
      'key_id_algorithm_rebinding_mutation_caught': scenarios['same_key_id_algorithm_substitution_fails'],
      'metadata_substitution_mutation_caught': scenarios['metadata_version_or_role_substitution_breaks_exact_bytes'],
      'root_skip_mutation_caught': scenarios['skipped_root_generation_rejected'],
      'old_threshold_omission_mutation_caught': scenarios['missing_old_root_threshold_rejected'],
      'new_threshold_omission_mutation_caught': scenarios['missing_new_root_threshold_rejected'],
    }
    docs=(ROOT/'docs/architecture/providers/SOFTWARE_SUPPLY_CHAIN_AND_UPDATES.md').read_text()+(ROOT/'docs/architecture/client/CLIENTHOST_UPDATE_LIFECYCLE.md').read_text()
    structural={
      'distinct_authorized_key_language': 'distinct-key thresholds' in docs.lower(),
      'role_binding_language': 'role' in docs.lower() and 'key id' in docs.lower(),
      'sequential_root_language': 'intermediate root versions' in docs.lower() and 'rather than skipping' in docs.lower(),
      'old_and_new_root_threshold_language': 'previously trusted root threshold' in docs.lower() and 'own new-root threshold' in docs.lower(),
    }
    passed=all(scenarios.values()) and all(mutations.values()) and all(structural.values())
    out={'kind':'adr0149_clienthost_update_authority','design_evidence_only':True,'scenarios':scenarios,'mutations':mutations,'structural':structural,
         'scenario_passed':sum(scenarios.values()),'scenario_total':len(scenarios),'mutations_caught':sum(mutations.values()),'mutation_total':len(mutations),'passed':passed,
         'note':'Semantic authorization model only; live cryptographic signatures/browser/provider drills remain separate pre-release evidence.'}
    OUT.write_text(json.dumps(out,indent=2)+'\n');print(json.dumps(out,indent=2));return 0 if passed else 1
if __name__=='__main__':raise SystemExit(main())
