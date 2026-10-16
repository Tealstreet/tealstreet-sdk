const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const vm = require('node:vm');
const {root, ts, load} = require('./types-oracle.cjs');

const profile = process.argv[2];
const control = process.argv.includes('--control');
const file = path.join(root, 'packages/safe-cex/src/exchanges/weex/weex.utils.ts');
const ast = ts.createSourceFile(file, fs.readFileSync(file, 'utf8'), ts.ScriptTarget.Latest, true);
const names = ['toNumber', 'toIntegerString', 'normalizeTimestampToMs', 'normalizeTimestampToSeconds', 'normalizeOrderSide', 'normalizeExecution', 'normalizeHistoricalTrade'];
const nodes = ast.statements.filter(node => ts.isVariableStatement(node) && node.declarationList.declarations.some(declaration => names.includes(declaration.name.getText(ast))));
assert.equal(nodes.length, 7, 'WEEX SOURCE declaration floor');
let selected = nodes.map(node => node.getText(ast)).join('\n');
function replace(before, after) {
  assert.equal(selected.split(before).length - 1, 1, 'WEEX SOURCE unique control selector');
  selected = selected.replace(before, after);
}
if (control) {
  if (profile === 'commission' || profile === 'historical') {
    replace('maker: raw.maker,', "maker: (() => { throw new TypeError('early maker admission'); })(),");
  } else if (profile === 'clock') {
    replace('buyer: raw.buyer,', "buyer: raw.buyer === null ? (() => { throw new TypeError('early buyer admission'); })() : raw.buyer,");
  } else if (profile === 'guards') {
    replace('const price = toNumber(raw.price);', 'if (!symbol) return null;\n  const price = toNumber(raw.price);');
  } else if (profile === 'bounds') {
    replace("const symbol = String(raw.symbol ?? market?.symbol ?? '');", "if (raw.ignored !== undefined) throw new TypeError('eager ignored-field refusal');\n  const symbol = String(raw.symbol ?? market?.symbol ?? '');");
  } else {
    throw new Error('Unknown WEEX source control profile');
  }
}
const compiled = ts.transpileModule(selected, {compilerOptions: {module: ts.ModuleKind.CommonJS, target: ts.ScriptTarget.ES2022}, reportDiagnostics: true});
assert.equal(compiled.diagnostics.filter(diagnostic => diagnostic.category === ts.DiagnosticCategory.Error).length, 0);
const helper = {exports: {}, require};
const helperFile = path.join(root, 'packages/safe-cex/src/utils/omitUndefined.ts');
vm.runInNewContext(ts.transpileModule(fs.readFileSync(helperFile, 'utf8'), {compilerOptions: {module: ts.ModuleKind.CommonJS, target: ts.ScriptTarget.ES2022, esModuleInterop: true}}).outputText, helper, {filename: helperFile});
process.env.TZ = 'UTC';
const dateText = '2024-01-02T03:04:05';
const baseMs = 1704164645000;
const record = () => ({symbol: 'B/Q', side: 'BUY', price: 3, qty: 5, time: 1700000000123, tradeId: 'fill-a', orderId: 'order-b'});
const observations = [];
function run(name, raw, secondTime) {
  const calls = [];
  const context = {
    exports: {}, ...load(path.join(root, 'packages/safe-cex/src/types.ts')),
    ...load(path.join(root, 'packages/safe-cex/src/exchanges/weex/weex.types.ts')),
    ...helper.exports,
    Date: {parse(text) {
      calls.push(text);
      const parsed = Date.parse(text);
      return secondTime === undefined ? parsed : calls.length === 1 ? parsed + 123 : secondTime;
    }},
  };
  vm.runInNewContext(compiled.outputText, context, {filename: file});
  try { return {result: context.exports[name](raw), calls}; }
  catch (error) { return {error: {name: error.name, message: error.message}, calls}; }
}
function conversion(outcome, label) {
  assert.equal(outcome.error?.name, 'TypeError', label);
  assert.equal(outcome.error.message, 'Cannot convert object to primitive value', label);
}
if (profile === 'commission') {
  for (const name of ['normalizeExecution', 'normalizeHistoricalTrade']) {
    const outcome = run(name, {...record(), maker: null, commissionAsset: 7, commission: {toString: null}});
    conversion(outcome, 'WEEX SOURCE commission before maker');
    observations.push({id: name, ...outcome});
  }
  assert.equal(observations.length, 2, 'WEEX SOURCE commission floor');
} else if (profile === 'historical') {
  for (const [id, orderId] of [['missing', undefined], ['zero', 0], ['array', []]]) {
    const raw = {...record(), orderId, maker: null, commissionAsset: 7, buyer: null, quoteQty: {toString: null}};
    const guarded = run('normalizeHistoricalTrade', raw);
    assert.equal(guarded.result, null, 'WEEX SOURCE historical order guard');
    observations.push({id, ...guarded});
    const throwing = run('normalizeHistoricalTrade', {...raw, commission: {toString: null}});
    conversion(throwing, 'WEEX SOURCE conversion before historical order guard');
    observations.push({id: `${id}-commission`, ...throwing});
  }
  const quote = run('normalizeHistoricalTrade', {...record(), maker: null, quoteQty: {toString: null}});
  conversion(quote, 'WEEX SOURCE quote before model admission');
  observations.push({id: 'quote', ...quote});
  assert.equal(observations.length, 7, 'WEEX SOURCE historical floor');
} else if (profile === 'clock') {
  for (const [field, invalid] of [['buyer', null], ['maker', null], ['commissionAsset', 7]]) {
    const outcome = run('normalizeHistoricalTrade', {...record(), time: dateText, [field]: invalid});
    assert.deepEqual(outcome.calls, [dateText, dateText], 'WEEX SOURCE historical second timestamp');
    assert.equal(outcome.result[field], invalid, 'WEEX SOURCE verbatim model field');
    assert.equal(outcome.result.time, baseMs);
    observations.push({id: field, ...outcome});
  }
  for (const [id, time] of [['distinct', baseMs + 987], ['zero', 0], ['nan', NaN]]) {
    const outcome = run('normalizeHistoricalTrade', {...record(), time: dateText}, time);
    assert.deepEqual(outcome.calls, [dateText, dateText], 'WEEX SOURCE historical second timestamp');
    assert.equal(outcome.result.time, Number.isNaN(time) ? 0 : time, 'WEEX SOURCE independent milliseconds');
    observations.push({id, ...outcome});
  }
  assert.equal(observations.length, 6, 'WEEX SOURCE clock floor');
} else if (profile === 'guards') {
  for (const name of ['normalizeExecution', 'normalizeHistoricalTrade']) {
    const cold = run(name, {...record(), symbol: '', time: dateText, maker: null, commission: {toString: null}});
    assert.equal(cold.result, null, 'WEEX SOURCE cold guard');
    assert.deepEqual(cold.calls, [dateText], 'WEEX SOURCE cold guard scalar work');
    observations.push({id: `${name}-cold`, ...cold});
    const eager = run(name, {...record(), symbol: '', price: {toString: null}, time: dateText});
    conversion(eager, 'WEEX SOURCE eager scalar conversion');
    assert.deepEqual(eager.calls, []);
    observations.push({id: `${name}-eager`, ...eager});
  }
  assert.equal(observations.length, 4, 'WEEX SOURCE guard floor');
} else if (profile === 'bounds') {
  const deep = Array.from({length: 150}).reduce(value => [value], null);
  const unit = JSON.parse('"\\ud800"');
  assert.equal(unit.charCodeAt(0), 0xd800, 'WEEX SOURCE materialized ignored unit');
  for (const name of ['normalizeExecution', 'normalizeHistoricalTrade']) {
    for (const [id, raw] of [['depth', {...record(), ignored: deep}], ['unit', {...record(), ignored: unit}], ['cold-unit', {...record(), symbol: '', ignored: unit}]]) {
      const outcome = run(name, raw);
      if (id === 'cold-unit') {
        assert.equal(outcome.result, null, 'WEEX SOURCE ignored input admitted');
      } else {
        assert.equal(outcome.result?.symbol, 'B/Q', 'WEEX SOURCE ignored input admitted');
        assert.equal(outcome.result.orderId, 'order-b');
      }
      observations.push({id: `${name}-${id}`, ...outcome});
    }
  }
  assert.equal(observations.length, 6, 'WEEX SOURCE bounds floor');
} else {
  throw new Error('Select a WEEX source profile');
}
process.stdout.write(JSON.stringify({profile, observations}));
