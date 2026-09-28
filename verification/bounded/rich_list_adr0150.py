#!/usr/bin/env python3
"""ADR-0150 V-005 two-implementation rich-list extension evidence.

ADR-0155: a wire-criticality mismatch rejects the containing object; every other registered-schema or
presentation-semantic failure ignores the entire non-critical rich-list extension and leaves W0 RichText intact.
"""
from __future__ import annotations
from pathlib import Path
import json, subprocess, sys
ROOT=Path(__file__).resolve().parents[2]
VEC=ROOT/'verification/vectors/adr0150-rich-list-vectors.json'
OUT=ROOT/'verification/results/rich-list-adr0150-latest.json'
REG=ROOT/'spec/registries/om-numeric-fields.json'
CBOR=ROOT/'spec/registries/om-cbor-profile.json'
sys.path.insert(0,str(ROOT/'verification/cbor'))
from python_canonical import enc

def item_map(it,extra):
    pairs=[[0,it['block_index']],[1,it['depth']],[2,it['style']]]
    if 'ordinal' in it:pairs.append([3,it['ordinal']])
    for k,v in extra.items():pairs.append([int(k),v])
    return {'$map':pairs}
def payload(v):
    pairs=[]
    if not v.get('missing_items'):pairs.append([0,[item_map(x,v.get('item_extra',{})) for x in v['items']]])
    for k,val in v.get('payload_extra',{}).items():pairs.append([int(k),val])
    return {'$map':pairs}
def prefix(it):
    return '  '*it['depth'] + ('- ' if it['style']==1 else f"{it.get('ordinal')}. ")
def block(v,idx,it):
    o=v.get('block_overrides',{}).get(str(idx))
    return o if o is not None else {'kind':'paragraph','text':prefix(it)+f'item {idx}','marked':False}
def schema(v):
    # ADR-0155: wire criticality first (reject); every other registered-schema failure of this
    # non-critical registration ignores the enhancement and leaves the W0 event valid.
    if v['critical'] is not False:return 'reject_object','criticality_mismatch'
    if v['owning_object']!='ConversationEvent':return 'ignore_extension','wrong_extension_object'
    if v.get('payload_extra'):return 'ignore_extension','unknown_field'
    if v.get('missing_items'):return 'ignore_extension','missing_field'
    if not isinstance(v['items'],list) or not 1<=len(v['items'])<=256:return 'ignore_extension','item_count_schema'
    for it in v['items']:
        if v.get('item_extra'):return 'ignore_extension','unknown_field'
        x=it.get('block_index'); d=it.get('depth'); s=it.get('style')
        if not isinstance(x,int) or isinstance(x,bool) or not 0<=x<=4095:return 'ignore_extension','block_index_schema'
        if not isinstance(d,int) or isinstance(d,bool) or not 0<=d<=3:return 'ignore_extension','depth_schema'
        if not isinstance(s,int) or isinstance(s,bool) or s not in {1,2}:return 'ignore_extension','style_schema'
        if 'ordinal' in it:
            o=it['ordinal']
            if not isinstance(o,int) or isinstance(o,bool) or not 1<=o<=9999:return 'ignore_extension','ordinal_schema'
    return None
def validate(v):
    s=schema(v)
    if s:return s
    if v['event_kind'] not in {'message.create','message.edit'}:return 'ignore_extension','event_kind'
    if v.get('rendered_event','self')!='self':return 'ignore_extension','annotation_not_for_rendered_text'
    last=None
    for i,it in enumerate(v['items']):
        if last is not None and it['block_index']<=last:return 'ignore_extension','block_index_order'
        if it['block_index']>=v['block_count']:return 'ignore_extension','block_reference'
        b=block(v,it['block_index'],it)
        if b.get('kind')!='paragraph':return 'ignore_extension','block_kind'
        gap=last is None or it['block_index']!=last+1
        if gap and it['depth']!=0:return 'ignore_extension','depth_run'
        if not gap and it['depth']>v['items'][i-1]['depth']+1:return 'ignore_extension','depth_transition'
        if (it['style']==2) != ('ordinal' in it):return 'ignore_extension','ordinal_semantics'
        if b.get('marked') or not isinstance(b.get('text'),str) or not b['text'].startswith(prefix(it)):return 'ignore_extension','fallback_prefix'
        last=it['block_index']
    return 'accept_extension',None

