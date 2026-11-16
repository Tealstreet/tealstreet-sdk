import assert from 'node:assert/strict';
import { cpSync, existsSync, mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { dirname, join } from 'node:path';
import { spawnSync } from 'node:child_process';
import { fileURLToPath } from 'node:url';
import test from 'node:test';

const root = fileURLToPath(new URL('../../', import.meta.url));
const budgetPath = 'sdk-rs/harness/wasm-budget.mjs';
const policyPath = 'packages/safe-cex/src/exchanges/rust/construction-rules.json';
const budget = readFileSync(join(root, budgetPath), 'utf8');
const policy = readFileSync(join(root, policyPath), 'utf8');

function probe(t, artifact, source) {
  const scratch = mkdtempSync(join(tmpdir(), 'sdk-artifact-census-'));
  t.after(() => rmSync(scratch, { recursive: true, force: true }));
  for (const path of [budgetPath, policyPath, 'sdk-rs/harness/artifact-census.mjs',
    'sdk-rs/artifact-census.json', 'sdk-rs/harness/construction-features.test.mjs',
    'sdk-rs/hosts/wasm/Cargo.toml', 'sdk-rs/hosts/wasm/js']) {
    if (!existsSync(join(root, path))) continue;
    mkdirSync(dirname(join(scratch, path)), { recursive: true });
    cpSync(join(root, path), join(scratch, path), { recursive: true });
  }
  writeFileSync(join(scratch, artifact), source);
  const args = artifact === budgetPath
    ? ['--input-type=module', '-e', `import('./${budgetPath}')`]
    : ['--test', '--test-reporter=tap', '--test-concurrency=1', 'sdk-rs/harness/construction-features.test.mjs'];
  // The child owns its runner; inheriting node:test's context silently skips it.
  const env = { ...process.env };
  delete env.NODE_TEST_CONTEXT;
  const result = spawnSync(process.execPath, args, { cwd: scratch, env, encoding: 'utf8', timeout: 15_000 });
  if (artifact === policyPath && result.status === 0) assert.match(result.stdout, /^# tests [1-9]/m);
  return result;
}

function refuses(result, reason) {
  assert.ifError(result.error);
  assert.notEqual(result.status, 0, `mutated artifact was admitted: ${result.stdout.slice(0, 1500)} ${result.stderr.slice(0, 1500)}`);
  assert.match(result.stderr + result.stdout, reason);
}

function duplicatePolicy(name, value) {
  const end = policy.lastIndexOf('\n  }');
  assert.ok(end > 0);
  return `${policy.slice(0, end)},\n    "${name}": ${value}${policy.slice(end)}`;
}

test('unchanged complete budget imports and construction checks remain valid', (t) => {
  for (const [path, source] of [[budgetPath, budget], [policyPath, policy]]) {
    const result = probe(t, path, source);
    assert.ifError(result.error);
    assert.equal(result.status, 0, result.stderr + result.stdout);
  }
});

for (const value of ['2_043_904', '9_000_000']) {
  test(`raw budget refuses a duplicate venue before last-wins evaluation (${value})`, (t) => {
    const mutated = budget.replace('  weex: 2_043_904,', `  weex: 2_043_904,\n  weex: ${value},`);
    assert.notEqual(mutated, budget);
    refuses(probe(t, budgetPath, mutated), /duplicate.*weex/i);
  });
}

test('budget census refuses an unknown entry', (t) => {
  const mutated = budget.replace('  weex: 2_043_904,', '  weex: 2_043_904,\n  unreviewedvenue: 2_043_904,');
  refuses(probe(t, budgetPath, mutated), /unexpected.*unreviewedvenue/i);
});

test('budget census refuses a deleted entry before a default cap can hide it', (t) => {
  refuses(probe(t, budgetPath, budget.replace('  weex: 2_043_904,', '')), /missing.*weex/i);
});

for (const name of ['krakenspot', 'kraken\\u0073pot']) {
  test(`construction census refuses a duplicate policy losing Spot (${name})`, (t) => {
    refuses(probe(t, policyPath, duplicatePolicy(name, '{}')), /duplicate.*krakenspot/i);
  });
}

test('construction census refuses a deleted policy instead of shrinking its test list', (t) => {
  const changed = JSON.parse(policy);
  delete changed.venues.krakenspot;
  refuses(probe(t, policyPath, JSON.stringify(changed)), /missing.*krakenspot/i);
});

test('construction census refuses an unknown policy', (t) => {
  const changed = JSON.parse(policy);
  changed.venues.unreviewedvenue = {};
  refuses(probe(t, policyPath, JSON.stringify(changed)), /unexpected.*unreviewedvenue/i);
});

test('duplicate nested policy fields cannot choose which Spot value survives', (t) => {
  const mutated = policy.replace('"krakenspot": {"bridge": {"spot": true}}',
    '"krakenspot": {"bridge": {"spot": true, "spot": false}}');
  assert.notEqual(mutated, policy);
  refuses(probe(t, policyPath, mutated), /duplicate.*spot/i);
});

test('construction census treats object order as immaterial', (t) => {
  const changed = JSON.parse(policy);
  changed.venues = Object.fromEntries(Object.entries(changed.venues).reverse());
  const result = probe(t, policyPath, JSON.stringify(changed));
  assert.ifError(result.error);
  assert.equal(result.status, 0, result.stderr + result.stdout);
});
