import { createHash } from 'node:crypto';
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

// Controlled policy bytes exercise provenance guards without installing any
// Kucoin SDK permission. Complete production owners are validated separately.
function kucoinPolicyFixture(method, inherited = false, spot = true) {
  const owner = `safe_cex_venue_kucoin::kucoin_exchange::${spot ? 'KucoinSpotExchange' : 'KucoinExchange'}`;
  const api = { kind: 'trait', trait: 'safe_cex_exchange::exchange::Exchange', name: method };
  const rust = Buffer.from('controlled Rust method source for policy guards');
  const io = Buffer.from('controlled native I/O fixture for policy guards');
  const observer = Buffer.from('controlled native observer fixture for policy guards');
  const fixture = 'safe-cex-venue-kucoin/tests/support/exchange_runtime.rs';
  const helper = `safe-cex-parity/accepted-set/kucoin-${inherited ? 'refusal' : 'constant'}-fixture.rs`;
  const source = 'safe-cex-venue-kucoin/src/kucoin_exchange.rs';
  const venue = `../packages/safe-cex/src/exchanges/kucoin/${spot ? 'kucoinspot' : 'kucoin'}.exchange.ts`;
  const base = '../packages/safe-cex/src/exchanges/base.ts';
  const bytes = { [source]: rust.toString('base64'), [fixture]: io.toString('base64'), [helper]: observer.toString('base64'), [venue]: fs.readFileSync(path.resolve(root, venue)).toString('base64'), [base]: fs.readFileSync(path.resolve(root, base)).toString('base64') };
  const digest = value => createHash('sha256').update(value).digest('hex');
  const tsFile = inherited ? base : venue;
  const methodName = method === 'fetch_public_trades' ? 'fetchPublicTrades' : method === 'fetch_symbol_max_leverage' ? 'fetchSymbolMaxLeverage' : method === 'change_margin_type' ? 'changeMarginType' : 'changePositionMode';
  const lines = Buffer.from(bytes[tsFile], 'base64').toString().split('\n');
  const begin = lines.findIndex(line => line.trimStart().startsWith(`${methodName} = async`)) + 1;
  assert.ok(begin > 0);
  const end = begin + (inherited ? 3 : method === 'fetch_symbol_max_leverage' ? 2 : 0);
  const ts = { file: tsFile, method: methodName, begin: [begin, 1], end: [end, 1], excerpt: lines.slice(begin - 1, end).join('\n'), sha256: digest(Buffer.from(bytes[tsFile], 'base64')) };
  const state = { http: 0, state_unchanged: true, options_unchanged: true, notifications: 0, events: 0, reports_unchanged: true, sockets_unchanged: true, now_unchanged: true };
  const output = inherited ? { ...state, error: 'Not implemented', error_name: 'Error' } : { ...state, native: method === 'fetch_symbol_max_leverage' ? { kind: 'number', bits: '3ff0000000000000' } : { kind: 'unit' } };
  const spec = { owner, ident: spot ? 'KucoinSpotExchange' : 'KucoinExchange', package: 'safe-cex-venue-kucoin', api,
    source: { file: source, begin: [1, 1], end: [1, 2], sha256: digest(rust) },
    fixture: { file: fixture, sha256: digest(io) }, additionalNativeFixture: { file: helper, sha256: digest(observer) },
    typescript: inherited ? { base: ts, venue: { file: venue, sha256: digest(Buffer.from(bytes[venue], 'base64')) } } : { override: ts },
    call: 'controlled native adapter for policy guard tests', cases: [{ input: { symbol: 'left' }, output }, { input: { symbol: 'right' }, output }],
    ...(!inherited ? { category: 'native-method' } : {}),
  };
  const type = { name: owner, ident: spec.ident, kind: 'record', deserialize: false, rawEntrypoints: [true], entrypointSources: [{ filename: source, begin: [1, 1], end: [1, 2] }] };
  const entry = { api, constantOutput: true, input: 'observed', call: spec.call, cases: spec.cases };
  return { spec, type, entry, bytes, inherited, packageMetadata: { name: spec.package, manifest_path: path.join(root, spec.package, 'Cargo.toml') } };
}
function kucoinPolicyChild(item, { corruptFile = null, anchor = null, module = null, refusal = null, label = 'Kucoin policy guard' } = {}) {
  const tableModule = new URL(item.inherited ? './accepted-inherited-constant-output.mjs' : './accepted-constant-output.mjs', import.meta.url).href;
  const table = item.inherited ? 'inherited-constant-outputs' : 'constant-outputs';
  const policyKey = `${item.spec.owner}::${item.spec.api.trait}::${item.spec.api.name}`;
  const targetModule = module ? new URL(module, import.meta.url).href : null;
  return spawnSync(process.execPath, ['--input-type=module', '--eval', `import assert from 'node:assert/strict';import {registerHooks}from'node:module';const item=${JSON.stringify(item)};let injected=0;registerHooks({load(url,ctx,next){const result=next(url,ctx);let source=String(result.source);if(url===${JSON.stringify(tableModule)}){const needle=${JSON.stringify(`const reviewed = loadPolicy('${table}');`)};assert.equal(source.split(needle).length,2);source=source.replace(needle,${JSON.stringify(`const reviewed = ${JSON.stringify({ [policyKey]: item.spec })};`)});}if(url===${JSON.stringify(targetModule)}){const anchor=${JSON.stringify(anchor)};assert.equal(source.split(anchor).length,2);source=source.replace(anchor,'');injected++;}return source===String(result.source)?result:{...result,source};}});const{reviewedConstantOutput}=await import(${JSON.stringify(new URL('./accepted-constant-output.mjs', import.meta.url).href)});const context={packageMetadata:item.packageMetadata,readSource:file=>{const bytes=Buffer.from(item.bytes[file],'base64');return file===${JSON.stringify(corruptFile)}?Buffer.concat([bytes,Buffer.from([10])]):bytes;}};${refusal ? `assert.throws(()=>reviewedConstantOutput(item.type,item.entry,0,context),new RegExp(${JSON.stringify(refusal)}),${JSON.stringify(label+' must refuse')});` : 'assert.equal(reviewedConstantOutput(item.type,item.entry,0,context),true);'}assert.equal(injected,${anchor ? 1 : 0});`], { encoding: 'utf8' });
}

