// Execute unchanged complete modules and call the real class-field functions.
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const vm = require('node:vm');
assert(process.env.SAFE_CEX_TYPESCRIPT_PATH, 'compiler_path_required');
const ts = require(process.env.SAFE_CEX_TYPESCRIPT_PATH);
const sourceRoot = path.resolve(__dirname, '../../../packages/safe-cex/src');
function execute(relative, requireModule) {
  const file = path.join(sourceRoot, relative);
  const compiled = ts.transpileModule(fs.readFileSync(file, 'utf8'), {
    fileName: file, reportDiagnostics: true,
    compilerOptions: {module: ts.ModuleKind.CommonJS, target: ts.ScriptTarget.ES2022, esModuleInterop: true},
  });
  assert.equal(compiled.diagnostics.filter(d => d.category === ts.DiagnosticCategory.Error).length, 0, 'mapping_transpile');
  const context = vm.createContext({exports: {}, require: requireModule});
  vm.runInContext(compiled.outputText, context, {filename: file});
  return context.exports;
}
const logging = execute('../../logging/src/LogSeverity.ts', name => {throw Error(`unexpected_logging_import:${name}`);});
const types = execute('types.ts', name => {
  if (name === '@tealstreet/logging/src/LogSeverity') return logging;
  throw Error(`unexpected_enum_import:${name}`);
});
const venue = execute('exchanges/cryptocom/cryptocom.types.ts', name => {throw Error(`unexpected_venue_import:${name}`);});
const untouched = () => {throw Error('unclaimed_consumer_called');};
let constructors = 0;
class SocketStub {constructor() {constructors++;}}
const exchangeModule = execute('exchanges/cryptocom/cryptocom.exchange.ts', name => {
  if (name === '../../types') return types;
  if (name === './cryptocom.types') return venue;
  if (name === '../base') return {BaseExchange: class {createXhrWithSecondaryKey() {return {};}}};
  if (name === '../clientOrderId') return {patternClientOrderIdRule: () => ({})};
  if (name === './cryptocom.ws-private') return {CryptocomPrivateWebSocket: SocketStub};
  if (name === './cryptocom.ws-public') return {CryptocomPublicWebSocket: SocketStub};
  if (name === './cryptocom.api') return {createAPI: untouched};
  if (name === '../../utils/virtualClock') return {virtualClock: {getCurrentTimeMs: untouched}};
  if (name === '../../utils/createPositionId') return {createPositionId: untouched};
  if (name === 'p-iteration') return {forEachSeries: untouched};
  if (['axios-rate-limit', 'lodash/sumBy', 'lodash/uniqBy'].includes(name)) return untouched;
  throw Error(`unexpected_exchange_import:${name}`);
});
const exchange = new exchangeModule.CryptocomExchange('cryptocom', {}, {});
assert.equal(constructors, 2, 'mapping_socket_stubs');
const enumInventory = Object.fromEntries(['OrderType', 'OrderTimeInForce', 'OrderStopTriggerType'].map(name => [name, Object.entries(types[name])]));
const groups = [
  ['mapOrderStatus', ['ACTIVE', 'PENDING', 'FILLED', 'CANCELED', 'EXPIRED', 'REJECTED', '', 'UNKNOWN', 'active']],
  ['mapOrderType', [...Object.values(types.OrderType), '', 'UNKNOWN', 'MARKET']],
  ['mapCryptocomOrderType', ['MARKET', 'LIMIT', 'STOP_LOSS', 'STOP_LIMIT', 'TAKE_PROFIT', 'TAKE_PROFIT_LIMIT', '', 'UNKNOWN', 'market']],
  ['mapTimeInForce', [...Object.values(types.OrderTimeInForce), '', 'UNKNOWN', 'POST_ONLY']],
  ['mapStopTriggerType', [...Object.values(types.OrderStopTriggerType), '', 'UNKNOWN', 'INDEX_PRICE']],
  ['mapCryptocomTimeInForce', ['GOOD_TILL_CANCEL', 'IMMEDIATE_OR_CANCEL', 'FILL_OR_KILL', 'POST_ONLY', '', 'UNKNOWN', 'PostOnly']],
];
let coercions = 0;
const throwing = {[Symbol.toPrimitive]() {coercions++;throw Error('argument_coercion_called');}};
const nonStrings = [
  ['undefined', undefined], ['null', null], ['false', false], ['true', true], ['zero', 0],
  ['nan', NaN], ['infinity', Infinity], ['object', {}], ['array', []], ['boxed', new String('market')],
  ['symbol', Symbol('market')], ['bigint', 1n], ['function', () => {}], ['throwing', throwing],
];
function units(text) {return Array.from({length: text.length}, (_, index) => text.charCodeAt(index));}
const cases = groups.map(([method, strings]) => {
  const exotic = ['\ud800', '\udc00', '🙂', strings[0] + '\ud800', '\udc00' + strings[0]];
  const rows = [...strings, ...exotic].map(input => ({label: 'string', units: units(input), result: exchange[method](input)}));
  for (const [label, input] of nonStrings) rows.push({label, units: null, result: exchange[method](input)});
  return {method, rows};
});
assert.equal(coercions, 0, 'mapping_no_argument_coercion');
console.log(JSON.stringify({enumInventory, cases, coercions}));
