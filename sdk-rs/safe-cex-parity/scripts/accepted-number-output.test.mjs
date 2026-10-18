import assert from 'node:assert/strict';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';
import { spawnSync } from 'node:child_process';
import test, { before } from 'node:test';
import { buildScanner } from './accepted-scanner.mjs';
import { verifiedNumberOutputOrigin, verifiedNumberOutputDefinition, numberOutputDefinitionCovers } from './accepted-number-output.mjs';

const sdk = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '../..');
let scanner;
before(() => { scanner = buildScanner(); });
function fixture(check) {
  const root = fs.mkdtempSync(path.join(os.tmpdir(), 'number-output-control-'));
  try {
    const artifact = path.join(root, 'libsafe_cex_num.rmeta');
    fs.writeFileSync(artifact, 'synthetic compiler-path unit fixture; not a native compilation');
    const provider = {
      id: 'sdk-num', name: 'safe-cex-num', source: null,
      manifest_path: path.join(sdk, 'safe-cex-num/Cargo.toml'),
      targets: [{ name: 'safe_cex_num', kind: ['lib'], src_path: path.join(sdk, 'safe-cex-num/src/lib.rs') }],
    };
    const context = {
      consumer: { id: 'consumer', edition: '2024' },
      metadata: { packages: [provider], workspace_members: [provider.id], resolve: { nodes: [{ id: 'consumer', deps: [{ name: 'safe_cex_num', pkg: provider.id }] }] } },
      doc: { external_crates: { 1: { name: 'safe_cex_num', path: artifact } } },
      artifacts: [{ package_id: provider.id, profile: { test: false }, target: provider.targets[0], filenames: [artifact] }],
    };
    check(root, context);
  } finally { fs.rmSync(root, { recursive: true, force: true }); }
}
const capability = (context, tool = scanner) => verifiedNumberOutputOrigin(context.doc, context.consumer, context.metadata, context.artifacts, tool);

test('numeric expression permission requires the canonical SDK compiler binding', () => fixture((root, original) => {
  assert.deepEqual(capability(original), ['safe_cex_num::js_number_output']);
  for (const [mutate, assertion] of [
    [value => { value.consumer.edition = '2015'; }, /supported consumer edition/],
    [value => { value.metadata.resolve.nodes[0].deps.push(value.metadata.resolve.nodes[0].deps[0]); }, /unique dependency binding/],
    [value => { value.metadata.packages[0].manifest_path = path.join(root, 'not-the-sdk.toml'); }, /canonical SDK package/],
    [value => { value.metadata.workspace_members = []; }, /SDK workspace member/],
    [value => { value.metadata.packages[0].targets[0].src_path = path.join(root, 'not-the-lib.rs'); }, /canonical provider library/],
    [value => { value.artifacts[0].package_id = 'same-spelling-vendor'; }, /actual consumer compiler artifact/],
    [value => { value.artifacts[0].profile.test = true; }, /actual consumer compiler artifact/],
    [value => { value.artifacts.push(value.artifacts[0]); }, /actual consumer compiler artifact/],
    [value => { value.doc.external_crates[2] = value.doc.external_crates[1]; }, /actual consumer compiler artifact/],
    [value => { value.doc.external_crates[1].path = path.join(root, 'missing.rmeta'); }, /actual consumer compiler artifact/],
  ]) {
    const changed = structuredClone(original); mutate(changed);
    assert.throws(() => capability(changed), assertion);
  }
  const renamed = structuredClone(original); renamed.metadata.resolve.nodes[0].deps[0].name = 'other_num';
  assert.deepEqual(capability(renamed), [], 'renamed bindings receive no capability');
  assert.deepEqual(capability(original), ['safe_cex_num::js_number_output']);
  const provider = original.metadata.packages[0];
  const doc = { root: 0, index: { 0: { crate_id: 0, name: 'safe_cex_num', span: { filename: 'safe-cex-num/src/lib.rs' }, inner: { module: {} } } } };
  const definition = verifiedNumberOutputDefinition(doc, provider, original.metadata, original.artifacts, scanner);
  assert.equal(definition.file, fs.realpathSync(path.join(sdk, 'safe-cex-num/src/numeric_ingress/eligibility.rs')));
  assert.equal(definition.ident, 'UNSUPPORTED_MODULE_MACRO:js_number_output');
  assert.equal(definition.selfOrigin, 'safe_cex_num::js_number_output::self');
  assert.ok(definition.line > 0 && definition.column > 0);
  assert.equal(verifiedNumberOutputDefinition(doc, original.consumer, original.metadata, original.artifacts, scanner), null);
  const mismatched = structuredClone(doc); mismatched.index[0].span.filename = 'safe-cex-num/src/bin/fake.rs';
  assert.throws(() => verifiedNumberOutputDefinition(mismatched, provider, original.metadata, original.artifacts, scanner), /actual definition compiler owner required/);
  const wrong = structuredClone(original.artifacts); wrong[0].package_id = 'wrong-provider';
  assert.throws(() => verifiedNumberOutputDefinition(doc, provider, original.metadata, wrong, scanner), /actual definition compiler artifact required/);
}));

