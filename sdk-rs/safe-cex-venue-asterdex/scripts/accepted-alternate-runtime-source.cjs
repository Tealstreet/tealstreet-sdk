// Execute maintained socket classes and DefaultStore. Only base socket/time seams are injected.
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const { ts, root, values } = require('./source.cjs');
const load = require('./load-module.cjs');
const parse = file => ts.createSourceFile(file, fs.readFileSync(path.join(root, file), 'utf8'), ts.ScriptTarget.Latest, true);
const printer = ts.createPrinter();
const print = node => printer.printNode(ts.EmitHint.Unspecified, node, node.getSourceFile());
const compile = source => ts.transpileModule(source, { compilerOptions: { target: ts.ScriptTarget.ES2022, module: ts.ModuleKind.CommonJS } }).outputText;
const enumSource = ['packages/safe-cex/src/types.ts', 'packages/logging/src/LogSeverity.ts']
  .flatMap(file => parse(file).statements.filter(ts.isEnumDeclaration)).map(print).join('\n');
const enums = {};
new Function('exports', compile(enumSource))(enums);
const compiler = path.resolve(path.dirname(require.resolve(process.env.SAFE_CEX_TYPESCRIPT_PATH)), '..');
const dep = name => require(path.join(path.dirname(compiler), name));
const clone = load('packages/safe-cex/src/utils/clone.ts', [], {});
const stores = {};
new Function('exports', 'clone', 'virtualClock', 'uniqBy', compile(enumSource + '\n' +
  parse('packages/safe-cex/src/store/store.base.ts').statements.filter(n => !ts.isImportDeclaration(n)).map(print).join('\n')))(stores, clone.clone, { getCurrentTimeMs: () => 1234 }, dep('lodash/uniqBy'));
class Base {
  constructor(parent) { this.parent = parent; this.isDisposed = false; this.onClose = () => {}; }
}
const jsonParser = load('packages/json-parse/src/jsonParse.ts', [], {});
const sockets = load('packages/safe-cex/src/exchanges/asterdex/asterdex.ws-public.ts', [], {
  '@tealstreet/json-parse/src/jsonParse': jsonParser,
  '../../config/config': { ENABLE_RECONNECTING_WEBSOCKET: false },
  '../../utils/orderbook': load('packages/safe-cex/src/utils/orderbook.ts', [], {}),
  '../base.ws': { BaseWebSocket: Base }, './asterdex.types': values, nanoid: { nanoid: () => 'owner' },
});
const privateSockets = load('packages/safe-cex/src/exchanges/asterdex/asterdex.ws-private.ts', [], {
  '@tealstreet/json-parse/src/jsonParse': jsonParser,
  '../../config/config': { ENABLE_RECONNECTING_WEBSOCKET: false }, '../../types': enums,
  '../../utils/virtualClock': { virtualClock: { getCurrentTimeMs: () => 1234 } },
  '../base.ws': { BaseWebSocket: Base }, './asterdex.types': values,
});
const exchangeClass = parse('packages/safe-cex/src/exchanges/asterdex/asterdex.exchange.ts').statements
  .find(n => ts.isClassDeclaration(n) && n.name.text === 'AsterDEXExchange');