test('Kucoin constant policies keep exact inherited override and observer provenance', () => {
  const inherited = [kucoinPolicyFixture('fetch_symbol_max_leverage', true, false), kucoinPolicyFixture('fetch_public_trades', true, false), kucoinPolicyFixture('fetch_public_trades', true, true)];
  const overrides = ['fetch_symbol_max_leverage', 'change_margin_type', 'change_position_mode'].map(method => kucoinPolicyFixture(method));
  for (const item of [...inherited, ...overrides]) {
    let result = kucoinPolicyChild(item); assert.equal(result.status, 0, result.stderr);
    for (const [file, refusal] of [[item.spec.fixture.file, 'native fixture changed'], [item.spec.additionalNativeFixture.file, 'native observer changed']]) {
      result = kucoinPolicyChild(item, { corruptFile: file, refusal }); assert.equal(result.status, 0, result.stderr);
    }
    const unknown = structuredClone(item); unknown.spec.owner = unknown.type.name = 'unreviewed::KucoinExchange';
    result = kucoinPolicyChild(unknown, { refusal: /unreviewed/.source }); assert.equal(result.status, 0, result.stderr);
    const context = { packageMetadata: item.packageMetadata, readSource: file => Buffer.from(item.bytes[file], 'base64') };
    // Controlled compiler metadata never inherits a production permission.
    assert.throws(() => reviewedConstantOutput(item.type, item.entry, 0, context), /unreviewed owner or method|compiler API changed/);
  }
  const wrong = structuredClone(inherited[0]); wrong.spec.api.name = wrong.entry.api.name = 'set_leverage';
  for (const [item, label, module, anchor, corruptFile, refusal] of [
    [inherited[0], 'Kucoin inherited observer fixture pin', './accepted-inherited-constant-output.mjs', "  assert.equal(digest(context.readSource(spec.additionalNativeFixture.file)), spec.additionalNativeFixture.sha256, `${label}: reviewed native observer changed`);", inherited[0].spec.additionalNativeFixture.file, 'native observer changed'],
    [overrides[0], 'Kucoin override observer fixture pin', './accepted-inherited-constant-output.mjs', "  assert.equal(digest(context.readSource(spec.additionalNativeFixture.file)), spec.additionalNativeFixture.sha256, `${label}: reviewed native observer changed`);", overrides[0].spec.additionalNativeFixture.file, 'native observer changed'],
    [overrides[0], 'Kucoin override maintained source pin', './accepted-constant-output.mjs', "    assert.equal(createHash('sha256').update(bytes).digest('hex'), source.sha256, `${label}: maintained Spot source changed`);", overrides[0].spec.typescript.override.file, 'maintained Spot source changed'],
    [wrong, 'Kucoin inherited method scope', './accepted-inherited-constant-output.mjs', "  if (spec.package === 'safe-cex-venue-kucoin') reviewedKucoinInheritance(type, entry, spec);", null, 'unreviewed Kucoin inherited refusal'],
  ]) {
    let result = kucoinPolicyChild(item, { corruptFile, refusal, label }); assert.equal(result.status, 0, result.stderr);
    result = kucoinPolicyChild(item, { corruptFile, refusal, label, module, anchor }); assert.equal(result.status, 1, result.stderr);
    assert.match(result.stderr, new RegExp(label + ' must refuse'));
    console.log(`${label}: named assertion; process exit 1`);
    result = kucoinPolicyChild(item, { corruptFile, refusal, label }); assert.equal(result.status, 0, result.stderr);
  }
});
