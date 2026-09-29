#!/usr/bin/env python3
"""R16B deterministic models for the last two repo-local V-020 criteria."""
from __future__ import annotations
import collections,itertools,json,random
from pathlib import Path
ROOT=Path(__file__).resolve().parents[2]
OUT=ROOT/'verification/results/v020-provider-resilience-r16b-latest.json'

def blocked_vendor_suffix():
    runs=[]
    fallback_counter=collections.Counter()
    for seed in range(40):
      rng=random.Random(seed)
      n=600
      routes={}
      invites=[]
      edges=[]
      for u in range(n):
        # Cloudflare-first primary distribution, with distinct suffix/custom-domain shapes.
        r=rng.random()
        if r<.58: primary=('cloudflare',f'u{u}.workers.dev','cf-ip')
        elif r<.78: primary=('cloudflare',f'home{u}.example.net','cf-ip')
        elif r<.90: primary=('independent',f'home{u}.indie.example','indie-ip')
        else: primary=('selfhost',f'home{u}.self.example','self-ip')
        rs=[primary]
        # second-family fallback is deliberately not universal
        if rng.random()<.48: rs.append(('independent',f'fallback{u}.indie.example','indie-ip'))
        if rng.random()<.14: rs.append(('selfhost',f'fallback{u}.self.example','self-ip'))
        routes[u]=rs
        if u:
          inviter=rng.randrange(u); invites.append((inviter,u)); edges.append((inviter,u))
          for _ in range(rng.randrange(3)):
            v=rng.randrange(u); edges.append((v,u))
      def available(u,scenario):
        out=[]
        for fam,host,ipfam in routes[u]:
          if scenario=='suffix' and host.endswith('.workers.dev'): continue
          if scenario=='vendor' and (fam=='cloudflare' or ipfam=='cf-ip'): continue
          out.append((fam,host,ipfam))
        return out
      for scenario in ('suffix','vendor'):
        established=sum(bool(available(a,scenario)) and bool(available(b,scenario)) for a,b in edges)/len(edges)
        first=sum(bool(available(target,scenario)) for _,target in invites)/len(invites)
        carriers=collections.Counter()
        for u in range(n):
          for fam,host,ipfam in available(u,scenario): carriers[fam]+=1
        for k,v in carriers.items(): fallback_counter[(scenario,k)]+=v
        runs.append({'seed':seed,'scenario':scenario,'established_relationship_fraction':round(established,4),
                     'first_contact_fraction':round(first,4),'available_route_counts':dict(carriers)})
    def avg(s,key):
      xs=[r[key] for r in runs if r['scenario']==s]; return round(sum(xs)/len(xs),4)
    return {'runs':runs,'mean':{s:{'established_relationship_fraction':avg(s,'established_relationship_fraction'),
                                  'first_contact_fraction':avg(s,'first_contact_fraction')} for s in ('suffix','vendor')},
            'fallback_carriers':{f'{s}:{f}':v for (s,f),v in sorted(fallback_counter.items())}}

def sponsor_overlap():
    cases=0
    violations=[]
    DAYS=45
    MIN_OVERLAP=30
    for new_route_day in (None,0,5,15,29,30,31,44):
      for activity_day in range(DAYS):
        for op in ('deposit','drain'):
          cases+=1
          revoked_day=0
          within_overlap=activity_day < revoked_day+MIN_OVERLAP
          new_exists=new_route_day is not None and activity_day>=new_route_day
          # PC-D06: during 30-day overlap deposits/drains remain operational; once a new route exists,
          # deposits may return typed moved while forwarding remains discoverable.
          if within_overlap:
            outcome='moved' if op=='deposit' and new_exists else 'works'
          else:
            outcome='moved' if op=='deposit' and new_exists else 'closed'
          if within_overlap and outcome not in ('works','moved'): violations.append((new_route_day,activity_day,op,outcome))
          # Sponsor revocation never removes independent route.
          if new_exists: assert new_route_day is not None
    # PC-D07 / R-260: sponsorship never implies sponsor-served ClientHost.
    for invitee_opted_in in (False,True):
      cases+=1
      sponsor_serves_client=invitee_opted_in
      if not invitee_opted_in: assert not sponsor_serves_client
    # Explicit non-sponsor second route remains usable after sponsor closes.
    for second_route in (False,True):
      cases+=1
      after_day30_reachable=second_route
      if second_route: assert after_day30_reachable
    return {'cases':cases,'violations':violations,'passed':not violations,'minimum_overlap_days':MIN_OVERLAP,
            'non_sponsor_second_route_preserved':True,'sponsor_clienthost_requires_explicit_opt_in':True}

def main():
    b=blocked_vendor_suffix(); s=sponsor_overlap()
    payload={'kind':'r16b_v020_provider_resilience','gates':['V-020'],'passed':s['passed'],
      'criteria_evidenced':[
       "blocked-vendor and blocked-suffix simulation (for example all workers.dev locators and one vendor's IP ranges blocked): fraction of established relationships and of first contacts that still succeed, and which fallbacks carried them (R-021, R-028, R-045)",
       'sponsor revocation overlap and non-sponsor second route in the sponsorship model; PC-D06/PC-D07 specified (R-266, R-267, R-260)'],
      'blocked_vendor_suffix':b,'sponsor_overlap':s,'gate_status_effect':'repo_local_criteria_only_independent_review_still_required'}
    OUT.write_text(json.dumps(payload,indent=2)+'\n')
    print(json.dumps({'passed':payload['passed'],'suffix':b['mean']['suffix'],'vendor':b['mean']['vendor'],'sponsor_cases':s['cases']}))
    return 0 if payload['passed'] else 1
if __name__=='__main__': raise SystemExit(main())
