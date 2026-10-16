import { loadPolicy } from './accepted-policy-files.mjs';
import assert from 'node:assert/strict';
import { reviewedHyperliquidProjectionProbe } from './accepted-hyperliquid-projection.mjs';

// One reviewed fixed instantiation per generic alias. The spec owns the
// expected Rust type; contracts never supply it, and unknown names fall
// through to the original fail-closed families.
const productionSpecs = loadPolicy('generic-aliases');
const fields = ['canonical', 'expectedType', 'file', 'generics', 'rawTarget', 'suffix', 'target', 'unsized'];
const callerFields = ['instantiation', 'aliasProbeExpected', 'aliasProbeKind', 'aliasProbeSuffix', 'nativeCanonical'];

function rootPath(type) {
  return type?.resolved_path?.path;
}

export function reviewedGenericAliasProbe(alias, contract, context, specs = productionSpecs) {
  const spec = specs[alias.name];
  if (!spec) return undefined;
  const label = `${alias.name}: accepted-set assertion: reviewed generic alias`;
  assert.deepEqual(Object.keys(spec).sort(), fields, `${label}: unsupported spec fields`);
  assert.equal(alias.span?.filename, spec.file, `${label}: compiler source identity changed`);
  for (const key of ['generics', 'rawTarget', 'target']) assert.deepEqual(alias[key], spec[key], `${label}: compiler ${key} changed`);
  for (const key of callerFields) assert.equal(contract?.[key], undefined, `${label}: caller cannot supply ${key}`);
  assert.ok(typeof spec.expectedType === 'string' && spec.expectedType.trim(), `${label}: expected compiler type required`);
  assert.ok(/^<.*>$/.test(spec.suffix), `${label}: fixed instantiation suffix required`);
  assert.equal(typeof spec.unsized, 'boolean', `${label}: sizedness must be reviewed`);
  const projection = reviewedHyperliquidProjectionProbe(alias, contract, context, spec);
  if (projection) return projection;
  assert.equal(spec.unsized, !rootPath(alias.target), `${label}: only rootless dyn targets are unsized`);
  if (!spec.unsized) assert.equal(spec.canonical, rootPath(alias.target), `${label}: canonical root must be the compiler target root`);
  assert.ok(Array.isArray(context?.macroOrigins) && context.macroOrigins.includes('serde_json'), `${label}: actual serde_json compiler dependency required`);
  assert.ok(Array.isArray(context.externalTypeAliases), `${label}: actual artifact context required`);
  const roots = context.externalTypeAliases.filter(row => row.name === alias.name && row.canonical === spec.canonical);
  const external = spec.canonical.split('::')[0] !== alias.name.split('::')[0];
  assert.ok(spec.unsized || ((!external || roots.length) && roots.every(row => ['sysroot', 'workspace'].includes(row.artifact?.kind))), `${label}: canonical root needs actual compiler identity`);
  return {
    suffix: spec.suffix,
    canonical: spec.canonical,
    expectedType: spec.expectedType,
    probeKind: spec.unsized ? 'reviewed-unsized' : 'reviewed-fixed',
    monomorphization: `one reviewed ${alias.name}${spec.suffix} instantiation; no claim for other generic arguments`,
  };
}
