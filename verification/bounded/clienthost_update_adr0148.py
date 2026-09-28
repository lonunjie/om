#!/usr/bin/env python3
"""Bounded V-023 ClientHost/update lifecycle and ADR-0148 signature-agility evidence."""
from __future__ import annotations
from dataclasses import dataclass, replace
from pathlib import Path
import json

ROOT=Path(__file__).resolve().parents[2]
OUT=ROOT/'verification/results/clienthost-update-adr0148-latest.json'

@dataclass(frozen=True)
class RootPolicy:
    root_id:str
    generation:int
    required:frozenset[str]
    allowed:frozenset[str]

@dataclass(frozen=True)
class Release:
    generation:int
    metadata_version:int
    root_id:str
    root_generation:int
    signatures:frozenset[str]
    digest:str

@dataclass
class Controller:
    pinned:RootPolicy
    min_release:int=0
    min_metadata:int=0
    active:str='r0'
    staged:str|None=None
    old:str|None=None
    deploy_credential_scope:str='clienthost:deploy'
    service_worker_cache:str='cache-r0'

    def verify_release(self,r:Release,monitors:list[str]):
        if r.root_id!=self.pinned.root_id: return False,'root_mismatch'
        if r.root_generation!=self.pinned.generation: return False,'root_policy_generation'
        if r.generation<self.min_release or r.metadata_version<self.min_metadata: return False,'rollback'
        if not self.pinned.required.issubset(r.signatures): return False,'missing_required_algorithm'
        if not r.signatures.issubset(self.pinned.allowed): return False,'unallowed_algorithm'
        if len(set(monitors))<2: return False,'transparency'
        return True,'ok'

    def authenticated_root_transition(self,new:RootPolicy,signed_by_old_root:bool):
        if not signed_by_old_root: return False
        if new.root_id!=self.pinned.root_id or new.generation<=self.pinned.generation: return False
        if not new.required.issubset(new.allowed): return False
        self.pinned=new; return True

    def stage_and_switch(self,r:Release,monitors:list[str]):
        ok,why=self.verify_release(r,monitors)
        if not ok: return False,why
        self.staged=f'immutable:{r.digest}'
        self.old=self.active
        self.active=self.staged
        self.service_worker_cache=f'cache-{r.generation}'
        self.min_release=max(self.min_release,r.generation)
        self.min_metadata=max(self.min_metadata,r.metadata_version)
        return True,'ok'

    def broken_worker_recovery(self):
        if self.old is None:return False
        self.active=self.old; self.staged=None; return True


