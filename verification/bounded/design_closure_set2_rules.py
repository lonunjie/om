#!/usr/bin/env python3
"""Design-only final closure audit for ADR-0139.

No compiler, TLC, Rust, Wasm/native runtime, network access or product implementation
is used here. This checks architecture ledgers, source authority and closure drift.
"""
from pathlib import Path
import json, re, sys

ROOT=Path(__file__).resolve().parents[2]
OUT=ROOT/'verification/results/design-closure-set2-latest.json'

def jsonl(path):
    return [json.loads(x) for x in path.read_text().splitlines() if x.strip()]

def fail(problems,msg):
    problems.append(msg)

def main():
    problems=[]; checks=[]
    reqs=jsonl(ROOT/'machine/requirements.jsonl'); req_ids={x['id'] for x in reqs}
    threats=jsonl(ROOT/'machine/threat_register.jsonl'); threat_ids={x['id'] for x in threats}
    decisions=jsonl(ROOT/'machine/decisions.jsonl'); decision_ids={x['id'] for x in decisions}
    design=jsonl(ROOT/'machine/architecture_design_queue.jsonl')
    vg=jsonl(ROOT/'machine/architecture_verification_gates.jsonl'); v_ids={x['id'] for x in vg}
    pg=jsonl(ROOT/'machine/public_release_gates.jsonl'); pr_ids={x['id'] for x in pg}
    suites=json.load(open(ROOT/'machine/conformance_suites.json'))['suites']
    suite_ids={x.get('id') or x.get('name') for x in suites}
    valid_evidence=v_ids|pr_ids|{'validator'}|{f'conformance:{x}' for x in suite_ids}
    review_basenames={p.name for p in ROOT.rglob('*') if p.is_file()}

    # Design queue closure.
    # ADR-0156: an explicitly tracked open item is allowed only when it blocks engineering, names
    # its scopes, and none of them is a W0 scope; anything else is a closure failure.
    w0={k for k,v in json.load(open(ROOT/'machine/repository_contract.json'))['engineering_scopes'].items() if v.get('wave')=='W0'}
    open_items=[x for x in design if x.get('status') not in {'resolved','superseded','closed'}]
    open_design=[x['id'] for x in open_items]
    bad_open=[x['id'] for x in open_items if not x.get('blocks_engineering') or not x.get('scopes') or set(x['scopes'])&w0]
    if bad_open: fail(problems,f'open design queue items not confined to tracked later-wave scopes: {bad_open}')
    checks.append({'name':'design_queue_closed_for_w0','items':len(design),'open':len(open_design),'open_tracked_later_wave':open_design})

    # Every requirement exactly once in traceability and only valid targets.
    tr=json.load(open(ROOT/'machine/requirement_traceability.json'))['traces']
    seen={}
    for row in tr:
        rid=row.get('requirement'); seen[rid]=seen.get(rid,0)+1
        if rid not in req_ids: fail(problems,f'trace references unknown requirement {rid}')
        for target in row.get('verified_by',[]):
            if target not in valid_evidence:
                if target.startswith('review:') and target.split(':',1)[1].split('/')[-1] in review_basenames:
                    pass
                else:
                    fail(problems,f'{rid} invalid trace target {target}')
    missing=sorted(req_ids-set(seen)); dup=sorted(k for k,v in seen.items() if v!=1)
    if missing: fail(problems,f'requirements missing trace: {missing}')
    if dup: fail(problems,f'requirements not exactly once in trace: {dup}')
    checks.append({'name':'requirement_traceability','requirements':len(req_ids),'rows':len(tr),'missing':len(missing),'duplicates':len(dup)})

    # Every threat points at valid requirements/evidence and at least one existing mitigation doc.
    bad_th=[]
    for t in threats:
        tid=t['id']
        for rid in t.get('requirements',[]):
            if rid not in req_ids: bad_th.append(f'{tid}:unknown requirement {rid}')
        for g in t.get('gates',[]):
            if g not in valid_evidence: bad_th.append(f'{tid}:unknown gate/evidence {g}')
        docs=t.get('mitigation_docs',[])
        if not docs: bad_th.append(f'{tid}:no mitigation docs')
        for d in docs:
            if not (ROOT/d).exists(): bad_th.append(f'{tid}:missing mitigation doc {d}')
    problems.extend(bad_th)
    checks.append({'name':'threat_traceability','threats':len(threats),'problems':len(bad_th)})

    # ADR decisions must resolve to indexed existing ADR files.
    index=(ROOT/'decisions/INDEX.md').read_text()
    indexed=set(re.findall(r'\((ADR-\d{4}[^)]*\.md)\)',index))
    indexed_nums={x.split('-')[0]+'-'+x.split('-')[1] for x in indexed}
    bad_dec=[]
    for d in decisions:
        adr=d.get('adr')
        if not adr: continue
        if adr not in indexed_nums: bad_dec.append(f"{d['id']}:{adr} not indexed")
    for f in indexed:
        if not (ROOT/'decisions'/f).exists(): bad_dec.append(f'index missing file {f}')
    problems.extend(bad_dec)
    checks.append({'name':'decision_adr_index','decisions':len(decisions),'indexed_adrs':len(indexed),'problems':len(bad_dec)})

    # Invariant doc IDs must exactly equal machine map; refs must all exist.
    invtext=(ROOT/'docs/architecture/safety/SECURITY_INVARIANTS.md').read_text()
    doc_inv=re.findall(r'^\| `([A-Z]\d+)` \|',invtext,re.M)
    imap=json.load(open(ROOT/'machine/security_invariant_traceability.json'))['invariants']
    map_ids=[x['id'] for x in imap]
    if set(doc_inv)!=set(map_ids): fail(problems,f'invariant ID mismatch doc={sorted(doc_inv)} map={sorted(map_ids)}')
    if len(map_ids)!=len(set(map_ids)): fail(problems,'duplicate invariant map IDs')
    bad_inv=[]
    for x in imap:
        iid=x['id']
        if not x.get('requirements') or not x.get('decisions') or not x.get('evidence') or not x.get('sources'):
            bad_inv.append(f'{iid}:empty required mapping dimension')
        for rid in x.get('requirements',[]):
            if rid not in req_ids: bad_inv.append(f'{iid}:unknown requirement {rid}')
        for tid in x.get('threats',[]):
            if tid not in threat_ids: bad_inv.append(f'{iid}:unknown threat {tid}')
        for did in x.get('decisions',[]):
            if did not in decision_ids: bad_inv.append(f'{iid}:unknown decision {did}')
        for ev in x.get('evidence',[]):
            if ev not in valid_evidence: bad_inv.append(f'{iid}:unknown evidence {ev}')
        for src in x.get('sources',[]):
            if not (ROOT/src).exists(): bad_inv.append(f'{iid}:missing source {src}')
    problems.extend(bad_inv)
    checks.append({'name':'security_invariant_traceability','document_invariants':len(doc_inv),'mapped':len(map_ids),'problems':len(bad_inv)})

    # Canonical stale-marker checks. Historical ADRs/reviews/results are deliberately excluded.
    exact={
      'spec/protocol/conversation-event.logical.schema.json':['TBD'],
      'spec/protocol/crypto-profile.logical.schema.json':['TBD'],
      'docs/architecture/identity/IDENTITY_CONTINUITY.md':['## Open questions'],
      'docs/architecture/identity/IDENTITY_DISCOVERY_VERIFICATION.md':['## Open questions'],
      'docs/architecture/discovery/KEY_TRANSPARENCY_AND_DIRECTORY.md':['## Open question'],
      'docs/architecture/crypto/MULTI_DEVICE_SYNC.md':['Candidate design direction','## Open details'],
    }
    stale=[]
    for rel,terms in exact.items():
        txt=(ROOT/rel).read_text()
        for term in terms:
            if term.lower() in txt.lower(): stale.append(f'{rel}:{term}')
    if stale: problems.extend('stale canonical marker '+x for x in stale)
    # Whole current canonical-tree ambiguity sweep. Decision history/reviews/research/results are intentionally not canonical current instructions.
    ambiguity_re=re.compile(r'\b(?:TBD|TODO|FIXME|provisional|to be determined|not yet specified)\b|candidate design direction|^## Open questions?$|^## Open details$',re.I|re.M)
    roots=[ROOT/'docs/architecture',ROOT/'docs/process',ROOT/'docs/constitution',ROOT/'spec']
    ambiguity_hits=[]; scanned=0
    for base in roots:
        for fp in base.rglob('*'):
            if not fp.is_file() or fp.suffix.lower() not in {'.md','.json','.jsonl','.cddl'}: continue
            scanned+=1
            try: txt=fp.read_text()
            except UnicodeDecodeError: continue
            for m in ambiguity_re.finditer(txt):
                # A marker on a line that cites a tracked open design item is tracked, not hidden.
                line=txt[txt.rfind('\n',0,m.start())+1:txt.find('\n',m.end()) if txt.find('\n',m.end())>=0 else len(txt)]
                if any(i in line for i in open_design): continue
                ambiguity_hits.append(f"{fp.relative_to(ROOT)}:{m.group(0)}")
    if ambiguity_hits: problems.extend('unresolved canonical ambiguity marker '+x for x in ambiguity_hits)
    checks.append({'name':'canonical_ambiguity_markers','selected_stale_problems':len(stale),'canonical_files_scanned':scanned,'whole_tree_hits':ambiguity_hits})

    advisory=(ROOT/'docs/architecture/providers/VULNERABILITY_RESPONSE_AND_SECURITY_ADVISORIES.md').read_text().lower()
    if 'stored' not in advisory or 'floor' not in advisory or 'never' not in advisory:
        fail(problems,'security advisory document lacks explicit stored-floor non-regression wording')
    if re.search(r'later advisory lowers',advisory):
        fail(problems,'security advisory doc still says later advisory lowers floor')
    archive=(ROOT/'docs/architecture/ecosystem/IMPORT_EXPORT_AND_USER_ESCAPE.md').read_text().lower()
    if 'verification-only' not in archive or 'v-014' not in archive:
        fail(problems,'archive candidate is not explicitly verification-only and V-014 gated')
    readme=(ROOT/'README.md').read_text().lower()
    if 'cddl' not in readme or 'canonical' not in readme or 'registr' not in readme:
        fail(problems,'README lacks frozen W0 CDDL/registry canonical authority statement')
    checks.append({'name':'closure_semantics','advisory_floor_rule':True,'archive_candidate_nonproduction':True,'wire_authority':True})

    # Handoff/contract must say the design is closed and engineering is authorized only per scope ADR (ADR-0169).
    st=(ROOT/'docs/process/ENGINEER_START_HERE.md').read_text().lower()
    ho=(ROOT/'docs/process/ENGINEERING_HANDOFF.md').read_text().lower()
    rc=json.load(open(ROOT/'machine/repository_contract.json'))
    if 'design-frozen' not in st or 'not authorized' not in st: fail(problems,'ENGINEER_START_HERE missing scoped lock statement')
    if 'not authorized' not in ho: fail(problems,'ENGINEERING_HANDOFF missing scoped lock statement')
    auth=[k for k,v in rc.get('engineering_scopes',{}).items() if v.get('authorized')]
    if bool(rc.get('engineering_authorized')) != bool(auth): fail(problems,'engineering_authorized disagrees with the authorized scope set')
    for k in auth:
        adr=rc['engineering_scopes'][k].get('authorizing_adr')
        if not adr or not list((ROOT/'decisions').glob(f'{adr}-*.md')): fail(problems,f'scope {k} authorized without an existing authorizing ADR')
    checks.append({'name':'engineering_lock','global_authorized':rc.get('engineering_authorized'),'authorized_scopes':auth})

    # Final report must cover every declared scope and the requested attack classes.
    rpt=(ROOT/'docs/reports/2026-09-26-final-architecture-closure-and-adversarial-review.md').read_text().lower()
    miss_scopes=[s for s in rc['engineering_scopes'] if f'`{s}`'.lower() not in rpt]
    attack_terms=['authority laundering','optional security context','stable correlators','fail-open','aba','downgrade','recovery escalation','provider colocation','multi-device races','multi-account isolation','portability authority','offline/partition','compromised-client','update/distribution','resource exhaustion']
    miss_terms=[x for x in attack_terms if x not in rpt]
    if miss_scopes: fail(problems,f'adversarial report missing scopes: {miss_scopes}')
    if miss_terms: fail(problems,f'adversarial report missing attack classes: {miss_terms}')
    checks.append({'name':'eleven_scope_adversarial_coverage','scopes':len(rc['engineering_scopes']),'missing_scopes':miss_scopes,'missing_attack_classes':miss_terms})

    result={'kind':'design_closure_set2','design_only':True,'requirements':len(reqs),'threats':len(threats),'decisions':len(decisions),'design_queue_items':len(design),'security_invariants':len(map_ids),'checks':checks,'problems':problems,'passed':not problems,'note':'No TLC, Rust, Wasm/native build or product implementation was executed by this audit.'}
    OUT.write_text(json.dumps(result,indent=2)+'\n')
    print(json.dumps(result,indent=2))
    return 0 if not problems else 1

if __name__=='__main__': sys.exit(main())
