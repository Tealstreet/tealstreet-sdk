import assert from 'node:assert/strict';
import { cpSync, existsSync, mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { dirname, join } from 'node:path';
import { spawnSync } from 'node:child_process';
import { fileURLToPath } from 'node:url';
import test from 'node:test';

const root = fileURLToPath(new URL('../../', import.meta.url));
const budgetPath = 'sdk-rs/harness/wasm-budget.mjs';
const capsPath = 'sdk-rs/harness/wasm-caps.json';
const policyPath = 'packages/safe-cex/src/exchanges/rust/construction-rules.json';
// AGT-23 moved the cap numbers from a JS literal in wasm-budget.mjs to the generated
// wasm-caps.json; these probes now mutate that file (read through readWasmCaps).
const caps = readFileSync(join(root, capsPath), 'utf8');
// Mutate the actual entry so generated cap updates cannot disable these probes.
const weexRow = caps.match(/^    "weex": ([0-9]+),$/m);
assert.ok(weexRow, 'WEEX cap fixture entry must exist');
const weexCap = Number(weexRow[1]);
const policy = readFileSync(join(root, policyPath), 'utf8');

function probe(t, artifact, source) {
  const scratch = mkdtempSync(join(tmpdir(), 'sdk-artifact-census-'));
  t.after(() => rmSync(scratch, { recursive: true, force: true }));
  for (const path of [budgetPath, capsPath, policyPath, 'sdk-rs/harness/artifact-census.mjs',
    'sdk-rs/artifact-census.json', 'sdk-rs/harness/construction-features.test.mjs',
    'sdk-rs/hosts/wasm/Cargo.toml', 'sdk-rs/hosts/wasm/js']) {
    if (!existsSync(join(root, path))) continue;
    mkdirSync(dirname(join(scratch, path)), { recursive: true });
    cpSync(join(root, path), join(scratch, path), { recursive: true });
  }
  writeFileSync(join(scratch, artifact), source);
  const args = artifact === capsPath
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
  for (const [path, source] of [[capsPath, caps], [policyPath, policy]]) {
    const result = probe(t, path, source);
    assert.ifError(result.error);
    assert.equal(result.status, 0, result.stderr + result.stdout);
  }
});

for (const value of [weexCap, 9_000_000]) {
  test(`generated caps refuse a duplicate family before last-wins evaluation (${value})`, (t) => {
    const mutated = caps.replace(weexRow[0], `${weexRow[0]}\n    "weex": ${value},`);
    assert.notEqual(mutated, caps);
    refuses(probe(t, capsPath, mutated), /duplicate.*weex/i);
  });
}

test('generated caps refuse an escaped duplicate spelling of a family', (t) => {
  const mutated = caps.replace(weexRow[0], `${weexRow[0]}\n    "we\\u0065x": 9000000,`);
  refuses(probe(t, capsPath, mutated), /duplicate.*weex/i);
});

test('budget census refuses an unknown entry', (t) => {
  const mutated = caps.replace(weexRow[0], `${weexRow[0]}\n    "unreviewedvenue": ${weexCap},`);
  refuses(probe(t, capsPath, mutated), /unexpected.*unreviewedvenue/i);
});

test('budget census refuses a deleted entry before a default cap can hide it', (t) => {
  refuses(probe(t, capsPath, caps.replace(`${weexRow[0]}\n`, '')), /missing.*weex/i);
});

for (const [label, value, reason] of [
  ['an unaligned cap', weexCap + 1, /invalid wasm cap: weex/],
  ['a zero cap', 0, /invalid wasm cap: weex/],
  ['a string cap', `"${weexCap}"`, /invalid wasm cap: weex/],
]) {
  test(`generated caps refuse ${label}`, (t) => {
    refuses(probe(t, capsPath, caps.replace(weexRow[0], `    "weex": ${value},`)), reason);
  });
}

test('generated caps refuse a changed reserve, schema or extra field', (t) => {
  refuses(probe(t, capsPath, caps.replace('"reserveBytes": 10240', '"reserveBytes": 0')), /reserve must stay 10 KiB/);
  refuses(probe(t, capsPath, caps.replace('"schema": 1', '"schema": 2')), /unsupported wasm caps schema/);
  refuses(probe(t, capsPath, caps.replace('"schema": 1,', '"schema": 1,\n  "venue": 1343488,')), /unexpected wasm caps field: venue/);
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

// The JS-literal grammar probes (inert template/quoted declarations, grouped maps)
// were retired by AGT-23 with the literal itself: caps are JSON read by readWasmCaps.
