import assert from 'node:assert/strict';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { spawnSync } from 'node:child_process';
import test from 'node:test';
import { loadPolicy, policyCrate } from './accepted-policy-files.mjs';

function fixture() {
  const root = fs.mkdtempSync(path.join(os.tmpdir(), 'accepted-policy-files-'));
  const directory = path.join(root, 'reviewed');
  fs.mkdirSync(directory);
  const compiler = String.raw`{"fixture_core::Owner":{"cases":[{"input":-0}],"call":"$type($value)\nλ\\u{feff}"}}`;
  const source = '{"fixture-venue/src/private/wire.rs::LOCAL_FUNCTION:compare":{"reason":"typed native comparison","sourceSha256":"literal"}}';
  fs.writeFileSync(path.join(directory, 'fixture-core.json'), compiler);
  fs.writeFileSync(path.join(directory, 'fixture-venue.json'), source);
  return { root, directory, old: JSON.parse(compiler.slice(0, -1) + ',' + source.slice(1)), close: () => fs.rmSync(root, { recursive: true, force: true }) };
}

test('per-crate policy tables preserve compiler and source records exactly', () => {
  const f = fixture();
  try {
    assert.deepEqual(loadPolicy('reviewed', f.root), f.old);
    assert.ok(Object.is(loadPolicy('reviewed', f.root)['fixture_core::Owner'].cases[0].input, -0));
    assert.equal(policyCrate('fixture_core::Trait::method for Other'), 'fixture-core');
    assert.equal(policyCrate('fixture-venue/src/private/wire.rs::LOCAL_FUNCTION:compare'), 'fixture-venue');
    // The direct cutover supplies the COMPLETE original production tables;
    // no permanently stale snapshot constrains later reviewed row additions.
    if (process.env.ACCEPTED_LEGACY_POLICY_DIRECTORY) {
      let records = 0;
      for (const file of fs.readdirSync(process.env.ACCEPTED_LEGACY_POLICY_DIRECTORY).sort()) {
        assert.match(file, /^[a-z-]+\.json$/);
        const old = JSON.parse(fs.readFileSync(path.join(process.env.ACCEPTED_LEGACY_POLICY_DIRECTORY, file), 'utf8'));
        const merged = loadPolicy(file.slice(0, -5));
        assert.deepEqual(merged, old, `complete production ${file} cutover equality`);
        for (const key of Object.keys(old)) {
          assert.equal(JSON.stringify(merged[key]), JSON.stringify(old[key]), `complete production ${key} record order and adapter bytes`);
        }
        records += Object.keys(old).length;
      }
      console.log(`Complete production policy cutover equality: ${records} unchanged records`);
    }
  } finally { f.close(); }
});

test('policy shards refuse missing malformed misplaced duplicate symlink and obsolete inputs', () => {
  const f = fixture(), a = path.join(f.directory, 'fixture-core.json'), b = path.join(f.directory, 'fixture-venue.json');
  const originalA = fs.readFileSync(a), originalB = fs.readFileSync(b);
  try {
    assert.throws(() => loadPolicy('../reviewed', f.root), /invalid policy table/);
    assert.throws(() => loadPolicy('absent', f.root), /ENOENT/);
    for (const value of ['null', '[]', 'true', '{']) {
      fs.writeFileSync(a, value);
      assert.throws(() => loadPolicy('reviewed', f.root));
    }
    fs.writeFileSync(a, '{}');
    assert.throws(() => loadPolicy('reviewed', f.root), /empty policy shard/);
    fs.writeFileSync(a, '{"foreign::Owner":{}}');
    assert.throws(() => loadPolicy('reviewed', f.root), /belongs in another crate file/);
    fs.writeFileSync(a, '{"foreign-crate/src/lib.rs::LOCAL_FUNCTION:helper":{}}');
    assert.throws(() => loadPolicy('reviewed', f.root), /belongs in another crate file/);
    fs.writeFileSync(a, '{"fixture-core/../other/lib.rs::Hidden":{}}');
    assert.throws(() => loadPolicy('reviewed', f.root), /unsupported policy owner/);
    fs.writeFileSync(a, originalA);
    fs.writeFileSync(b, originalA);
    assert.throws(() => loadPolicy('reviewed', f.root), /duplicate policy owner/);
    fs.writeFileSync(b, originalB);
    fs.writeFileSync(path.join(f.directory, 'notes.txt'), 'hidden');
    assert.throws(() => loadPolicy('reviewed', f.root), /unexpected policy filename/);
    fs.rmSync(path.join(f.directory, 'notes.txt'));
    const legacy = path.join(f.root, 'reviewed.json');
    fs.writeFileSync(legacy, '{}');
    assert.throws(() => loadPolicy('reviewed', f.root), /obsolete flat policy/);
    fs.rmSync(legacy);
    fs.rmSync(b);
    fs.symlinkSync(a, b);
    assert.throws(() => loadPolicy('reviewed', f.root), /regular file/);
    fs.rmSync(b); fs.rmSync(a);
    assert.throws(() => loadPolicy('reviewed', f.root), /must not be empty/);
  } finally { f.close(); }
});