const method = exchangeClass.members.find(n => n.name?.getText() === 'reverseMapOrderType');
assert.ok(method);
const mapping = {};
new Function('exports', ...Object.keys(enums), compile(`export class Mapping { ${print(method)} }`))(mapping, ...Object.values(enums));
const reverse = new mapping.Mapping();
const bits = value => { const b = Buffer.alloc(8); b.writeDoubleBE(value); return b.toString('hex'); };
const sourceNames = {
  handle_ticker_update: 'handleTickerUpdate', handle_mark_price_update: 'handleMarkPriceUpdate',
  handle_trade_update: 'handleTradeUpdate', handle_depth_snapshot: 'handleDepthSnapshot', handle_kline_update: 'handleKlineUpdate',
  handle_order_trade_update: 'handleOrderTradeUpdate', apply_balance_updates: 'applyBalanceUpdates',
  handle_account_update: 'handleAccountUpdate', handle_account_position_update: 'handleAccountPositionUpdate',
};
const camel = field => field.replace(/_([a-z])/g, (_, c) => c.toUpperCase());
module.exports = function verify(contracts) {
  let count = 0;
  const originalSet = global.setTimeout, originalClear = global.clearTimeout;
  let timer = 0;
  global.setTimeout = () => ++timer;
  global.clearTimeout = () => {};
  try {
    for (const owner of ['public_stream::PublicStream', 'private_stream::PrivateStream']) {
      const contract = contracts[`safe_cex_venue_asterdex::${owner}`];
      for (const entry of contract.entrypoints.filter(e => e.input === 'observed')) {
        const name = entry.api.name;
        assert.ok(sourceNames[name], name);
        for (const row of entry.cases) {
          const store = new stores.DefaultStore();
          store.update({ tickers: [{ id: 'BTCUSDT', symbol: 'BTCUSDT', bid: 9, ask: 11, last: 102,
            mark: 0, index: 0, percentage: 1, openInterest: 7, fundingRate: 0, fundingTime: 0, volume: 50, quoteVolume: 5000 }],
            balance: { total: 0, free: 0, used: 0, upnl: 0 } });
          const events = [];
          const parent = { store, options: { key: 'key', secret: 'secret' }, isDisposed: false,
            log() {}, markBalanceStateUpdatedByWebsocket: () => 1, markPositionStateUpdatedByWebsocket() {},
            reverseMapOrderType: type => reverse.reverseMapOrderType(type),
            emitter: { emit: (event, ...args) => events.push({ event, args }) } };
          const input = structuredClone(row.input);
          let output;
          if (owner.startsWith('public')) {
            const stream = new sockets.AsterDEXPublicWebsocket(parent, store);
            let close;
            if (name === 'handle_kline_update') stream.klineCallbacks.set('btcusdt@kline_1m', c => { close = c.close; });
            stream[sourceNames[name]](input);
            if (name === 'handle_kline_update') output = bits(close);
            else if (name === 'handle_trade_update') output = { last: bits(store.tickers[0].last), side: events[0].args[0].side, symbol: events[0].args[0].symbol };
            else output = Object.fromEntries(Object.keys(row.output).map(key => [key, bits(store.tickers[0][camel(key)])]));
          } else {
            const stream = new privateSockets.AsterDEXPrivateWebsocket(parent, store);
            if (name === 'apply_balance_updates') {
              assert.equal(stream.applyBalanceUpdates([{ asset: 'USDT', total: input[0], free: input[1], used: input[2] }], new Map()), false);
            } else stream[sourceNames[name]](input);
            if (name === 'handle_order_trade_update') output = store.orders.map(order => ({ id: order.id, symbol: order.symbol, amount: bits(order.amount), remaining: bits(order.remaining) }));
            else output = Object.fromEntries(['total', 'free', 'used'].map(key => [key, bits(store.balance[key])]));
          }
          assert.deepEqual(output, row.output, `${owner}::${name}: actual maintained class and Store`);
          count++;
        }
      }
    }
    for (const entry of contracts['safe_cex_venue_asterdex::order_execution::OpenInterestMap'].entrypoints) {
      for (const row of entry.cases) {
        const map = new Map([['first', 1.25], ['second', 2.5]]);
        const removed = entry.api.name === 'delete' ? map.delete(row.input) : map.set(row.input.key, row.input.value);
        const values = [...map].map(([key, value]) => [key, bits(value)]);
        assert.deepEqual(entry.api.name === 'delete' ? { removed, values } : values, row.output);
        count++;
      }
    }
  } finally { global.setTimeout = originalSet; global.clearTimeout = originalClear; }
  assert.equal(count, 22);
  console.log(`Aster alternate runtime source: ${count} actual socket/Store/Map expectations pass.`);
};
