import { loadPolicy } from './accepted-policy-files.mjs';
import assert from 'node:assert/strict';
import fs from 'node:fs';
import path from 'node:path';
import test from 'node:test';
import { fileURLToPath } from 'node:url';
import { spawnSync } from 'node:child_process';
import { reviewedConstantOutput } from './accepted-constant-output.mjs';
import { reviewedInheritedConstantOutput } from './accepted-inherited-constant-output.mjs';

const root = fileURLToPath(new URL('../..', import.meta.url));
const specs = Object.values(loadPolicy('inherited-constant-outputs'));
const grvtSpecs = specs.filter(spec => spec.package === 'safe-cex-venue-grvt');
function fixture(spec) {
  return {
    type: { name: spec.owner, ident: spec.ident, kind: 'record', deserialize: false, rawEntrypoints: [true], entrypointSources: [{ filename: spec.source.file, begin: spec.source.begin, end: spec.source.end }] },
    entry: { api: spec.api, constantOutput: true, input: 'observed', call: spec.call, cases: spec.cases },
    packageMetadata: { name: spec.package, manifest_path: path.join(root, spec.package, 'Cargo.toml') },
  };
}
const context = item => ({ packageMetadata: item.packageMetadata, readSource: file => fs.readFileSync(path.resolve(root, file)) });

test('reviewed inherited refusals retain exact compiler source fixture and native witnesses', () => {
  assert.deepEqual(grvtSpecs.map(spec => spec.api.name).sort(), [
    'change_margin_type', 'change_position_mode', 'fetch_public_trades', 'fetch_symbol_max_leverage',
  ], 'the four reviewed indexed GRVT methods remain installed');
  for (const spec of specs) {
    const item = fixture(spec);
    assert.equal(reviewedInheritedConstantOutput(item.type, item.entry, 0, context(item)), true);
    assert.equal(reviewedConstantOutput(item.type, item.entry, 0, context(item)), true);
    assert.equal(reviewedInheritedConstantOutput({ ...item.type, name: 'unreviewed::Owner' }, item.entry, 0, context(item)), false);
    assert.equal(reviewedInheritedConstantOutput(item.type, { ...item.entry, constantOutput: undefined }, 0, context(item)), false);
    for (const [mutate, message] of [
      [x => { x.type.ident = 'Other'; }, /actual compiled venue owner/],
      [x => { x.type.deserialize = true; }, /actual compiled venue owner/],
      [x => { x.type.rawEntrypoints[0] = false; }, /actual indexed raw method/],
      [x => { x.entry.api.signature.output = null; }, /exact compiler API/],
      [x => { x.type.entrypointSources[0].begin[0]++; }, /compiler method source/],
      [x => { x.packageMetadata.name = 'safe-cex-num'; }, /actual compiler package/],
      [x => { x.packageMetadata.manifest_path = '/unreviewed/Cargo.toml'; }, /actual compiler package/],
      [x => { x.entry.unitInput = true; }, /no invented unit-input/],
      [x => { x.entry.constantOutput = false; }, /flag must be true/],
      [x => { x.entry.call += ' '; }, /native adapter changed/],
      [x => { x.entry.cases[0].output.http = 1; }, /native witnesses changed/],
    ]) {
      const changed = structuredClone(item); mutate(changed);
      assert.throws(() => reviewedConstantOutput(changed.type, changed.entry, 0, context(changed)), message);
    }
    for (const [file, message] of [
      [spec.source.file, /reviewed Rust source/],
      [spec.typescript.base.file, /maintained base source/],
      [spec.typescript.venue.file, /venue inheritance/],
      [spec.fixture.file, /native fixture/],
    ]) {
      const actual = context(item);
      assert.throws(() => reviewedConstantOutput(item.type, item.entry, 0, { ...actual, readSource: selected => selected === file ? Buffer.concat([actual.readSource(selected), Buffer.from([10])]) : actual.readSource(selected) }), message);
    }
  }
});

test('reviewed inherited refusal source fixture and adapter pins reject executed mutations', () => {
  const spec = grvtSpecs[0];
  const moduleUrl = new URL('./accepted-inherited-constant-output.mjs', import.meta.url).href;
  const entryUrl = new URL('./accepted-constant-output.mjs', import.meta.url).href;
  const cases = [
    ['inherited refusal maintained source pin', "  assert.equal(digest(baseBytes), base.sha256, `${label}: maintained base source changed`);", spec.typescript.base.file, /maintained base source changed/],
    ['inherited refusal native fixture pin', "  assert.equal(digest(context.readSource(spec.fixture.file)), spec.fixture.sha256, `${label}: reviewed native fixture changed`);", spec.fixture.file, /native fixture changed/],
    ['inherited refusal native adapter pin', "  assert.deepEqual(entry.call, spec.call, `${label}: reviewed native adapter changed`);", null, /native adapter changed/],
  ];
  for (const [label, anchor, alteredFile, refusal] of cases) {
    const item = fixture(spec);
    if (!alteredFile) {
      const needle = 'let state_unchanged=before==snapshot();';
      assert.equal(item.entry.call.split(needle).length, 2);
      item.entry.call = item.entry.call.replace(needle, 'let state_unchanged=true;');
    }
    const run = mutant => {
      const hook = mutant ? `import {registerHooks} from 'node:module';registerHooks({load(url,context,next){const loaded=next(url,context);if(url!==${JSON.stringify(moduleUrl)})return loaded;const source=String(loaded.source),anchor=${JSON.stringify(anchor)};assert.equal(source.split(anchor).length,2);return {...loaded,source:source.replace(anchor,'')};}});` : '';
      return spawnSync(process.execPath, ['--input-type=module', '--eval', `import assert from 'node:assert/strict';import fs from 'node:fs';import path from 'node:path';${hook}const {reviewedConstantOutput}=await import(${JSON.stringify(entryUrl)});const item=${JSON.stringify(item)};const context={packageMetadata:item.packageMetadata,readSource:file=>{const bytes=fs.readFileSync(path.resolve(${JSON.stringify(root)},file));return file===${JSON.stringify(alteredFile)}?Buffer.concat([bytes,Buffer.from([10])]):bytes;}};assert.throws(()=>reviewedConstantOutput(item.type,item.entry,0,context),${refusal},${JSON.stringify(label + ' must refuse')});`], { encoding: 'utf8' });
    };
    let result = run(false); assert.equal(result.status, 0, result.stderr);
    result = run(true); assert.equal(result.status, 1, result.stderr);
    assert.match(result.stderr, new RegExp(label + ' must refuse'));
    console.log(`${label}: named assertion; process exit 1`);
    result = run(false); assert.equal(result.status, 0, result.stderr);
  }
});
