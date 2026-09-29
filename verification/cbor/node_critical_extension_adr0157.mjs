import fs from 'node:fs';
const root = new URL('../../', import.meta.url);
const vectors = JSON.parse(fs.readFileSync(new URL('verification/vectors/adr0157-critical-extension-convergence.json', root)));
const fields = JSON.parse(fs.readFileSync(new URL('spec/registries/om-numeric-fields.json', root)));
const base = new Map((fields.extensions || []).map(e => [e.name, {critical: e.critical === true}]));
const future = new Map(base);
future.set('om.critical.future.example', {critical:true});
function verdict(reg, id, wireCritical) {
  const known=reg.get(id);
  if (!known) {
    if (wireCritical || id.startsWith('om.critical.')) return 'reject_unknown_critical_extension';
    return 'accept_ignored';
  }
  if (wireCritical !== known.critical) return 'reject_criticality_mismatch';
  return 'accept_registered';
}
const invalidRegs=(fields.extensions||[]).filter(e => e.name.startsWith('om.critical.') && e.critical !== true).map(e=>e.name);
const rows=vectors.cases.map(c => ({id:c.id, baseline:verdict(base,c.extension_id,c.wire_critical), future_registry:verdict(future,c.extension_id,c.wire_critical)}));
const mismatches=[];
for (let i=0;i<rows.length;i++) for (const k of ['baseline','future_registry']) if (rows[i][k] !== vectors.cases[i][k]) mismatches.push({id:rows[i].id,field:k,expected:vectors.cases[i][k],actual:rows[i][k]});
if (invalidRegs.length) mismatches.push({registry_noncritical_reserved_prefix:invalidRegs});
const out={implementation:'node',cases:rows.length,mismatches,passed:mismatches.length===0};
process.stdout.write(JSON.stringify(out,null,2));
if (!out.passed) process.exit(1);
