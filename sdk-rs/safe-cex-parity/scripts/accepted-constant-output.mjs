import { loadPolicy } from './accepted-policy-files.mjs';
import assert from 'node:assert/strict';
import { reviewedInheritedConstantOutput } from './accepted-inherited-constant-output.mjs';
import fs from 'node:fs';
import { createHash } from 'node:crypto';
import { fileURLToPath } from 'node:url';

const reviewed = loadPolicy('constant-outputs');

// Permission belongs to exact reviewed production serializer bodies and native
// adapters. It is never inferred from a method name or a caller's constant data.
export function reviewedConstantOutput(type, entry, index, context) {
  if (entry.constantOutput === undefined) return false;
  const label = `${type.name}: accepted-set assertion: reviewed constant output`;
  assert.equal(entry.constantOutput, true, `${label}: flag must be true`);
  if (reviewedInheritedConstantOutput(type, entry, index, context)) return true;
  const spec = reviewed[`${type.name}::${entry.api.trait}::${entry.api.name}`];
  assert.ok(spec, `${label}: unreviewed owner or method`);
  assert.ok(type.deserialize === false && type.kind === 'record' && type.ident === spec.ident, `${label}: actual serializer owner required`);
  assert.equal(entry.unitInput, undefined, `${label}: constant metadata inputs are not unit input`);
  assert.deepEqual(entry.api, spec.api, `${label}: compiler API changed`);
  assert.ok(context.packageMetadata?.name === 'safe-cex-num'
    && context.packageMetadata.manifest_path === fileURLToPath(new URL('../../safe-cex-num/Cargo.toml', import.meta.url)), `${label}: actual SDK compiler package required`);
  const source = type.entrypointSources?.[index];
  assert.deepEqual(source, { filename: spec.source.file, begin: spec.source.begin, end: spec.source.end }, `${label}: compiler source span changed`);
  assert.equal(typeof context.readSource, 'function', `${label}: actual source reader required`);
  assert.equal(createHash('sha256').update(context.readSource(spec.source.file)).digest('hex'), spec.source.sha256, `${label}: reviewed source changed`);
  assert.deepEqual(entry.call, spec.call, `${label}: reviewed native adapter changed`);
  assert.deepEqual(entry.cases, spec.cases, `${label}: reviewed native witnesses changed`);
  return true;
}