def main()->int:
    vectors=json.loads(VEC.read_text()); py={}; mismatches=[]
    for v in vectors:
        pl=payload(v); entry={'$map':[['om.rich-list.v1',{'$map':[[0,v['critical']],[1,pl]]}]]}
        verdict,error=validate(v); py[v['id']]={'verdict':verdict,'error':error,'cbor_hex':enc(entry).hex()}
        if verdict!=v['expected_verdict'] or error!=v.get('expected_error'):mismatches.append(v['id']+':expected')
    cp=subprocess.run(['node',str(ROOT/'verification/cbor/node_adr0150_rich_list.mjs'),str(VEC)],capture_output=True,text=True)
    if cp.returncode:
        print(cp.stderr,file=sys.stderr);return cp.returncode
    node=json.loads(cp.stdout)
    for k in py:
        if py[k]!=node.get(k):mismatches.append(k+':python_node')
    reg=json.loads(REG.read_text()); cbor=json.loads(CBOR.read_text()); ex={x['name']:x for x in reg['extensions']}; structs=reg['structs']
    e=ex.get('om.rich-list.v1',{})
    structural={
      'registered':bool(e), 'owner':e.get('object')=='ConversationEvent','criticality':e.get('critical') is False,
      'schema_ref':e.get('value')=={'ref':'RichListV1Extension'},'structs_present':all(x in structs for x in ['RichListV1Extension','RichListItemV1Extension']),
      'cbor_registry_mirror':'om.rich-list.v1' in cbor['extensions']['known_extensions'],
    }
    ids={x['id']:x for x in vectors}
    semantic={
      'all_python_node_agree':len(mismatches)==0,
      'max_256_accepts':py['max-item-count']['verdict']=='accept_extension',
      '257_schema_ignored_not_rejected':py['too-many-items']['verdict']=='ignore_extension',
      'depth4_schema_ignored_not_rejected':py['depth-schema-high']['verdict']=='ignore_extension',
      'ordinal10000_schema_ignored_not_rejected':py['ordinal-schema-high']['verdict']=='ignore_extension',
      'wrong_criticality_rejects_object':py['wrong-criticality']['verdict']=='reject_object',
      'create_annotations_never_apply_to_edit_text':py['create-annotations-over-edit-text']['error']=='annotation_not_for_rendered_text',
      'duplicate_reference_ignores_whole_extension':py['duplicate-block-index']['verdict']=='ignore_extension',
      'depth_transition_ignores_whole_extension':py['depth-jump']['verdict']=='ignore_extension',
      'fallback_mismatch_ignores_whole_extension':py['fallback-mismatch']['verdict']=='ignore_extension',
      'ordered_ordinal_semantic_mismatch_ignored':py['ordered-missing-ordinal']['verdict']=='ignore_extension',
      'w0_fallback_preserved_for_semantic_ignores':all(v['expected_verdict']=='ignore_extension' for v in vectors if v['id'] in {'duplicate-block-index','decreasing-block-index','missing-referenced-block','nonparagraph-block','first-depth-nonzero','gap-depth-nonzero','depth-jump','ordered-missing-ordinal','unordered-has-ordinal','fallback-mismatch','fallback-marked-first-inline','ordered-leading-zero-fallback'}),
    }
    passed=not mismatches and all(structural.values()) and all(semantic.values())
    out={'kind':'adr0150_rich_list_cross_impl','design_evidence_only':True,'gates':['V-005'],'vector_count':len(vectors),'python_node_agreement':len(vectors)-sum(x.endswith(':python_node') for x in mismatches),'structural':structural,'semantic':semantic,'mismatches':mismatches,'passed':passed,
         'note':'ADR-0155: wire-criticality mismatch rejects the object; object/schema/reference/nesting/fallback failures and annotations evaluated against another event text ignore the entire non-critical list enhancement and preserve W0 RichText.'}
    OUT.write_text(json.dumps(out,indent=2)+'\n');print(json.dumps(out,indent=2));return 0 if passed else 1
if __name__=='__main__':raise SystemExit(main())
