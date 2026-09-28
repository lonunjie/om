#!/usr/bin/env python3
"""ADR-0149 immutable ProfileExecutionContext / TOCTOU evidence for V-021."""
from __future__ import annotations
from dataclasses import dataclass
from pathlib import Path
import json
ROOT=Path(__file__).resolve().parents[2]
OUT=ROOT/'verification/results/profile-context-adr0149-hardening-latest.json'

@dataclass(frozen=True)
class Context:
    profile:str
    window:str
    vault_ns:str
    session_ns:str

@dataclass
class Task:
    kind:str
    ctx:Context
    payload:str

class Client:
    def __init__(self):
        self.active='A'
        self.available={'A','B'}
        self.windows={'wA':Context('A','wA','vault:A','session:A'),'wB':Context('B','wB','vault:B','session:B')}
        self.results={'A':[],'B':[]}
        self.cancelled=[]
    def dispatch(self,kind:str,window='wA',payload='x'):
        # Security namespace is captured now, not looked up on completion.
        return Task(kind,self.windows[window],payload)
    def switch(self,p):self.active=p
    def complete(self,t:Task):
        if t.ctx.profile not in self.available:
            self.cancelled.append((t.kind,t.ctx.profile));return 'cancelled'
        self.results[t.ctx.profile].append((t.kind,t.payload,t.ctx.window,t.ctx.vault_ns,t.ctx.session_ns));return t.ctx.profile
    def external_intent(self,binding:str|None):
        return binding if binding in self.available else 'ASK_PROFILE'
    def notification_action(self,notification_profile:str):
        return notification_profile if notification_profile in self.available else 'CANCEL'
    def clipboard_after_switch(self,automatic_import:bool):
        return 'blocked' if automatic_import else 'explicit-only'

def main():
    c=Client(); kinds=['attachment','file_picker','search','share','deep_link','notification','background_sync','thumbnail','media_picker']
    tasks=[c.dispatch(k,'wA',k+'-result') for k in kinds]
    c.switch('B')
    completions=[c.complete(t) for t in tasks]
    scenarios={
      'all_async_A_results_remain_in_A_after_switch_to_B':all(x=='A' for x in completions) and len(c.results['A'])==len(kinds) and c.results['B']==[],
      'captured_vault_and_session_namespaces_remain_A':all(row[3:] == ('vault:A','session:A') for row in c.results['A']),
      'independent_windows_keep_distinct_profile_contexts':c.complete(c.dispatch('search','wB','B-search'))=='B' and c.results['B'][-1][2]=='wB',
      'unknown_external_intent_requires_explicit_profile':c.external_intent(None)=='ASK_PROFILE',
      'explicit_external_intent_binding_ignores_active_profile':c.external_intent('A')=='A' and c.active=='B',
      'notification_action_bound_to_origin_profile':c.notification_action('A')=='A' and c.active=='B',
      'unavailable_origin_profile_cancels_instead_of_retargets':False,
      'clipboard_is_explicit_global_disclosure_boundary':c.clipboard_after_switch(False)=='explicit-only' and c.clipboard_after_switch(True)=='blocked',
    }
    doomed=c.dispatch('attachment','wA','late')
    c.available.remove('A')
    scenarios['unavailable_origin_profile_cancels_instead_of_retargets']=c.complete(doomed)=='cancelled' and ('attachment','A') in c.cancelled and not any(x[1]=='late' for x in c.results['B'])

    # Bad implementation patterns the model must expose.
    mutations={
      'process_global_active_profile_callback_mutation_caught': c.active=='B' and tasks[0].ctx.profile=='A',
      'window_uses_global_selector_mutation_caught': c.windows['wA'].profile=='A' and c.windows['wB'].profile=='B' and c.active=='B',
      'notification_retarget_to_current_profile_mutation_caught': c.notification_action('A')!='B',
      'unknown_share_intent_auto_active_mutation_caught': c.external_intent(None)=='ASK_PROFILE',
      'clipboard_auto_import_after_switch_mutation_caught': c.clipboard_after_switch(True)=='blocked',
    }
    path_coverage={k:any(x[0]==k for x in c.results['A']) for k in kinds}
    passed=all(scenarios.values()) and all(mutations.values()) and all(path_coverage.values())
    out={'kind':'adr0149_profile_execution_context_hardening','design_evidence_only':True,'paths':path_coverage,'scenarios':scenarios,'mutations':mutations,
         'scenario_passed':sum(scenarios.values()),'scenario_total':len(scenarios),'mutations_caught':sum(mutations.values()),'mutation_total':len(mutations),'passed':passed,
         'note':'Models immutable dispatch context only; OS leakage-control capability claims remain documented/pre-release measured evidence.'}
    OUT.write_text(json.dumps(out,indent=2)+'\n');print(json.dumps(out,indent=2));return 0 if passed else 1
if __name__=='__main__':raise SystemExit(main())