test('numeric provider recipe and unconditional source route reject executed mutations', () => fixture((root, original) => {
  const copiedSdk = path.join(root, 'sdk-rs');
  for (const relative of ['safe-cex-num/Cargo.toml', 'safe-cex-num/src/lib.rs', 'safe-cex-num/src/numeric_ingress.rs', 'safe-cex-num/src/numeric_ingress/eligibility.rs', 'safe-cex-parity/scripts/accepted-number-output.mjs']) {
    fs.mkdirSync(path.dirname(path.join(copiedSdk, relative)), { recursive: true });
    fs.copyFileSync(path.join(sdk, relative), path.join(copiedSdk, relative));
  }
  const context = structuredClone(original);
  context.metadata.packages[0].manifest_path = path.join(copiedSdk, 'safe-cex-num/Cargo.toml');
  context.metadata.packages[0].targets[0].src_path = path.join(copiedSdk, 'safe-cex-num/src/lib.rs');
  context.artifacts[0].target.src_path = context.metadata.packages[0].targets[0].src_path;
  const worker = `import fs from 'node:fs'; import assert from 'node:assert/strict'; import { verifiedNumberOutputOrigin, verifiedNumberOutputDefinition, numberOutputDefinitionCovers } from ${JSON.stringify(pathToFileURL(path.join(copiedSdk, 'safe-cex-parity/scripts/accepted-number-output.mjs')).href)}; const value = JSON.parse(fs.readFileSync(process.argv[1])); verifiedNumberOutputOrigin(value.doc, value.consumer, value.metadata, value.artifacts, process.argv[2]); const doc = { root:0, index:{0:{crate_id:0,name:'safe_cex_num',span:{filename:'safe-cex-num/src/lib.rs'},inner:{module:{}}}}}; const definition = verifiedNumberOutputDefinition(doc, value.metadata.packages[0], value.metadata, value.artifacts, process.argv[2]); const scan = (await import('node:child_process')).spawnSync(process.argv[2], ['--macro-origins-json','[]',definition.file], {encoding:'utf8'}); assert.equal(scan.status,0,scan.stderr); const uncovered=scan.stdout.trim().split('\\n').filter(Boolean).map(line=>line.split('\\t')).filter(([file,ident,row,column])=>ident.startsWith('UNSUPPORTED_')&&!numberOutputDefinitionCovers(definition,file,ident,row,column)); assert.deepEqual(uncovered,[], 'unreviewed numeric definition scanner events');`;
  const input = path.join(root, 'compiler-context.json');
  fs.writeFileSync(input, JSON.stringify(context));
  const execute = () => spawnSync(process.execPath, ['--input-type=module', '--eval', worker, input, scanner], { encoding: 'utf8' });
  const baseline = () => { const result = execute(); assert.equal(result.status, 0, result.stderr); };
  baseline();
  for (const [relative, before, after, label, assertion] of [
    ['safe-cex-num/src/numeric_ingress/eligibility.rs', 'let Self { $($field,)* $($opaque),+ }', 'let _review_control = (); let Self { $($field,)* $($opaque),+ }', 'numeric provider recipe changed', /recipe changed; expression-macro review required/],
    ['safe-cex-num/src/numeric_ingress.rs', 'mod eligibility;', '#[cfg(not(doc))] mod eligibility;', 'numeric provider conditional route', /unconditional source route required/],
    ['safe-cex-num/src/numeric_ingress/eligibility.rs', '#[macro_export]', '#[macro_export(local_inner_macros)]', 'numeric provider export attributes', /unconditional source route required/],
    ['safe-cex-num/src/numeric_ingress/eligibility.rs', '#[macro_export]', 'mod duplicate { macro_rules! js_number_output { () => {} } }\n#[macro_export]', 'numeric provider nested duplicate declaration', /unique reviewed macro definition required/],
    ['safe-cex-num/src/numeric_ingress/eligibility.rs', '#[macro_export]', '#[macro_export] macro_rules! unreviewed_export { () => {} }\n#[macro_export]', 'numeric provider unrelated export attribute', /unreviewed numeric definition scanner events/],
  ]) {
    const file = path.join(copiedSdk, relative), source = fs.readFileSync(file, 'utf8');
    assert.equal(source.split(before).length, 2, 'mutation selects exactly one maintained source occurrence');
    try {
      fs.writeFileSync(file, source.replace(before, after));
      const result = execute();
      assert.equal(result.status, 1, result.stderr);
      assert.match(result.stderr, assertion);
      assert.doesNotMatch(result.stderr, /could not compile|error\[E\d+\]/);
      console.log(`${label}: named assertion; process exit 1`);
    } finally { fs.writeFileSync(file, source); }
    baseline();
  }
}));

