import { loadPolicy } from './accepted-policy-files.mjs';
import assert from 'node:assert/strict';
import fs from 'node:fs';
import path from 'node:path';

// These two concrete signatures have no nominal root. Expected types belong
// to reviewed policy, never to the classification contract. Native invariant
// equality checks the complete signature, including every borrowed lifetime.
const specs = loadPolicy('function-pointer-aliases');
const fields = ['expectedType', 'file', 'generics', 'rawTarget', 'roots', 'target'];
const callerFields = ['instantiation', 'aliasProbeExpected', 'aliasProbeKind', 'aliasProbeSuffix', 'nativeCanonical'];

export function reviewedFunctionPointerAliasProbe(alias, contract, context) {
  if (!Object.hasOwn(specs, alias.name)) return undefined;
  const spec = specs[alias.name];
  const label = `${alias.name}: accepted-set assertion: reviewed function pointer`;
  assert.deepEqual(Object.keys(spec).sort(), fields, `${label}: unsupported spec fields`);
  assert.equal(alias.ident, alias.name.split('::').at(-1), `${label}: compiler identifier changed`);
  assert.equal(alias.span?.filename, spec.file, `${label}: compiler source identity changed`);
  assert.deepEqual(alias.generics, { params: [], where_predicates: [] }, `${label}: only the concrete nongeneric alias is reviewed`);
  for (const key of ['generics', 'rawTarget', 'target']) {
    assert.deepEqual(alias[key], spec[key], `${label}: compiler ${key} changed`);
    assert.deepEqual(contract?.[key], alias[key], `${label}: contract compiler ${key} changed`);
  }
  assert.equal(contract?.category, 'non-decoder', `${label}: explicit capability classification required`);
  assert.ok(contract.evidence?.trim() && contract.reason?.trim(), `${label}: specific classification evidence required`);
  for (const key of callerFields) assert.equal(contract[key], undefined, `${label}: caller cannot supply ${key}`);
  assert.ok(context?.macroOrigins?.includes('serde_json') && Array.isArray(context.externalTypeAliases), `${label}: actual compiler context required`);
  // Occurrences are significant: authenticate every external path/trait in the
  // fully expanded signature, including repetitions inside nested callbacks.
  const roots = context.externalTypeAliases.filter(row => row.name === alias.name).map(row => ({
    canonical: row.canonical, kind: row.kind, artifactKind: row.artifact?.kind,
    package: row.artifact?.owner?.name ?? null,
  }));
  assert.deepEqual(roots, spec.roots, `${label}: every nested compiler root identity required`);
  for (const row of context.externalTypeAliases.filter(row => row.name === alias.name && row.artifact?.kind === 'workspace')) {
    const artifact = row.artifact;
    assert.ok(context.workspace && artifact.owner?.id === artifact.packageId, `${label}: selected workspace package identity required`);
    assert.equal(fs.realpathSync(artifact.owner.manifest_path), fs.realpathSync(path.join(context.workspace, artifact.owner.name, 'Cargo.toml')), `${label}: selected workspace source changed`);
  }
  return {
    suffix: '', canonical: 'fn-pointer', expectedType: spec.expectedType,
    probeKind: 'reviewed-function-pointer',
    monomorphization: `the actual concrete ${alias.name} function pointer; no callback execution or other function-pointer permission`,
  };
}