test('policy shard ownership complete merge and direct cutover reject executed mutations', () => {
  const f = fixture();
  const module = new URL('./accepted-policy-files.mjs', import.meta.url).href;
  try {
    const foreign = path.join(f.root, 'foreign'); fs.mkdirSync(foreign);
    fs.writeFileSync(path.join(foreign, 'fixture-core.json'), '{"foreign::Owner":{}}');
    const legacyRoot = path.join(f.root, 'legacy'); fs.mkdirSync(legacyRoot);
    fs.mkdirSync(path.join(legacyRoot, 'reviewed'));
    fs.copyFileSync(path.join(f.directory, 'fixture-core.json'), path.join(legacyRoot, 'reviewed/fixture-core.json'));
    fs.writeFileSync(path.join(legacyRoot, 'reviewed.json'), '{}');
    // Preserve the fixture's literal -0 in the mutation worker expectation.
    const expected = path.join(f.root, 'expected.json');
    fs.writeFileSync(expected, fs.readFileSync(path.join(f.directory, 'fixture-core.json'), 'utf8').slice(0, -1) + ',' + fs.readFileSync(path.join(f.directory, 'fixture-venue.json'), 'utf8').slice(1));
    const cases = [
      ['policy shard ownership', '      assert.equal(policyCrate(key), file.slice(0, -5), `accepted-set: policy owner belongs in another crate file: ${key} in ${file}`);', '', `assert.throws(()=>loadPolicy('foreign',${JSON.stringify(f.root)}),/belongs in another crate file/,'policy shard ownership must refuse');`],
      ['policy shard complete merge', '  return Object.fromEntries(entries);', '  return Object.fromEntries(entries.slice(1));', `assert.deepEqual(loadPolicy('reviewed',${JSON.stringify(f.root)}),JSON.parse(fs.readFileSync(${JSON.stringify(expected)},'utf8')),'policy shard complete merge must retain every record');`],
      ['policy shard direct cutover', "  assert.ok(!fs.existsSync(path.join(root, name + '.json')), `accepted-set: obsolete flat policy must stay deleted: ${name}.json`);", '', `assert.throws(()=>loadPolicy('reviewed',${JSON.stringify(legacyRoot)}),/obsolete flat policy/,'policy shard direct cutover must refuse a restored flat file');`],
    ];
    for (const [label, anchor, replacement, assertion] of cases) {
      const run = mutant => spawnSync(process.execPath, ['--input-type=module', '--eval', `import assert from 'node:assert/strict';import fs from 'node:fs';import {registerHooks} from 'node:module';${mutant ? `registerHooks({load(url,context,next){const loaded=next(url,context);if(url!==${JSON.stringify(module)})return loaded;const source=String(loaded.source),anchor=${JSON.stringify(anchor)};assert.equal(source.split(anchor).length,2);return {...loaded,source:source.replace(anchor,${JSON.stringify(replacement)})};}});` : ''}const {loadPolicy}=await import(${JSON.stringify(module)});${assertion}`], { encoding: 'utf8' });
      let result = run(false); assert.equal(result.status, 0, result.stderr);
      result = run(true); assert.equal(result.status, 1, result.stderr);
      assert.match(result.stderr, new RegExp(label + ' must'));
      console.log(`${label}: named assertion; process exit 1`);
      result = run(false); assert.equal(result.status, 0, result.stderr);
    }
  } finally { f.close(); }
});
