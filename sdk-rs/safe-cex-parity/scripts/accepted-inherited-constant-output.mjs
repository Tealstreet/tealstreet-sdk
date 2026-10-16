import assert from 'node:assert/strict';
import fs from 'node:fs';
import { createHash } from 'node:crypto';
import { fileURLToPath } from 'node:url';

const reviewed = JSON.parse(fs.readFileSync(new URL('../accepted-set/inherited-constant-outputs.json', import.meta.url), 'utf8'));
const digest = bytes => createHash('sha256').update(bytes).digest('hex');

// Exact inherited refusals only. Every ordinary compiler entry remains present;
// this permits identical native outputs for the specifically reviewed methods.
export function reviewedInheritedConstantOutput(type, entry, index, context) {
  if (entry.constantOutput === undefined) return false;
  const spec = reviewed[`${type.name}::${entry.api.trait}::${entry.api.name}`];
  if (!spec) return false;
  const label = `${type.name}: accepted-set assertion: reviewed inherited refusal`;
  assert.equal(entry.constantOutput, true, `${label}: constant flag must be true`);
  assert.equal(entry.unitInput, undefined, `${label}: no invented unit-input permission`);
  assert.ok(type.name === spec.owner && type.ident === spec.ident && type.kind === 'record' && type.deserialize === false, `${label}: actual compiled venue owner required`);
  assert.equal(type.rawEntrypoints?.[index], true, `${label}: actual indexed raw method required`);
  assert.deepEqual(entry.api, spec.api, `${label}: exact compiler API changed`);
  assert.ok(['safe-cex-venue-grvt', 'safe-cex-venue-breakoutprop'].includes(spec.package), `${label}: unreviewed venue package`);
  assert.ok(context.packageMetadata?.name === spec.package
    && context.packageMetadata.manifest_path === fileURLToPath(new URL(`../../${spec.package}/Cargo.toml`, import.meta.url)), `${label}: actual compiler package required`);
  assert.deepEqual(type.entrypointSources?.[index], { filename: spec.source.file, begin: spec.source.begin, end: spec.source.end }, `${label}: compiler method source changed`);
  assert.equal(typeof context.readSource, 'function', `${label}: actual source reader required`);
  assert.equal(digest(context.readSource(spec.source.file)), spec.source.sha256, `${label}: reviewed Rust source changed`);
  const base = spec.typescript.base;
  assert.equal(base.file, '../packages/safe-cex/src/exchanges/base.ts', `${label}: maintained base source required`);
  const baseBytes = context.readSource(base.file);
  assert.equal(digest(baseBytes), base.sha256, `${label}: maintained base source changed`);
  const excerpt = baseBytes.toString('utf8').split('\n').slice(base.begin[0] - 1, base.end[0]).join('\n');
  assert.equal(excerpt, base.excerpt, `${label}: exact maintained base method lines changed`);
  assert.ok(excerpt.trimStart().startsWith(`${base.method} = async`), `${label}: exact maintained base method required`);
  assert.ok(excerpt.includes("await Promise.reject(new Error('Not implemented'))"), `${label}: inherited rejection body required`);
  assert.equal(digest(context.readSource(spec.typescript.venue.file)), spec.typescript.venue.sha256, `${label}: reviewed venue inheritance changed`);
  assert.equal(digest(context.readSource(spec.fixture.file)), spec.fixture.sha256, `${label}: reviewed native fixture changed`);
  assert.deepEqual(entry.call, spec.call, `${label}: reviewed native adapter changed`);
  assert.deepEqual(entry.cases, spec.cases, `${label}: reviewed native witnesses changed`);
  assert.ok(spec.cases.length >= 2 && new Set(spec.cases.map(row => JSON.stringify(row.input))).size >= 2, `${label}: distinct genuine native inputs required`);
  assert.ok(spec.cases.every(row => row.output.error === 'Not implemented' && row.output.error_name === 'Error' && row.output.http === 0), `${label}: native inherited error and zero HTTP required`);
  return true;
}
