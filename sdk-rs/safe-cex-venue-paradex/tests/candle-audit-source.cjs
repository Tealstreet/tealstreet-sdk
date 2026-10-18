const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const vm = require('node:vm');
const ts = require(process.env.SAFE_CEX_TYPESCRIPT_PATH);
const root = path.resolve(__dirname, '../../..');
const parse = file => ts.createSourceFile(file, fs.readFileSync(path.join(root, file), 'utf8'), 99, true);
const ws = parse('packages/safe-cex/src/exchanges/paradex/paradex.ws-public.ts');
const exchange = parse('packages/safe-cex/src/exchanges/paradex/paradex.exchange.ts');
const owner = exchange.statements.find(n => ts.isClassDeclaration(n) && n.name.text === 'ParadexExchange');
function member(name) {
  const found = owner.members.filter(n => n.name?.getText(exchange) === name);
  assert.equal(found.length, 1, name);
  return found[0].getText(exchange);
}
const types = parse('packages/safe-cex/src/types.ts');
const timeframe = types.statements.find(n => ts.isVariableStatement(n) && n.declarationList.declarations.some(d => d.name.getText(types) === 'timeframeToSeconds'));
const moduleTypes = {exports: {}};
function run(code, module, context) {
  const output = ts.transpileModule(code, {compilerOptions: {module: 1, target: 9}, reportDiagnostics: true});
  assert.equal(output.diagnostics.length, 0);
  vm.runInNewContext(output.outputText, {module, exports: module.exports, ...context});
}
run(timeframe.getText(types), moduleTypes, {});
let now = 1700000000000, next = 0, close = 102;
const timers = new Map(), reads = [], logs = [], first = [], second = [];
const context = {
  Date: {now: () => now},
  setTimeout(fn, delay) { const id = ++next; timers.set(id, {fn, at: now + delay, delay}); return id; },
  clearTimeout(id) { timers.delete(id); },
  require(name) {
    if (name === 'lodash/throttle') return require(path.join(root, 'node_modules/lodash/throttle'));
    if (name.endsWith('jsonParse')) return {jsonParse: JSON.parse};
    if (name === '../../types') return {...moduleTypes.exports, LogSeverity: {Warning: 'warning', Error: 'error', Info: 'info'}};
    if (name === '../base.ws') return {BaseWebSocket: class {
      constructor(parent) { this.parent = parent; this.isConnected = false; this.isDisposed = false; }
      dispose() { this.isDisposed = true; }
    }};
    if (name === './paradex.types') return {BASE_WS_URL: {public: {livenet: 'wss://fixture.invalid'}}};
    throw new Error(`Unexpected source dependency: ${name}`);
  },
};
const parentModule = {exports: {}};
run(`export class Parent {${member('fetchOHLCV')}\n${member('convertIntervalToTradingViewResolution')}}`, parentModule, {
  timeframeToSeconds: moduleTypes.exports.timeframeToSeconds,
  ENDPOINTS: {OHLCV: '/markets/klines', TRADINGVIEW_HISTORY: '/tradingview/history'},
  LogSeverity: {Warning: 'warning', Error: 'error'},
  omitUndefined: v => Object.fromEntries(Object.entries(v).filter(([, value]) => value !== undefined)),
});
const parent = new parentModule.exports.Parent();
Object.assign(parent, {options: {}, isPublicDataOnly: true, log: (...args) => logs.push(args), xhr: {
  async get(url, config) {
    reads.push({url, at: now, params: config.params});
    if (now === 1700000000000) throw new Error('candle terminal failure');
    return {data: {results: [[1700000000000, 101, 105, 100, close, 7]]}};
  },
}});
const wsModule = {exports: {}};
run(ws.getText(ws), wsModule, context);
const socket = new wsModule.exports.ParadexPublicWebSocket(parent);
const opts = {symbol: 'BTC-USD-PERP', interval: '1m'};
const cleanupFirst = socket.listenOHLCV(opts, row => first.push(row.close));
const cleanupSecond = socket.listenOHLCV(opts, row => second.push(row.close));
const flush = async () => { for (let i = 0; i < 32; ++i) await Promise.resolve(); };
async function advance(offset) {
  now = 1700000000000 + offset;
  for (const [id, timer] of [...timers]) if (timer.at <= now) { timers.delete(id); await timer.fn(); }
  await flush();
}
(async () => {
  await flush();
  assert.equal(reads.length, 2, 'documented and fallback requests both fail');
  assert.deepEqual([...timers.values()].map(t => t.delay), [3000]);
  await advance(2999);
  assert.equal(reads.length, 2, 'no early retry');
  await advance(3000);
  await advance(6000);
  cleanupFirst();
  close = 104;
  await advance(9000);
  cleanupSecond();
  await advance(12000);
  assert.deepEqual(first, [102]);
  assert.deepEqual(second, [102, 104]);
  assert.equal(reads.length, 5);
  assert.equal(timers.size, 0);
  assert.equal(socket.ohlcvCallbacks.size, 0);
  assert.equal(socket.tradeSubscriptionCounts.size, 0);
  assert.equal(logs.filter(([message]) => message.includes('candle terminal failure')).length, 2);
  process.stdout.write(JSON.stringify({reads, first, second, logs, remainingTimers: timers.size}));
})().catch(error => { console.error(error); process.exitCode = 1; });
