import assert from 'node:assert/strict';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import test from 'node:test';
import { loadContracts } from './accepted-contract-files.mjs';

function fixture() {
  const root = fs.mkdtempSync(path.join(os.tmpdir(), 'accepted-contract-files-'));
  const directory = path.join(root, 'contracts');
  fs.mkdirSync(directory);
  const a = String.raw`{"fixture_core::Owner":{"shape":"observed","cases":[{"input":-0,"output":{"bits":"8000000000000000","text":"λ\\u{feff}"}}],"call":"match $value { _ => {\n true\n} }"}}`;
  const b = '{"fixture_venue::Alias":{"category":"non-decoder","reason":"typed callback","target":{"tuple":[]}}}';
  const legacy = path.join(root, 'legacy.json');
  fs.writeFileSync(path.join(directory, 'fixture-core.json'), a);
  fs.writeFileSync(path.join(directory, 'fixture-venue.json'), b);
  fs.writeFileSync(legacy, a.slice(0, -1) + ',' + b.slice(1));
  return { root, directory, legacy, close: () => fs.rmSync(root, { recursive: true, force: true }) };
}

test('per-crate contracts preserve every legacy value and explicit fixture file', () => {
  const f = fixture();
  try {
    const old = JSON.parse(fs.readFileSync(f.legacy, 'utf8'));
    assert.deepEqual(loadContracts(f.directory), old);
    assert.deepEqual(loadContracts(f.legacy), old);
    assert.ok(Object.is(loadContracts(f.directory)['fixture_core::Owner'].cases[0].input, -0));
    // The cutover command additionally compares the COMPLETE real old file,
    // including every Rust adapter byte, against the production loader. The
    // permanent fixture keeps the same equality proof for later loader edits.
    const legacyPath = process.argv.find(arg => arg.startsWith('--legacy-contracts='))?.slice('--legacy-contracts='.length);
    if (legacyPath) {
      const original = JSON.parse(fs.readFileSync(legacyPath, 'utf8'));
      assert.deepEqual(loadContracts(), original, 'complete production cutover must preserve every old contract byte semantically');
      console.log(`Complete production cutover equality: ${Object.keys(original).length} unchanged contracts`);
    }
  } finally { f.close(); }
});

test('contract directories reject missing malformed misplaced duplicate and hidden inputs', () => {
  const f = fixture();
  const a = path.join(f.directory, 'fixture-core.json');
  const b = path.join(f.directory, 'fixture-venue.json');
  const originalA = fs.readFileSync(a);
  const originalB = fs.readFileSync(b);
  try {
    assert.throws(() => loadContracts(path.join(f.root, 'absent')), /ENOENT/);
    for (const text of ['null', '[]', 'true', '{']) {
      fs.writeFileSync(a, text);
      assert.throws(() => loadContracts(f.directory));
    }
    fs.writeFileSync(a, '{}');
    assert.throws(() => loadContracts(f.directory), /empty contract shard/);
    fs.writeFileSync(a, '{"other::Owner":{}}');
    assert.throws(() => loadContracts(f.directory), /belongs in another crate file/);
    fs.writeFileSync(a, originalA);
    fs.writeFileSync(b, originalA);
    assert.throws(() => loadContracts(f.directory), /duplicate contract owner/);
    fs.writeFileSync(b, originalB);
    fs.writeFileSync(path.join(f.directory, 'notes.txt'), 'hidden');
    assert.throws(() => loadContracts(f.directory), /unexpected contract filename/);
    fs.rmSync(path.join(f.directory, 'notes.txt'));
    fs.rmSync(b);
    fs.symlinkSync(f.legacy, b);
    assert.throws(() => loadContracts(f.directory), /regular file/);
    fs.rmSync(b);
    fs.rmSync(a);
    assert.throws(() => loadContracts(f.directory), /must not be empty/);
  } finally { f.close(); }
});

test('contract shard namespace and complete merge checks reject executed mutations', async () => {
  const { spawnSync } = await import('node:child_process');
  const url = new URL('./accepted-contract-files.mjs', import.meta.url).href;
  const f = fixture();
  try {
    const foreign = path.join(f.root, 'foreign');
    fs.mkdirSync(foreign);
    fs.writeFileSync(path.join(foreign, 'fixture-core.json'), '{"another_crate::Owner":{}}');
    const cases = [
      ['contract shard namespace', "      assert.ok(key.startsWith(prefix) && key.length > prefix.length, `accepted-set: contract owner belongs in another crate file: ${key} in ${name}`);", '', `assert.throws(()=>loadContracts(${JSON.stringify(foreign)}),/belongs in another crate file/,'contract shard namespace must refuse');`],
      ['contract shard complete merge', '  return Object.fromEntries(entries);', '  return Object.fromEntries(entries.slice(1));', `assert.deepEqual(loadContracts(${JSON.stringify(f.directory)}),JSON.parse(fs.readFileSync(${JSON.stringify(f.legacy)},'utf8')),'contract shard complete merge must retain every owner');`],
    ];
    for (const [label, anchor, replacement, assertion] of cases) {
      const run = mutant => spawnSync(process.execPath, ['--input-type=module', '--eval', `import assert from 'node:assert/strict';import fs from 'node:fs';import {registerHooks} from 'node:module';${mutant ? `registerHooks({load(url,context,next){const result=next(url,context);if(url!==${JSON.stringify(url)})return result;const source=String(result.source),anchor=${JSON.stringify(anchor)};assert.equal(source.split(anchor).length,2);return {...result,source:source.replace(anchor,${JSON.stringify(replacement)})};}});` : ''}const {loadContracts}=await import(${JSON.stringify(url)});${assertion}`], { encoding: 'utf8' });
      let result = run(false); assert.equal(result.status, 0, result.stderr);
      result = run(true); assert.equal(result.status, 1, result.stderr);
      assert.match(result.stderr, new RegExp(label + ' must'));
      console.log(`${label}: named assertion; process exit 1`);
      result = run(false); assert.equal(result.status, 0, result.stderr);
    }
  } finally { f.close(); }
});
