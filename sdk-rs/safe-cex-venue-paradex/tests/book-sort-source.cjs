const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const vm = require('node:vm');
const ts = require(process.env.SAFE_CEX_TYPESCRIPT_PATH);
const root = path.resolve(__dirname, '../../..');
const parse = file => ts.createSourceFile(file, fs.readFileSync(path.join(root, file), 'utf8'), 99, true);
const ws = parse('packages/safe-cex/src/exchanges/paradex/paradex.ws-public.ts');
function run(code, module, context) {
  const output = ts.transpileModule(code, {compilerOptions: {module: 1, target: 9}, reportDiagnostics: true});
  assert.equal(output.diagnostics.length, 0);
  vm.runInNewContext(output.outputText, {module, exports: module.exports, ...context});
}

let now = 1700000000000, next = 0;
const timers = new Map(), logs = [];
const context = {
  Date: {now: () => now},
  setTimeout(fn, delay) { const id = ++next; timers.set(id, {fn, at: now + delay, delay}); return id; },
  clearTimeout(id) { timers.delete(id); },
  require(name) {
    if (name === 'lodash/throttle') return require(path.join(root, 'node_modules/lodash/throttle'));
    if (name.endsWith('jsonParse')) return {jsonParse: JSON.parse};
    if (name === '../../types') return {LogSeverity: {Warning: 'warning', Error: 'error', Info: 'info'}};
    if (name === '../base.ws') return {BaseWebSocket: class {
      constructor(parent) { this.parent = parent; this.isConnected = false; this.isDisposed = false; }
      dispose() { this.isDisposed = true; }
    }};
    if (name === './paradex.types') return {BASE_WS_URL: {public: {livenet: 'wss://fixture.invalid'}}};
    throw new Error(`Unexpected source dependency: ${name}`);
  },
};

const moduleWs = {exports: {}};
run(ws.getText(ws), moduleWs, context);
const updates = [], books = [];
const parent = {options: {}, log: (...args) => logs.push(args), store: {
  updateTicker: (where, patch) => updates.push({where, patch}),
}};
const socket = new moduleWs.exports.ParadexPublicWebSocket(parent);
socket.orderbookCallbacks.set('BTC-USD-PERP', new Set([book => books.push(book)]));
let reads = 0;
const inserts = Array.from({length: 32}, () => ({side: 'BUY', size: '2', get price() {
  ++reads;
  throw new TypeError(`sort getter ${reads}`);
}}));
inserts.push({side: 'SELL', price: '102', size: '3'});
socket.handleOrderbookUpdate({data: {market: 'BTC-USD-PERP', inserts}});
assert.equal(reads, 1);
assert.equal(updates.length, 0);
assert.equal(books.length, 0);
assert.equal(logs.length, 1);
assert.match(logs[0][0], /sort getter 1/);
const prices = ['NaN','558','NaN','197','572','NaN','NaN','48','245','415','459','NaN','665','NaN','625','832','557','440','367','613','NaN','NaN'];
for (const side of ['bids', 'asks']) {
  socket.handleOrderbookUpdate.cancel();
  const data = {market: 'BTC-USD-PERP', bids: [['101','2']], asks: [['102','3']]};
  data[side] = prices.map((p, i) => [p, String(i+1)]);
  socket.handleOrderbookUpdate({data});
  const book = books.at(-1);
  assert.equal(book[side].length, prices.length);
  for (let i = 0; i < prices.length; ++i) {
    const level = book[side].find(row => row.amount === i+1);
    assert.ok(Object.is(level.price, Number(prices[i])));
  }
}
socket.handleOrderbookUpdate.cancel();
assert.equal(books.length, 2);
process.stdout.write(JSON.stringify({reads, books: books.length, levels: prices.length}));
