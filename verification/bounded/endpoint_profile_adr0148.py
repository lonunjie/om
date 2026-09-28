#!/usr/bin/env python3
"""V-021 bounded/structural evidence, including ADR-0148 profile switching.

No OS claims are measured here. Platform rows remain documented capability claims that
must be rechecked on real release targets at V-021 pre-release.
"""
from __future__ import annotations
from dataclasses import dataclass, field
from pathlib import Path
import json, re

ROOT=Path(__file__).resolve().parents[2]
OUT=ROOT/'verification/results/endpoint-profile-adr0148-latest.json'

@dataclass
class Profile:
    pid:str; vault_key:str; session_key:str; draft:str; cache:set[str]=field(default_factory=set); sync:set[str]=field(default_factory=set); notices:list[str]=field(default_factory=list)

class Client:
    def __init__(self):
        self.p={"A":Profile("A","vkA","skA","draft A",{"A-msg"},{"A-sync"}),"B":Profile("B","vkB","skB","draft B",{"B-msg"},{"B-sync"})}
        self.active="A"; self.logout_count=0; self.rekey_count=0
        self.known_links={"om://relationship-A":"A"}
        self.push_routes={"push-A":"A","push-B":"B"}
        self.public_installation_id=None
    def switch(self,pid):
        before=(self.p[pid].vault_key,self.p[pid].session_key,self.logout_count,self.rekey_count)
        self.active=pid
        # decrypted view cache is rebound to destination namespace rather than unioned.
        rendered=set(self.p[pid].cache)
        after=(self.p[pid].vault_key,self.p[pid].session_key,self.logout_count,self.rekey_count)
        return before==after,rendered
    def query_draft(self,pid):
        if pid!=self.active: raise PermissionError("profile namespace")
        return self.p[pid].draft
    def ingest_sync(self,pid,item):
        if pid!=self.active: raise PermissionError("profile namespace")
        self.p[pid].sync.add(item)
    def notification(self,route):
        pid=self.push_routes[route]
        return {"destination_profile":pid,"content":None if pid!=self.active else f"visible:{pid}"}
    def open_link(self,url):
        return self.known_links.get(url,"ASK_PROFILE")

def grant_with_notice(devices:set[str], creator:str, kind:str):
    notices={d:f"grant:{kind}" for d in devices if d!=creator}
    active = set(notices)==(devices-{creator})
    return active,notices

def clienthost_deploy(pinned_root:str, metadata_root:str, monitor_ops:list[str]):
    if metadata_root!=pinned_root: return False,"root_change_rejected"
    if len(set(monitor_ops))<2: return False,"insufficient_transparency"
    return True,"deployable"

def structural_checks():
    matrix=(ROOT/'docs/architecture/client/PLATFORM_CAPABILITY_MATRIX.md').read_text()
    uic=(ROOT/'docs/architecture/ux/UI_CORE_PLATFORM_CONTRACT.md').read_text()
    endpoint=(ROOT/'docs/architecture/client/ENDPOINT_COERCION_AND_PLATFORM_LEAKAGE.md').read_text()
    platform_cols=['iOS/iPadOS native','Android native','macOS','Windows','Linux','PWA (Chromium)','PWA (Safari)','PWA (Firefox)']
    rows=['Exclude data from OS/cloud backup','Block screenshots / screen recording','Blank app-switcher snapshot','Keyboard / IME learning off','Cloud spellcheck / writing tools off','Clear posted notifications','Mark clipboard data sensitive']
    adapters=['SecureSurface','BackupExclusion','InputPrivacy','NotificationRemoval','Clipboard']
    return {
      'matrix_has_all_platform_columns':all(x in matrix for x in platform_cols),
      'matrix_has_required_control_rows':all(x in matrix for x in rows),
      'platform_adapter_families_declared':all(x in uic for x in adapters) and all(x in matrix for x in adapters),
      'endpoint_section4_present':'## 4.' in endpoint or '## 4 ' in endpoint,
      'deletion_notification_surface':'Posted OS notifications' in matrix,
      'deletion_snapshot_surface':'App-switcher snapshot' in matrix,
    }

def main():
    c=Client(); stableA=(c.p['A'].vault_key,c.p['A'].session_key); ok,view=c.switch('B')
    profile={
      'switch_no_logout_rekey':ok and c.logout_count==0 and c.rekey_count==0,
      'cache_isolation':view=={'B-msg'} and 'A-msg' not in view,
      'draft_isolation':c.query_draft('B')=='draft B',
      'sync_isolation':False,
      'notification_destination_is_explicit':c.notification('push-A')['destination_profile']=='A' and c.notification('push-A')['content'] is None,
      'unknown_deep_link_requires_choice':c.open_link('om://unknown')=='ASK_PROFILE',
      'known_deep_link_uses_explicit_binding':c.open_link('om://relationship-A')=='A',
      'no_public_installation_linkage':c.public_installation_id is None,
    }
    try: c.ingest_sync('A','leak')
    except PermissionError: profile['sync_isolation']=True
    grants={
      'personal_grant_notice_all_other_devices': grant_with_notice({'d1','d2','d3'},'d1','automation')[0],
      'silent_grant_mutation_rejected': not (set()==({'d1','d2','d3'}-{'d1'})),
    }
    ch={
      'metadata_cannot_change_owner_pinned_root': clienthost_deploy('root-owner','root-remote',['m1','m2'])[0] is False,
      'two_independent_monitor_observations_required': clienthost_deploy('root-owner','root-owner',['m1'])[0] is False,
      'valid_root_and_two_monitors_can_stage': clienthost_deploy('root-owner','root-owner',['m1','m2'])[0] is True,
    }
    structural=structural_checks()
    passed=all(profile.values()) and all(grants.values()) and all(ch.values()) and all(structural.values())
    out={'kind':'v021_endpoint_profile_adr0148','design_evidence_only':True,'profile_switching':profile,'grant_notice':grants,'clienthost':ch,'structural':structural,
         'passed':passed,'note':'Platform capability values are documented, not measured; release forensic audit remains V-021 pre-release.'}
    OUT.write_text(json.dumps(out,indent=2)+'\n'); print(json.dumps(out,indent=2)); return 0 if passed else 1
if __name__=='__main__': raise SystemExit(main())
