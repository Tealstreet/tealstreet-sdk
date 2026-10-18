import assert from 'node:assert/strict';
import test from 'node:test';
import {measurementScope, scopedMeasurements} from './accepted-measure-scope.mjs';

const packageName = 'safe-cex-venue-binance';
const prefix = 'safe_cex_venue_binance::';
const scope = (...selectors) => measurementScope(['--measure', `--package=${packageName}`, ...selectors], packageName);
const owner = (name, begin = [1, 1], end = [10, 1]) => ({name: prefix + name,
  span: {filename: 'src/signing.rs', begin, end}});
const packages = [{package: packageName, types: [owner('signing::encode'), owner('signing::nested::parse'),
  owner('signing_extra::encode')], publicAliases: [owner('signing::Callback')],
defaultWitnesses: [owner('other::Defaults')], locals: [
  {file: 'src/signing.rs', line: 1, column: 0, label: 'inside'},
  {file: 'src/signing.rs', line: 10, column: 0, label: 'end-boundary'},
  {file: 'src/signing.rs', line: 10, column: 1, label: 'after'},
  {file: 'src/other.rs', line: 1, column: 0, label: 'different-file'},
], delegatedLocals: [{name: 'helper', runtimeOwner: prefix + 'signing::encode'},
  {name: 'other-helper', runtimeOwner: prefix + 'other::Defaults'}], declarations: ['full inventory']},
{package: 'safe-cex-exchange', types: [{name: 'safe_cex_exchange::signing::encode'}]}];

test('unscoped execution preserves the complete original inventory', () => {
  assert.equal(measurementScope(['--measure'], undefined), undefined);
  assert.equal(scopedMeasurements(packages, undefined, '/workspace'), packages);
});

test('module measurement includes descendants and aliases, excludes prefix siblings and dependencies', () => {
  const original = structuredClone(packages);
  const result = scopedMeasurements(packages, scope('--module=signing'), '/workspace');
  assert.equal(result.length, 1);
  assert.deepEqual(result[0].types.map(row => row.name),
    [prefix + 'signing::encode', prefix + 'signing::nested::parse']);
  assert.deepEqual(result[0].publicAliases.map(row => row.name), [prefix + 'signing::Callback']);
  assert.deepEqual(result[0].defaultWitnesses, []);
  assert.deepEqual(result[0].locals.map(row => row.label), ['inside', 'end-boundary', 'after']);
  assert.deepEqual(result[0].delegatedLocals.map(row => row.name), ['helper']);
  assert.deepEqual(result[0].declarations, ['full inventory']);
  assert.deepEqual(packages, original);
});

test('exact owner selectors union with modules without duplicate receipts', () => {
  const result = scopedMeasurements(packages, scope('--module=signing',
    `--owner=${prefix}signing::encode`, '--owner=other::Defaults'), '/workspace');
  assert.equal(result[0].types.length, 2);
  assert.deepEqual(result[0].defaultWitnesses.map(row => row.name), [prefix + 'other::Defaults']);
  const exact = scopedMeasurements(packages, scope('--owner=signing::encode'), '/workspace');
  assert.deepEqual(exact[0].types.map(row => row.name), [prefix + 'signing::encode']);
  assert.deepEqual(exact[0].publicAliases, []);
});

test('every selector must resolve even when another selector matches', () => {
  assert.throws(() => scopedMeasurements(packages, scope('--module=signing', '--module=missing'), '/workspace'),
    /module has no compiler owners/);
  assert.throws(() => scopedMeasurements(packages, scope('--module=signing', '--owner=missing'), '/workspace'),
    /owner absent from compiler inventory/);
  assert.throws(() => scopedMeasurements([], scope('--module=signing'), '/workspace'), /package absent/);
});

test('compiler owner namespaces are independent of Cargo package spelling', () => {
  const renamed = [{package: packageName, types: [{name: 'different_library::signing::encode'}]}];
  for (const selector of ['--module=signing', '--module=different_library::signing',
    '--owner=signing::encode', '--owner=different_library::signing::encode']) {
    assert.deepEqual(scopedMeasurements(renamed, scope(selector), '/workspace')[0].types, renamed[0].types);
  }
});

test('owner measurements preserve impl-local helpers outside the type declaration', () => {
  const type = owner('order_records::Iso8601', [8, 1], [11, 1]);
  const helper = {file: type.span.filename, line: 14, column: 32, label: 'TimestampVisitor'};
  const result = scopedMeasurements([{package: packageName, types: [type], locals: [helper]}],
    scope('--owner=order_records::Iso8601'), '/workspace');
  assert.deepEqual(result[0].locals, [helper]);
});

test('selectors cannot scope the normal gate or silently accept malformed requests', () => {
  assert.throws(() => measurementScope([`--package=${packageName}`, '--module=signing'], packageName), /require --measure/);
  assert.throws(() => measurementScope(['--measure', '--inventory', `--package=${packageName}`, '--owner=x'], packageName), /without --inventory/);
  assert.throws(() => measurementScope(['--measure', '--module=signing'], undefined), /exactly one --package/);
  assert.throws(() => scope('--module='), /nonempty names/);
  assert.throws(() => scope('--owner= '), /nonempty names/);
  assert.throws(() => scope('--module=sign*'), /exact Rust module paths/);
  assert.throws(() => scope('--owner=signing::encode', '--owner=signing::encode'), /duplicate/);
});
