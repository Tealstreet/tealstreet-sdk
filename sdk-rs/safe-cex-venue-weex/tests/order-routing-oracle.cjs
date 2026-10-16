const assert = require('node:assert/strict');
const fs = require('node:fs'), path = require('node:path'), vm = require('node:vm');
const {root, ts, capture, load} = require('./types-oracle.cjs');
const enums = load(path.join(root, 'packages/safe-cex/src/types.ts'));
const file = path.join(root, 'packages/safe-cex/src/exchanges/weex/weex.exchange.ts');
const ast = ts.createSourceFile(file, fs.readFileSync(file, 'utf8'), ts.ScriptTarget.Latest, true);
const declaration = ast.statements.find(n => ts.isClassDeclaration(n) && n.name?.getText(ast) === 'WeexExchange');
const names = ['isBatchableRegularOrder', 'isAlgoOrder', 'inferPositionSide'];
const members = declaration.members.filter(n => ts.isPropertyDeclaration(n) && names.includes(n.name.getText(ast)));
assert.equal(members.length, 3, 'routing::source_inventory');
const context = {exports: {}, ...enums};
vm.runInNewContext(ts.transpileModule('export class Slice {\n' + members.map(n => n.getText(ast)).join('\n') + '\n}',
  {compilerOptions: {module: ts.ModuleKind.CommonJS, target: ts.ScriptTarget.ES2022}}).outputText, context, {filename: file});
const slice = new context.exports.Slice();
const units = value => Array.from({length: value.length}, (_, i) => value.charCodeAt(i));
function value(input) {
  const kind = input === null ? 'null' : typeof input;
  if (kind === 'number') return {kind, bits: capture(input).bits};
  if (kind === 'string') return {kind, units: units(input)};
  if (kind === 'boolean') return {kind, value: input};
  if (kind === 'bigint') return {kind, text: String(input)};
  return {kind};
}
const input = opts => Object.fromEntries(Object.entries(opts).map(([name, item]) => [name, value(item)]));
const typeNames = ['Limit', 'Market', 'Conditional', 'StopLoss', 'TakeProfit', 'TrailingStopLoss'];
const batches = [];
function batch(id, patch) {
  const opts = {symbol: 'unused', type: enums.OrderType.Limit, side: enums.OrderSide.Buy, amount: 1, ...patch};
  batches.push({id, input: input(opts), result: slice.isBatchableRegularOrder(opts)});
}
for (const name of typeNames) batch('base_' + name, {type: enums.OrderType[name]});
for (const field of ['triggerPrice', 'stopLoss', 'takeProfit']) {
  for (const [suffix, item] of [['zero', 0], ['negative_zero', -0], ['nan', NaN], ['infinity', Infinity], ['negative_infinity', -Infinity]]) {
    batch(field + '_' + suffix, {[field]: item});
  }
}
for (const field of ['tpTriggerType', 'slTriggerType']) {
  for (const name of ['LastPrice', 'MarkPrice', 'IndexPrice']) batch(field + '_' + name, {[field]: enums.OrderStopTriggerType[name]});
}
for (const field of ['reduceOnly', 'close']) {
  for (const [suffix, item] of [['undefined', undefined], ['false', false], ['true', true]]) batch(field + '_' + suffix, {[field]: item});
}
batch('price_zero', {price: 0}); batch('price_nan', {price: NaN}); batch('amount_infinity', {amount: Infinity});
batch('unrelated_trigger', {triggerType: enums.OrderStopTriggerType.MarkPrice});
batch('both_flags_false', {reduceOnly: false, close: false});
batch('all_guards', {type: enums.OrderType.StopLoss, triggerPrice: 0, stopLoss: 0, takeProfit: 0,
  reduceOnly: true, close: true, tpTriggerType: enums.OrderStopTriggerType.LastPrice, slTriggerType: enums.OrderStopTriggerType.LastPrice});
// Supplements cover scalar projections beyond the declared optional field types.
for (const field of ['triggerPrice', 'tpTriggerType']) batch(field + '_null', {[field]: null});
batch('reduceOnly_number', {reduceOnly: 1}); batch('close_string', {close: 'true'});
batch('unknown_type', {type: 'LIMIT'});
const ids = [['plain', 'plain-42'], ['double', '42__tp'], ['prefix', '__42'], ['suffix', '42__'],
  ['single', '42_tp'], ['triplet', '___'], ['empty', ''], ['lone', '\ud800__42'],
  ['lone_plain', '\ud800-42'], ['split_unicode', '_\ud800_'], ['fullwidth', '＿＿']];
const algos = [];
for (const name of [...typeNames, 'unknown']) {
  const type = name === 'unknown' ? 'LIMIT' : enums.OrderType[name];
  for (const [suffix, id] of ids) algos.push({id: name + '_' + suffix, type: value(type), units: units(id),
    result: slice.isAlgoOrder({type, id})});
}
const reduceValues = [['undefined', undefined], ['false', false], ['true', true], ['zero', 0],
  ['negative_zero', -0], ['nan', NaN], ['one', 1], ['empty', ''], ['string', 'false'],
  ['null', null], ['object', {}], ['function', () => {}], ['symbol', Symbol('reduce')], ['bigint_zero', 0n], ['bigint_one', 1n]];
const positions = [];
for (const [sideName, side] of [['Buy', enums.OrderSide.Buy], ['Sell', enums.OrderSide.Sell], ['unknown', 'BUY']]) {
  for (const [suffix, reduceOnly] of reduceValues) positions.push({id: sideName + '_' + suffix,
    side: value(side), reduceOnly: value(reduceOnly), result: slice.inferPositionSide({side, reduceOnly})});
}
process.stdout.write(JSON.stringify({batches, algos, positions}));
