import assert from 'node:assert/strict';
import { createHash } from 'node:crypto';
import fs from 'node:fs';
import path from 'node:path';
import { spawnSync } from 'node:child_process';
import { fileURLToPath } from 'node:url';
import test from 'node:test';

const root = fileURLToPath(new URL('../..', import.meta.url));
const moduleUrl = new URL('./accepted-inherited-constant-output.mjs', import.meta.url).href;
const digest = bytes => createHash('sha256').update(bytes).digest('hex');
const metadata = JSON.parse(fs.readFileSync(new URL('../fixtures/accepted-binance-inherited-methods.json', import.meta.url)));
const baseFile = '../packages/safe-cex/src/exchanges/base.ts';
const read = file => fs.readFileSync(path.resolve(root, file));
const pin = file => ({ file, sha256: digest(read(file)) });
const sourceNames = { fetch_symbol_max_leverage: 'fetchSymbolMaxLeverage', fetch_public_trades: 'fetchPublicTrades', set_leverage: 'setLeverage', change_position_mode: 'changePositionMode', change_margin_type: 'changeMarginType' };
const venueFiles = { BinanceExchange: 'binance', BinancePMExchange: 'binancepm', BinanceSpotExchange: 'binancespot' };

// Controlled policy records only: this test installs no permission in the SDK
// shards. The separate native target executes all eight actual SDK methods.
function fixture(meta) {
  meta = structuredClone(meta);
  const method = sourceNames[meta.api.name];
  const lines = read(baseFile).toString().split('\n');
  const begin = lines.findIndex(line => line.trimStart().startsWith(`${method} = async`)) + 1;
  assert.ok(begin > 0);
  const length = lines.slice(begin - 1).findIndex(line => line.trim() === '};') + 1;
  assert.ok(length > 0);
  const end = begin + length - 1;
  const spec = {
    owner: meta.name, ident: meta.ident, package: 'safe-cex-venue-binance', api: meta.api,
    source: { ...pin(meta.source.filename), begin: [...meta.source.begin], end: [...meta.source.end] },
    typescript: {
      base: { ...pin(baseFile), method, begin: [begin, 1], end: [end, 1], excerpt: lines.slice(begin - 1, end).join('\n') },
      venue: pin(`../packages/safe-cex/src/exchanges/binance/${venueFiles[meta.ident]}.exchange.ts`),
    },
    fixture: pin('safe-cex-venue-binance/tests/support/inverse_rest_runtime.rs'),
    additionalNativeFixture: pin('safe-cex-venue-binance/tests/inherited_refusal_binding.rs'),
    sourceOracle: pin('safe-cex-venue-binance/scripts/inherited-refusal-source.cjs'),
    call: 'controlled adapter used only to test policy provenance',
    cases: ['BTCUSDT', 'ETHUSDT'].map(symbol => ({ input: { symbol }, output: { error: 'Not implemented', error_name: 'Error', http: 0, events: 0, reports: 0 } })),
  };
  return {
    spec,
    type: { name: meta.name, ident: meta.ident, kind: 'record', deserialize: false, rawEntrypoints: [true], entrypointSources: [meta.source] },
    entry: { api: meta.api, constantOutput: true, call: spec.call, cases: spec.cases },
  };
}

function run(item, { corruptFile = null, refusal = null, remove = null, label = 'Binance provenance' } = {}) {
  const key = `${item.spec.owner}::${item.spec.api.trait}::${item.spec.api.name}`;
  const program = `
import assert from 'node:assert/strict';import fs from 'node:fs';import path from 'node:path';import {registerHooks} from 'node:module';
const item=${JSON.stringify(item)};let changed=0;
registerHooks({load(url,ctx,next){const result=next(url,ctx);if(url!==${JSON.stringify(moduleUrl)})return result;let source=String(result.source);const needle="const reviewed = loadPolicy('inherited-constant-outputs');";assert.equal(source.split(needle).length,2);source=source.replace(needle,${JSON.stringify(`const reviewed = ${JSON.stringify({ [key]: item.spec })};`)});const remove=${JSON.stringify(remove)};if(remove){assert.equal(source.split(remove).length,2);source=source.replace(remove,'');changed++;}return {...result,source};}});
const {reviewedInheritedConstantOutput}=await import(${JSON.stringify(moduleUrl)});
const context={packageMetadata:{name:'safe-cex-venue-binance',manifest_path:path.join(${JSON.stringify(root)},'safe-cex-venue-binance/Cargo.toml')},readSource:file=>{const bytes=fs.readFileSync(path.resolve(${JSON.stringify(root)},file));return file===${JSON.stringify(corruptFile)}?Buffer.concat([bytes,Buffer.from([10])]):bytes;}};
${refusal ? `assert.throws(()=>reviewedInheritedConstantOutput(item.type,item.entry,0,context),new RegExp(${JSON.stringify(refusal)}),${JSON.stringify(label + ' must refuse')});` : 'assert.equal(reviewedInheritedConstantOutput(item.type,item.entry,0,context),true);'}
assert.equal(changed,${remove ? 1 : 0});`;
  const result = spawnSync(process.execPath, ['--input-type=module', '--eval', program], { encoding: 'utf8' });
  assert.ifError(result.error);
  return result;
}

