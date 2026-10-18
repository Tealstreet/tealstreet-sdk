import assert from 'node:assert/strict';
import fs from 'node:fs';
import path from 'node:path';
import { loadPolicy } from './accepted-policy-files.mjs';

const specs = loadPolicy('napi-aliases');
const names = ['safe_cex_ffi::native_callback::Callback', 'safe_cex_ffi::native_callback::Request'];
const fields = ['canonical', 'expectedType', 'file', 'generics', 'rawTarget', 'roots', 'suffix', 'target'];
const callerFields = ['instantiation', 'aliasProbeExpected', 'aliasProbeKind', 'aliasProbeSuffix', 'nativeCanonical'];

export function reviewedNapiAliasProbe(alias, contract, context) {
  if (!names.includes(alias.name)) return undefined;
  const spec = specs[alias.name];
  const label = `${alias.name}: accepted-set assertion: reviewed NAPI alias`;
  assert.deepEqual(Object.keys(spec).sort(), fields, `${label}: unsupported policy fields`);
  assert.equal(alias.ident, alias.name.split('::').at(-1), `${label}: compiler identifier changed`);
  assert.equal(alias.span?.filename, spec.file, `${label}: compiler source identity changed`);
  for (const key of ['generics', 'rawTarget', 'target']) {
    assert.deepEqual(alias[key], spec[key], `${label}: compiler ${key} changed`);
    assert.deepEqual(contract?.[key], alias[key], `${label}: contract compiler ${key} changed`);
  }
  assert.equal(contract?.category, 'non-decoder', `${label}: explicit callback capability classification required`);
  assert.ok(contract.evidence?.trim() && contract.reason?.trim(), `${label}: callback capability evidence required`);
  for (const key of callerFields) assert.equal(contract[key], undefined, `${label}: caller cannot supply ${key}`);
  assert.ok(context?.macroOrigins?.includes('serde_json') && Array.isArray(context.externalTypeAliases), `${label}: actual compiler context required`);
  assert.equal(context.packageMetadata?.name, 'safe-cex-ffi', `${label}: selected FFI package required`);
  assert.equal(fs.realpathSync(context.packageMetadata.manifest_path), fs.realpathSync(path.join(context.workspace, 'safe-cex-ffi/Cargo.toml')), `${label}: selected workspace source required`);
  const roots = context.externalTypeAliases.filter(row => row.name === alias.name);
  assert.deepEqual(roots.map(row => ({ canonical: row.canonical, kind: row.kind, artifactKind: row.artifact?.kind,
    packageId: row.artifact?.packageId, features: row.artifact?.features })), spec.roots, `${label}: every nested NAPI Cargo artifact identity required`);
  assert.equal(new Set(roots.map(row => fs.realpathSync(row.artifact.compilerPath))).size, 1, `${label}: one actual NAPI compiler artifact required`);
  assert.equal(spec.canonical, alias.target.resolved_path.path, `${label}: reviewed canonical root required`);
  return { suffix: spec.suffix, canonical: spec.canonical, expectedType: spec.expectedType, probeKind: 'reviewed-napi',
    monomorphization: `one exact ${alias.name}${spec.suffix} NAPI callback capability; ordinary alias trait probes remain required` };
}