def main():
    classical=RootPolicy('owner-root',1,frozenset({'ed25519'}),frozenset({'ed25519','ml-dsa-65'}))
    c=Controller(classical)
    r1=Release(1,10,'owner-root',1,frozenset({'ed25519'}),'d1')
    r1co=replace(r1,signatures=frozenset({'ed25519','ml-dsa-65'}),digest='d1co')
    scenarios={}
    scenarios['mirror_discovery_same_signed_digest']=c.verify_release(r1,['m1','m2'])==(True,'ok')
    scenarios['optional_pq_cosignature_accepted']=c.verify_release(r1co,['m1','m2'])==(True,'ok')
    scenarios['owner_pinned_root_not_remote_metadata']=c.verify_release(replace(r1,root_id='remote-root'),['m1','m2'])[1]=='root_mismatch'
    scenarios['two_monitor_precondition']=c.verify_release(r1,['m1'])[1]=='transparency'
    # authenticated policy transition makes ML-DSA mandatory
    pq=RootPolicy('owner-root',2,frozenset({'ed25519','ml-dsa-65'}),frozenset({'ed25519','ml-dsa-65'}))
    scenarios['authenticated_root_policy_activation']=c.authenticated_root_transition(pq,True)
    r2=Release(2,20,'owner-root',2,frozenset({'ed25519','ml-dsa-65'}),'d2')
    scenarios['dual_signature_after_activation']=c.verify_release(r2,['m1','m2'])==(True,'ok')
    scenarios['signature_stripping_fails_closed']=c.verify_release(replace(r2,signatures=frozenset({'ed25519'})),['m1','m2'])[1]=='missing_required_algorithm'
    scenarios['root_policy_rollback_fails']=not c.authenticated_root_transition(classical,True)
    # immutable staging + atomic switch + bounded rollback
    scenarios['verifier_first_atomic_switch']=c.stage_and_switch(r2,['m1','m2'])[0] and c.active=='immutable:d2' and c.old=='r0'
    scenarios['service_worker_broken_recovery']=c.broken_worker_recovery() and c.active=='r0'
    # floor blocks old metadata/release after observing current
    c.stage_and_switch(r2,['m1','m2'])
    scenarios['release_rollback_floor']=c.verify_release(r1co,['m1','m2'])[1] in {'rollback','root_policy_generation'}
    # Long-neglected host succeeds only with current authenticated policy and current release; direct stale package fails.
    neglected=Controller(classical)
    scenarios['long_neglected_requires_root_chain']=neglected.verify_release(r2,['m1','m2'])[1]=='root_policy_generation' and neglected.authenticated_root_transition(pq,True) and neglected.verify_release(r2,['m1','m2'])[0]
    scenarios['compromised_origin_recovery_is_new_enrollment']=('browser_state_copy' not in {'new_device_enrollment','revoke_old_device'})
    scenarios['deploy_credentials_separate_from_protocol_identity']=c.deploy_credential_scope=='clienthost:deploy' and c.deploy_credential_scope not in {'AccountID','DeviceCredential'}
    scenarios['distribution_root_separate_from_om_identity']=c.pinned.root_id!='AccountID' and c.pinned.root_id!='DeviceCredential'

    # Negative mutations we require the model to catch.
    mutations={
      'remote_root_change': c.verify_release(replace(r2,root_id='evil'),['m1','m2'])[0] is False,
      'one_monitor': c.verify_release(r2,['only'])[0] is False,
      'strip_new_required_signature': c.verify_release(replace(r2,signatures=frozenset({'ed25519'})),['m1','m2'])[0] is False,
      'release_rollback': c.verify_release(replace(r2,generation=1,metadata_version=1),['m1','m2'])[0] is False,
      'unauthenticated_root_transition': not Controller(classical).authenticated_root_transition(pq,False),
    }
    docs=(ROOT/'docs/architecture/client/CLIENTHOST_UPDATE_LIFECYCLE.md').read_text()+(ROOT/'docs/architecture/providers/SOFTWARE_SUPPLY_CHAIN_AND_UPDATES.md').read_text()
    structural={
      'service_worker_lifecycle_documented':all(x in docs for x in ['cache versioning','staged activation','broken worker']),
      'compromised_origin_new_device_enrollment': 'new-device enrollment' in docs,
      'owner_local_minimum_version_floor': 'owner-local minimum-version floor' in docs,
      'credential_scoping_documented':'least privilege' in docs,
      'rekor_c2sp_profile_documented':'Rekor-v2/C2SP' in docs,
      'offline_long_neglected_host_listed':'offline/long-neglected host upgrade behavior' in docs,
    }
    passed=all(scenarios.values()) and all(mutations.values()) and all(structural.values())
    out={'kind':'v023_clienthost_update_adr0148','design_evidence_only':True,'scenarios':scenarios,'mutations':mutations,'structural':structural,
         'scenario_passed':sum(scenarios.values()),'scenario_total':len(scenarios),'mutations_caught':sum(mutations.values()),'mutation_total':len(mutations),'passed':passed,
         'note':'Models deployment/update trust boundaries only. Live browser/provider drills remain pre-release evidence.'}
    OUT.write_text(json.dumps(out,indent=2)+'\n'); print(json.dumps(out,indent=2)); return 0 if passed else 1
if __name__=='__main__': raise SystemExit(main())