test('Binance supports only eight exact inherited methods with all provenance guards', () => {
  assert.equal(metadata.length, 8);
  for (const meta of metadata) {
    const item = fixture(meta);
    let result = run(item); assert.equal(result.status, 0, result.stderr);
    for (const [mutate, refusal] of [
      [x => { x.entry.api = { ...x.entry.api, signature: { output: null } }; }, 'compiler API changed'],
      [x => { x.type.entrypointSources[0].begin[0]++; }, 'compiler method source changed'],
      [x => { x.type.rawEntrypoints[0] = false; }, 'actual indexed raw method required'],
      [x => { x.entry.call += ' '; }, 'native adapter changed'],
      [x => { x.entry.cases = structuredClone(x.entry.cases); x.entry.cases[0].output.http++; }, 'native witnesses changed'],
      [x => { x.spec.typescript.base.method = 'fetchPositions'; }, 'exact base method required'],
      [x => { x.spec.typescript.venue.file = '../packages/safe-cex/src/exchanges/binance/binanceinverse.exchange.ts'; }, 'actual maintained venue required'],
      [x => { x.spec.source.file = 'safe-cex-venue-binance/src/exchange_classes.rs'; }, 'actual trait implementation source required'],
    ]) {
      const changed = structuredClone(item); mutate(changed);
      result = run(changed, { refusal }); assert.equal(result.status, 0, result.stderr);
    }
    for (const [file, refusal] of [
      [item.spec.source.file, 'reviewed Rust source changed'],
      [baseFile, 'maintained base source changed'],
      [item.spec.typescript.venue.file, 'venue inheritance changed'],
      [item.spec.fixture.file, 'native fixture changed'],
      [item.spec.additionalNativeFixture.file, 'native observer changed'],
      [item.spec.sourceOracle.file, 'source oracle changed'],
    ]) {
      result = run(item, { corruptFile: file, refusal }); assert.equal(result.status, 0, result.stderr);
    }
  }
});

test('Binance inherited permission rejects implemented methods and the inverse class', () => {
  for (const [ident, method] of [
    ['BinanceExchange', 'fetch_public_trades'],
    ['BinancePMExchange', 'change_margin_type'],
    ['BinanceSpotExchange', 'fetch_positions'],
    ['BinanceInverseExchange', 'fetch_symbol_max_leverage'],
  ]) {
    const item = fixture(metadata[0]);
    item.type.name = item.spec.owner = `safe_cex_venue_binance::exchange_classes::${ident}`;
    item.type.ident = item.spec.ident = ident;
    item.entry.api.name = item.spec.api.name = method;
    const result = run(item, { refusal: 'unreviewed exact method' });
    assert.equal(result.status, 0, result.stderr);
  }
});

test('Binance exact-method and executable fixture checks reject removed-guard controls', () => {
  const item = fixture(metadata[0]);
  const wrongMethod = structuredClone(item);
  wrongMethod.spec.api.name = wrongMethod.entry.api.name = 'fetch_positions';
  for (const [changed, options] of [
    [wrongMethod, { label: 'Binance exact-method scope', refusal: 'unreviewed exact method', remove: "  if (spec.package === 'safe-cex-venue-binance') reviewedBinanceInheritance(type, entry, spec);" }],
    [item, { label: 'Binance native observer pin', refusal: 'native observer changed', corruptFile: item.spec.additionalNativeFixture.file, remove: "  assert.equal(digest(context.readSource(spec.additionalNativeFixture.file)), spec.additionalNativeFixture.sha256, `${label}: reviewed Binance native observer changed`);" }],
    [item, { label: 'Binance source oracle pin', refusal: 'source oracle changed', corruptFile: item.spec.sourceOracle.file, remove: "  assert.equal(digest(context.readSource(spec.sourceOracle.file)), spec.sourceOracle.sha256, `${label}: reviewed Binance source oracle changed`);" }],
  ]) {
    let result = run(changed, { ...options, remove: null }); assert.equal(result.status, 0, result.stderr);
    result = run(changed, options); assert.equal(result.status, 1, result.stderr);
    assert.match(result.stderr, new RegExp(options.label + ' must refuse'));
    result = run(changed, { ...options, remove: null }); assert.equal(result.status, 0, result.stderr);
    console.log(`${options.label}: named assertion; process exit 1`);
  }
});