test('numeric source inspection refuses opaque arguments later shadows and every glob', () => fixture((root) => {
  const file = path.join(root, 'source.rs');
  const positive = 'fn check(&self) { safe_cex_num::js_number_output!(self, struct { candles, last_accessed }); }';
  const scan = (source, origins = ['safe_cex_num::js_number_output']) => {
    fs.writeFileSync(file, source);
    const result = spawnSync(scanner, ['--macro-origins-json', JSON.stringify(origins), file], { encoding: 'utf8' });
    assert.equal(result.status, 0, result.stderr);
    return result.stdout;
  };
  assert.equal(scan(positive), '');
  assert.match(scan(positive, []), /UNSUPPORTED_NUMBER_OUTPUT_ORIGIN_OR_GRAMMAR/);
  for (const suffix of ['use hidden::*;', 'use std::*;', 'use nested::{Item, hidden::*};']) {
    assert.match(scan(positive + suffix), /UNSUPPORTED_NUMBER_OUTPUT_IMPORT_SCOPE/, `later glob: ${suffix}`);
  }
  for (const suffix of ['mod safe_cex_num {}', 'mod r#safe_cex_num {}', 'use hidden as safe_cex_num;', 'use hidden::safe_cex_num::{self};', 'use hidden::{r#safe_cex_num::{self}};', 'extern crate hidden as safe_cex_num;']) {
    assert.match(scan(positive + suffix), /UNSUPPORTED_MACRO_NAMESPACE_SHADOW/, `later shadow: ${suffix}`);
  }
  for (const operand of ['(self)', '{ #[cfg(not(doc))] impl Hidden {} self }', 'vec!({ #[cfg(not(doc))] impl Hidden {} self })']) {
    const source = positive.replace('!(self,', `!(${operand},`);
    assert.match(scan(source), /UNSUPPORTED_NUMBER_OUTPUT_ORIGIN_OR_GRAMMAR/);
    const guard = spawnSync(scanner, ['--doc-guards', file], { encoding: 'utf8' });
    assert.equal(guard.status, 1, guard.stderr);
    assert.match(guard.stderr, /DOC_CONFIGURATION changes production discovery/);
  }
  assert.equal(scan(positive), '');
}));


test('numeric definition coverage retains exact export and declaration positions', () => fixture((root, original) => {
  const provider = original.metadata.packages[0];
  const doc = { root: 0, index: { 0: { crate_id: 0, name: 'safe_cex_num', span: { filename: 'safe-cex-num/src/lib.rs' }, inner: { module: {} } } } };
  const definition = verifiedNumberOutputDefinition(doc, provider, original.metadata, original.artifacts, scanner);
  const context = path.join(root, 'definition.json');
  fs.writeFileSync(context, JSON.stringify(definition));
  const module = new URL('./accepted-number-output.mjs', import.meta.url).href;
  for (const [label, anchor, replacement, assertion] of [
    ['numeric definition export coverage', '[definition, definition.exportAttribute]', '[definition]', `assert.ok(covers(d,d.file,a.ident,a.line,a.column),'numeric definition export coverage must retain the verified attribute');`],
    ['numeric definition export position', 'Number(column) === marker.column', 'true', `assert.equal(covers(d,d.file,a.ident,a.line,a.column+1),false,'numeric definition export position must refuse a nearby attribute');`],
  ]) {
    const run = mutant => spawnSync(process.execPath, ['--input-type=module','--eval', `import assert from 'node:assert/strict';import fs from 'node:fs';import {registerHooks} from 'node:module';${mutant ? `registerHooks({load(url,context,next){const loaded=next(url,context);if(url!==${JSON.stringify(module)})return loaded;const source=String(loaded.source),anchor=${JSON.stringify(anchor)};assert.equal(source.split(anchor).length,2);return {...loaded,source:source.replace(anchor,${JSON.stringify(replacement)})};}});` : ''}const {numberOutputDefinitionCovers:covers}=await import(${JSON.stringify(module)});const d=JSON.parse(fs.readFileSync(${JSON.stringify(context)}));const a=d.exportAttribute;${assertion}`],{encoding:'utf8'});
    let result=run(false);assert.equal(result.status,0,result.stderr);
    result=run(true);assert.equal(result.status,1,result.stderr);
    assert.match(result.stderr,new RegExp(label+' must'));
    console.log(`${label}: named assertion; process exit 1`);
    result=run(false);assert.equal(result.status,0,result.stderr);
  }
  for(const marker of [definition,definition.exportAttribute]) {
    assert.equal(numberOutputDefinitionCovers(definition,definition.file,marker.ident,marker.line,marker.column),true);
    assert.equal(numberOutputDefinitionCovers(definition,definition.file,marker.ident,marker.line+1,marker.column),false);
    assert.equal(numberOutputDefinitionCovers(definition,definition.file,marker.ident,marker.line,marker.column+1),false);
    assert.equal(numberOutputDefinitionCovers(definition,definition.file,'UNSUPPORTED_ATTRIBUTE_EXPANSION:unknown',marker.line,marker.column),false);
    assert.equal(numberOutputDefinitionCovers(null,definition.file,marker.ident,marker.line,marker.column),false);
  }
  const foreign=path.join(root,'elsewhere.rs');fs.writeFileSync(foreign,'');
  assert.equal(numberOutputDefinitionCovers(definition,foreign,definition.exportAttribute.ident,definition.exportAttribute.line,definition.exportAttribute.column),false);
}));
