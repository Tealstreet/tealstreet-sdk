const assert = require('node:assert/strict');
const fs = require('node:fs'), path = require('node:path'), vm = require('node:vm');
const {root, ts, load, header} = require('./types-oracle.cjs');
const profile = process.argv[2], control = process.argv.includes('--control');
assert.ok(['market-order', 'market-unit', 'fake-input', 'ticker-input'].includes(profile), 'WEEX selected boundary profile');
const file = path.join(root, 'packages/safe-cex/src/exchanges/weex/weex.utils.ts');
const ast = ts.createSourceFile(file, fs.readFileSync(file, 'utf8'), ts.ScriptTarget.Latest, true);
const names = profile === 'fake-input' ? ['fakePositionsForMarket'] : profile === 'ticker-input' ? ['toNumber', 'normalizePercentage', 'normalizeTicker'] : ['toNumber', 'normalizeMarket'];
let text = names.map(name => {
  const nodes = ast.statements.filter(n => ts.isVariableStatement(n) && n.declarationList.declarations.some(d => d.name.getText(ast) === name));
  assert.equal(nodes.length, 1, 'WEEX actual boundary declaration ' + name);
  return nodes[0].getText(ast);
}).join('\n');
function replaceOnce(before, after) {
  assert.equal(text.split(before).length - 1, 1, 'WEEX unique boundary source control');
  text = text.replace(before, after);
}
if (control && profile === 'market-order') replaceOnce('id: raw.symbol,', "id: (() => {if (typeof raw.symbol !== 'string') throw new TypeError('early Market.symbol admission'); return raw.symbol;})(),");
if (control && profile === 'market-unit') replaceOnce('info: raw,', "info: {...raw, extra:'replacement'},");
if (control && profile === 'fake-input') replaceOnce('return [createFake(PositionSide.Long), createFake(PositionSide.Short)];', 'return [];');
if (control && profile === 'ticker-input') replaceOnce('id: market.id,', "id: 'replacement',");
const bindings = {
  ...load(path.join(root, 'packages/ccxt-compat/src/functions/ccxt_exchange.ts')),
  ...load(path.join(root, 'packages/safe-cex/src/types.ts')),
  ...load(path.join(root, 'packages/safe-cex/src/utils/createPositionId.ts')),
};
const context = {exports: {}, ...bindings};
const compiled = ts.transpileModule(text, {compilerOptions: {module: ts.ModuleKind.CommonJS, target: ts.ScriptTarget.ES2022}, reportDiagnostics: true});
assert.equal(compiled.diagnostics.filter(d => d.category === ts.DiagnosticCategory.Error).length, 0);
vm.runInNewContext(compiled.outputText, context, {filename: file});
const base = {id:'venue-id', symbol:'B/Q', baseAsset:'B', quoteAsset:'Q', base:'B', quote:'Q', active:true,
  precision:{amount:1,price:1}, limits:{amount:{min:0,max:10},leverage:{min:1,max:20}}};
const high = JSON.parse('"\\ud800"');
const units = value => Array.from({length:value.length}, (_, i) => value.charCodeAt(i));
const observation = {id:profile};
if (profile === 'market-order') {
  const cases = [];
  for (const field of ['symbol','baseAsset','quoteAsset']) {
    for (const numeric of ['maxOrderSize','minLeverage','maxLeverage','makerFeeRate','takerFeeRate']) {
      let error;
      try {context.exports.normalizeMarket({...base, [field]:null, [numeric]:{toString:null}});} catch (e) {error=e;}
      assert.equal(error?.name, 'TypeError', 'WEEX SOURCE market coercion class');
      assert.equal(error?.message, 'Cannot convert object to primitive value', 'WEEX SOURCE market conversion before string admission');
      cases.push({id:field+':'+numeric, name:error.name, message:error.message});
    }
  }
  assert.equal(cases.length, 15, 'WEEX SOURCE market error-order floor');
  observation.cases = cases;
} else if (profile === 'market-unit') {
  const value = context.exports.normalizeMarket({...base, extra:high});
  assert.deepEqual(units(value.info.extra), [0xd800], 'WEEX SOURCE market returned info unit');
  observation.units = units(value.info.extra);
} else {
  let deep = 1;
  for (let i=0;i<150;i++) deep=[deep];
  const cases = [];
  for (const [id, input] of [['unused-base-unit', {...base, base:high}], ['unused-info-depth', {...base, info:{ignored:deep}}]]) {
    if (profile === 'fake-input') {
      const positions = context.exports.fakePositionsForMarket(input, bindings.PositionMode.Hedged, bindings.MarginType.Cross, 3);
      assert.equal(positions.length, 2, 'WEEX SOURCE unused Market input positions');
      assert.equal(positions[0].symbol, 'B/Q', 'WEEX SOURCE unused Market input symbol');
      assert.equal(positions[0].id, 'B/Q:long', 'WEEX SOURCE unused Market input long id');
      assert.equal(positions[1].id, 'B/Q:short', 'WEEX SOURCE unused Market input short id');
      cases.push({id, count:positions.length});
    } else {
      const ticker = context.exports.normalizeTicker({lastPrice:3}, input);
      assert.equal(ticker.id, 'venue-id', 'WEEX SOURCE unused Market ticker id');
      assert.equal(ticker.symbol, 'B/Q', 'WEEX SOURCE unused Market ticker symbol');
      assert.equal(ticker.last, 3, 'WEEX SOURCE unused Market ticker last');
      cases.push({id, last:ticker.last});
    }
  }
  assert.equal(cases.length, 2, 'WEEX SOURCE unused Market input floor');
  observation.cases = cases;
}
process.stdout.write(JSON.stringify({...header, observations:[observation]})+'\n');
