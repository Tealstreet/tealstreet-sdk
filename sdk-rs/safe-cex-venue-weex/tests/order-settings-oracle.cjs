const assert = require('node:assert/strict');
const fs = require('node:fs'), path = require('node:path'), vm = require('node:vm');
const {root, ts, load} = require('./types-oracle.cjs');
const enums = load(path.join(root, 'packages/safe-cex/src/types.ts'));
const file = path.join(root, 'packages/safe-cex/src/exchanges/weex/weex.exchange.ts');
const tree = ts.createSourceFile(file, fs.readFileSync(file, 'utf8'), ts.ScriptTarget.Latest, true);
const cls = tree.statements.find(n => ts.isClassDeclaration(n) && n.name?.text === 'WeexExchange');
const names = ['parseOrderResult', 'getSymbolConfigFromCache', 'getOrFetchSymbolConfig'];
const fields = cls.members.filter(n => ts.isPropertyDeclaration(n) && names.includes(n.name.getText(tree)));
assert.equal(fields.length, 3, 'order_settings::source_fields');
const utilFile = path.join(root, 'packages/safe-cex/src/exchanges/weex/weex.utils.ts');
const utilTree = ts.createSourceFile(utilFile, fs.readFileSync(utilFile, 'utf8'), ts.ScriptTarget.Latest, true);
const integerString = utilTree.statements.find(n => ts.isVariableStatement(n) && n.declarationList.declarations.some(d => d.name.getText(utilTree) === 'toIntegerString'));
assert.ok(integerString, 'order_settings::source_integer_string');
const units = s => Array.from({length: s.length}, (_, i) => s.charCodeAt(i));
const text = u => String.fromCharCode(...u);
const u = {kind: 'undefined'}, n = {kind: 'null'}, str = s => ({kind: 'string', units: units(s)});
const num = value => ({kind: 'number', value}), bool = value => ({kind: 'boolean', value});
function decode(v) {
  if (v.kind === 'undefined') return undefined;
  if (v.kind === 'null') return null;
  if (v.kind === 'string') return text(v.units);
  if (v.kind === 'object') return {toString() {if (v.throws) throw new SyntaxError('coercion'); return text(v.units);}};
  return v.value;
}
function encode(v) {
  if (v === undefined) return u;
  if (v === null) return n;
  if (typeof v === 'string') return str(v);
  if (typeof v === 'object') return {kind: 'default', marginType: v.marginType, positionMode: v.positionMode,
    crossLeverage: v.crossLeverage, isolatedLongLeverage: v.isolatedLongLeverage, isolatedShortLeverage: v.isolatedShortLeverage};
  return {kind: typeof v, value: v};
}
function slice(context) {
  const compiled = ts.transpileModule(integerString.getText(utilTree) + '\nexport class Slice {' + fields.map(n => n.getText(tree)).join('\n') + '}',
    {compilerOptions: {module: ts.ModuleKind.CommonJS, target: ts.ScriptTarget.ES2022}, reportDiagnostics: true});
  assert.equal(compiled.diagnostics.filter(d => d.category === ts.DiagnosticCategory.Error).length, 0);
  const scope = {exports: {}, ...enums, ...context}; vm.runInNewContext(compiled.outputText, scope); return new scope.exports.Slice();
}
const parse = [];
function parseCase(id, patch = {}) {
  const input = {result: 'record', props: {}, fallback: u, uuid: str('GENERATED'), fail: '', ...patch};
  const events = [], emitted = [];
  const instance = slice({uuid() {events.push('uuid'); if (input.fail === 'uuid') throw new RangeError('uuid'); return decode(input.uuid);}});
  instance.emitAndLogError = message => {events.push('emit'); emitted.push(encode(message)); if (input.fail === 'emit') throw new RangeError('emit');};
  const result = input.result === 'record' ? {} : input.result === 'null' ? null : undefined;
  if (result) for (const key of ['success', 'errorMessage', 'errorCode', 'orderId', 'clientOrderId']) Object.defineProperty(result, key, {get() {
    events.push('get:' + key); if (input.fail === key) throw new TypeError(key); return decode(input.props[key] || u);
  }});
  let output, error;
  try {output = units(instance.parseOrderResult(result, decode(input.fallback)));}
  catch (err) {error = {name: err.name, units: units(err.message)};}
  parse.push({id, input, output, error, emitted, events});
}
parseCase('order_id', {props: {orderId: str('ORDER'), clientOrderId: str('CLIENT')}});
parseCase('client_id', {props: {orderId: num(0), clientOrderId: str('CLIENT')}});
parseCase('fallback', {fallback: str('FALLBACK')});
parseCase('uuid');
parseCase('null_result', {result: 'null'});
parseCase('undefined_result', {result: 'undefined', fallback: str('FALLBACK')});
parseCase('numeric_id', {props: {orderId: num(42)}});
parseCase('surrogate_id', {props: {orderId: {kind: 'string', units: [65, 55296, 66]}}});
parseCase('false_chain', {props: {orderId: bool(false), clientOrderId: n}, fallback: str('')});
parseCase('success_zero', {props: {success: num(0), orderId: str('OK')}});
parseCase('success_null', {props: {success: n, orderId: str('OK')}});
parseCase('reject_message', {props: {success: bool(false), errorMessage: str('DENIED'), errorCode: str('CODE')}});
parseCase('reject_code', {props: {success: bool(false), errorMessage: str(''), errorCode: num(17)}});
parseCase('reject_default', {props: {success: bool(false), errorMessage: num(0), errorCode: n}});
parseCase('emit_failure', {props: {success: bool(false), errorMessage: str('DENIED')}, fail: 'emit'});
parseCase('uuid_failure', {fail: 'uuid'});
parseCase('getter_failure', {fail: 'orderId'});
parseCase('object_id', {props: {orderId: {kind: 'object', units: units('OBJECT')}}});
parseCase('coercion_failure', {props: {orderId: {kind: 'object', units: [], throws: true}}});
parseCase('rejected_surrogate', {props: {success: bool(false), errorMessage: {kind: 'string', units: [55300]}}});
const settings = [];
async function settingsCase(id, patch = {}) {
  const input = {method: 'fetch', symbol: units('BTCUSDT'), cached: u, fetched: u, refetch: false, mode: 'oneway', afterMode: 'hedged', fail: '', ...patch};
  const events = [], symbols = [];
  const instance = slice({});
  instance.accountPositionMode = input.mode;
  instance.symbolSettings = {get(symbol) {events.push('cached'); symbols.push(units(symbol)); if (input.fail === 'cached') throw new TypeError('cached'); return decode(input.cached);}};
  instance.fetchSymbolConfigs = async symbol => {
    events.push('fetch:start'); symbols.push(units(symbol)); await Promise.resolve(); events.push('fetch:end');
    if (input.fail === 'fetch') throw new RangeError('fetch'); instance.accountPositionMode = input.afterMode;
    return {get(symbol) {events.push('fetched'); symbols.push(units(symbol)); if (input.fail === 'fetched') throw new TypeError('fetched'); return decode(input.fetched);}};
  };
  let output, error;
  try {output = encode(input.method === 'cache' ? instance.getSymbolConfigFromCache(text(input.symbol)) : await instance.getOrFetchSymbolConfig(text(input.symbol), input.refetch));}
  catch (err) {error = {name: err.name, units: units(err.message)};}
  settings.push({id, input, output, error, symbols, events});
}
(async () => {
  await settingsCase('cache_hit', {method: 'cache', cached: str('CACHED')});
  await settingsCase('cache_missing', {method: 'cache'});
  await settingsCase('cache_false', {method: 'cache', cached: bool(false)});
  await settingsCase('cached_return', {cached: str('CACHED')});
  await settingsCase('refetch', {cached: str('CACHED'), refetch: true, fetched: str('FETCHED')});
  await settingsCase('fetched_return', {fetched: str('FETCHED')});
  await settingsCase('default_after_await');
  await settingsCase('falsy_default', {cached: num(0), fetched: bool(false)});
  await settingsCase('surrogate_symbol', {symbol: [55296, 66], fetched: str('FETCHED')});
  await settingsCase('cache_failure', {fail: 'cached'});
  await settingsCase('fetch_failure', {fail: 'fetch'});
  await settingsCase('fetched_failure', {fail: 'fetched'});
  assert.equal(parse.length, 20); assert.equal(settings.length, 12);
  process.stdout.write(JSON.stringify({parse, settings}));
})().catch(err => {console.error(err); process.exitCode = 1;});
