import assert from 'node:assert/strict';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import test from 'node:test';
import { loadContracts } from './accepted-contract-files.mjs';
import { parseJsonWithSource, stringifyJsonWithSource, copyJsonSourceProperty } from './accepted-json-source.mjs';

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

test('authored JSON tokens retain integer key order, numbers and escaped keys', () => {
  const text = String.raw`{"input":{"9":9,"2":2,"x":-0,"huge":9007199254740993,"overflow":1e400,"\u0061":1,"__proto__":{"z":true}},"output":null}`;
  const row = parseJsonWithSource(text);
  assert.deepEqual(row, JSON.parse(text), 'source retention cannot change contract semantics');
  assert.equal(stringifyJsonWithSource(row), text);
  assert.equal(stringifyJsonWithSource(row.input), text.slice(9, -15));
  assert.ok(Object.hasOwn(row.input, '__proto__'));
  assert.equal(Object.getPrototypeOf(row.input), Object.prototype);
  row.output = true;
  assert.equal(stringifyJsonWithSource(row), text.replace('"output":null', '"output":true'));
  row.input.x = 0;
  assert.equal(JSON.parse(stringifyJsonWithSource(row)).input.x, 0);
  assert.doesNotMatch(stringifyJsonWithSource(row), /"x":-0/, 'changed values must never reuse stale raw input');
  row.input.x = -0;
  assert.equal(stringifyJsonWithSource(row), text.replace('"output":null', '"output":true'));
  for (const invalid of ['{', '[1,]', '{"x":undefined}', 'null true']) assert.throws(() => parseJsonWithSource(invalid), SyntaxError);
  const f = fixture();
  try {
    fs.writeFileSync(path.join(f.directory, 'fixture-core.json'), `{"fixture_core::Owner":${text}}`);
    assert.equal(stringifyJsonWithSource(loadContracts(f.directory)['fixture_core::Owner']), text, 'production loader preserves authored case text');
  } finally { f.close(); }
});

test('source-aware JSON rejects duplicate decoded keys at every nesting level', () => {
  for (const text of ['{"x":1,"x":2}', '{"input":{"x":1,"\\u0078":2}}', '[{"x":1,"x":2}]']) {
    assert.throws(() => parseJsonWithSource(text), /duplicate JSON key: x/);
  }
});

test('source-aware JSON retains programmatic and edited negative zero', () => {
  assert.equal(stringifyJsonWithSource({ input: -0 }), '{"input":-0}');
  const row = parseJsonWithSource('{"input":1,"output":true}');
  row.input = -0;
  assert.equal(stringifyJsonWithSource(row), '{"input":-0,"output":true}');
});

test('source-aware JSON retains authored integer-key order after edits', () => {
  const row = parseJsonWithSource('{"9":9,"2":2,"x":1}');
  row.x = 3;
  assert.equal(stringifyJsonWithSource(row), '{"9":9,"2":2,"x":3}');
  row.extra = false;
  assert.equal(stringifyJsonWithSource(row), '{"9":9,"2":2,"x":3,"extra":false}');
});

test('generated case wrappers retain nested and primitive authored input tokens', () => {
  for (const raw of ['{"9":9,"2":2,"x":1}', '-0', '9007199254740993', '1e400']) {
    const row = parseJsonWithSource(`{"input":${raw},"output":true}`);
    const input = copyJsonSourceProperty({ entrypoint: 3, input: row.input }, 'input', row);
    const wrapped = { input, output: row.output };
    assert.equal(stringifyJsonWithSource(wrapped), `{"input":{"entrypoint":3,"input":${raw}},"output":true}`);
    input.input = 'changed';
    assert.equal(JSON.parse(stringifyJsonWithSource(wrapped)).input.input, 'changed');
  }
});

