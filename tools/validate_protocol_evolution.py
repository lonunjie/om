#!/usr/bin/env python3
from __future__ import annotations
import json
from pathlib import Path
ROOT=Path(__file__).resolve().parents[1]
def load(p): return json.loads((ROOT/p).read_text())
def fail(msg): raise SystemExit('protocol-evolution check: '+msg)
policy=load('machine/protocol_evolution_policy.json')
if policy.get('status')!='frozen_by_ADR-0165': fail('policy status mismatch')
if len(policy.get('change_classes',[]))!=7: fail('expected 7 change classes')
if not policy.get('published_meaning_immutable'): fail('published meaning must be immutable')
neg=policy.get('negotiation',{})
if neg.get('primary_order')!='security_generation_for_conversation_type_descending': fail('wrong negotiation primary order')
if neg.get('profile_id_is_security_ordering') is not False: fail('profile id security-ordering flag')
if neg.get('tie_break')!='smallest_numeric_profile_id_only_after_equal_security_generation': fail('wrong tie-break')
ext=policy.get('extension_evolution',{})
for k in ('criticality_immutable_per_extension_id','reserved_critical_prefix_ids_are_permanently_critical','new_critical_semantics_require_reserved_critical_prefix','same_id_new_version_requires_noncritical_backward_compatible_fallback','authority_or_security_semantic_change_requires_new_id_or_profile','logical_schema_cannot_expand_frozen_w0_wire'):
    if not ext.get(k,False): fail('extension rule missing: '+k)
reg=load('spec/registries/om-numeric-fields.json')
grandfather={'om.acs.fork_ack','om.acs.legacy_policy'}
for e in reg.get('extensions',[]):
    name=e['name']; critical=bool(e.get('critical'))
    if name.startswith('om.critical.') and not critical: fail('om.critical. extension is non-critical: '+name)
    if critical and not name.startswith('om.critical.') and name not in grandfather: fail('critical extension lacks the om.critical. prefix: '+name)
# Required canonical documents and guardrail pointers.
for rel in ['decisions/ADR-0165-protocol-evolution-compatibility-and-engineering-guardrails.md','docs/process/PROTOCOL_EVOLUTION_AND_CHANGE_CONTROL.md','machine/protocol_evolution_policy.json','verification/bounded/protocol_evolution_adr0165.py']:
    if not (ROOT/rel).exists(): fail('missing '+rel)
for rel, needle in [
    ('AGENTS.md','Run 7 protocol-evolution rules'),
    ('docs/constitution/PROTOCOL_LONGEVITY_AND_0M_CONTINUITY.md','ADR-0165'),
    ('docs/constitution/PROTOCOL_EXTENSIBILITY.md','ADR-0165'),
    ('docs/architecture/crypto/CRYPTOGRAPHY_AND_AGILITY.md','ADR-0165'),
]:
    if needle not in (ROOT/rel).read_text(): fail('missing '+needle+' in '+rel)
print('protocol-evolution check: PASS')