test('raw contract and dispatch token retention reject executed loader mutations', async () => {
  const { spawnSync } = await import('node:child_process');
  const loader = new URL('./accepted-contract-files.mjs', import.meta.url).href;
  const tokens = new URL('./accepted-json-source.mjs', import.meta.url).href;
  const f = fixture();
  try {
    fs.writeFileSync(f.legacy, '{"fixture_core::Owner":{"cases":[{"input":{"9":9,"2":2,"x":1},"output":true}]}}');
    const mutations = [
      ['authored JSON contract source retention', loader, "const value = parseJsonWithSource(fs.readFileSync(file, 'utf8'));", "const value = JSON.parse(fs.readFileSync(file, 'utf8'));",
        `const {loadContracts}=await import(${JSON.stringify(loader)});assert.match(stringifyJsonWithSource(loadContracts(${JSON.stringify(f.legacy)})),/"input":\\{"9":9,"2":2,"x":1\\}/,'authored JSON contract source retention must preserve order');`],
      ['authored JSON dispatch token retention', tokens, '  if (node) {', '  if (false) {',
        "const row=parseJsonWithSource('{\"input\":9007199254740993}');const wrapped=copyJsonSourceProperty({input:row.input},'input',row);assert.equal(stringifyJsonWithSource(wrapped),'{\"input\":9007199254740993}','authored JSON dispatch token retention must preserve primitive');"],
    ];
    for (const [label, module, anchor, replacement, assertion] of mutations) {
      const execute = mutate => spawnSync(process.execPath, ['--input-type=module', '--eval', `import assert from 'node:assert/strict';import {registerHooks} from 'node:module';${mutate ? `registerHooks({load(url,context,next){const result=next(url,context);if(url!==${JSON.stringify(module)})return result;const source=String(result.source),anchor=${JSON.stringify(anchor)};assert.equal(source.split(anchor).length,2);return {...result,source:source.replace(anchor,${JSON.stringify(replacement)})};}});` : ''}const {parseJsonWithSource,stringifyJsonWithSource,copyJsonSourceProperty}=await import(${JSON.stringify(tokens)});${assertion}`], { encoding: 'utf8' });
      let result = execute(false); assert.equal(result.status, 0, result.stderr);
      result = execute(true); assert.equal(result.status, 1, result.stderr); assert.match(result.stderr, new RegExp(label + ' must'));
      console.log(`${label}: named assertion; process exit 1`);
      result = execute(false); assert.equal(result.status, 0, result.stderr);
    }
  } finally { f.close(); }
});

test('per-crate contracts preserve every legacy value and explicit fixture file', () => {
  const f = fixture();
  try {
    const old = JSON.parse(fs.readFileSync(f.legacy, 'utf8'));
    assert.deepEqual(loadContracts(f.directory), old);
    assert.deepEqual(loadContracts(f.legacy), old);
    assert.ok(Object.is(loadContracts(f.directory)['fixture_core::Owner'].cases[0].input, -0));
    const production = loadContracts();
    assert.deepEqual(JSON.parse(stringifyJsonWithSource(production)), production, 'all production contract values survive source-retaining serialization');
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

test('native contract serialization retains authored nested keys and signed zero', async () => {
  const { contractJson } = await import('./accepted-contract-files.mjs');
  const f = fixture();
  try {
    const text = String.raw`{"fixture_core::Owner":{"cases":[{"input":{"9":9,"2":2,"x":[{"10":10,"1":1,"text":"escaped \\\" {} [] : ,"}],"zero":-0},"output":null}]}}`;
    fs.writeFileSync(f.legacy, text);
    const contract = loadContracts(f.legacy)['fixture_core::Owner'];
    const input = contract.cases[0].input;
    const expected = String.raw`{"9":9,"2":2,"x":[{"10":10,"1":1,"text":"escaped \\\" {} [] : ,"}],"zero":-0}`;
    assert.equal(contractJson(input), expected, 'authored order must survive at every nesting level');
    assert.equal(contractJson({ generated: input }), `{"generated":${expected}}`, 'generated dispatch wrappers retain authored children');
    input.extra = false;
    delete input['9'];
    const encoded = contractJson(input);
    assert.ok(encoded.startsWith('{"2":2,"x":['));
    assert.ok(encoded.endsWith(',"zero":-0,"extra":false}'), 'serialization must observe current fields rather than stale source bytes');
    fs.writeFileSync(f.legacy, '{"fixture_core::Owner":{"x":1,"\\u0078":2}}');
    assert.throws(() => loadContracts(f.legacy), /duplicate JSON key: x/);
  } finally { f.close(); }
});
