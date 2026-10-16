import fs from 'node:fs';
import vm from 'node:vm';
import { createRequire } from 'node:module';
const require = createRequire(import.meta.url);
if (!process.env.SAFE_CEX_TYPESCRIPT_PATH) throw Error('isolated TypeScript compiler required');
const ts = require(process.env.SAFE_CEX_TYPESCRIPT_PATH);
const root = new URL('../../../', import.meta.url);
function body(path) {
  const source = fs.readFileSync(new URL(path, root), 'utf8');
  const ast = ts.createSourceFile(path, source, ts.ScriptTarget.Latest, true);
  return ast.statements.filter(n => !ts.isImportDeclaration(n)).map(n => n.getText(ast)).join('\n').replace(/\bexport /g, '');
}
let source = body('packages/safe-cex/src/utils/safe-math.ts') + '\n' + body('packages/safe-cex/src/utils/risk.ts') + '\n' + body('packages/safe-cex/src/webworker/postProcessors/OrdersRiskProfilePostprocessor.ts');
if (process.argv.includes('--control-no-clear')) {
  const wrong = source.replace('(store.ordersRisk as Writable<typeof store.ordersRisk>) = {};', 'void store;');
  if (wrong === source) throw Error('source recovery mutation must apply');
  source = wrong;
}
const logs = [];
const context = vm.createContext({ BasePostprocessor: class {}, console: { trace() {}, error(error) { logs.push({ name: error.name, message: error.message }); } },
  OrderStatus: { Open: 'open' }, OrderType: { Limit: 'limit', TrailingStopLoss: 'trailing_stop_market' }, OrderSide: { Buy: 'buy', Sell: 'sell' }, PositionSide: { Long: 'long', Short: 'short' }, PositionMode: { Hedged: 'hedged' } });
vm.runInContext(ts.transpileModule(source + '\nglobalThis.api = { calculateOrdersRisk, OrdersRiskProfilePostprocessor, formatPnl };', { compilerOptions: { target: ts.ScriptTarget.ES2022 } }).outputText, context);
const market = { id: 'BTC', symbol: 'BTC', base: 'BTC', quote: 'USD', active: true, precision: { amount: .01, price: .01 }, limits: { amount: { min: 0, max: 100 }, leverage: { min: 1, max: 100 } }, makerFee: .001, takerFee: .002 };
const ticker = { id: 'BTC', symbol: 'BTC', bid: 99, ask: 101, last: 100, mark: 100, index: 100, percentage: 0, openInterest: 0, fundingRate: 0, fundingTime: 0, volume: 0, quoteVolume: 0 };
const position = { id: 'p', symbol: 'BTC', side: 'long', entryPrice: 100, contracts: 2, notional: 200, leverage: 5, unrealizedPnl: 10, liquidationPrice: 0 };
const order = (id, price) => ({ id, symbol: 'BTC', status: 'open', type: 'limit', side: 'sell', price, amount: 1, filled: 0, remaining: 1, reduceOnly: true, lastUpdate: 0 });
const seed = { markets: [market], tickers: [ticker], positions: [position], orders: [order('prior', 105)] };
const prior = context.api.calculateOrdersRisk(seed.markets, seed.tickers, seed.positions, seed.orders);
const rows = [];
function recovery(input, key, field) {
  let failure;
  try { context.api.calculateOrdersRisk(input.markets, input.tickers, input.positions, input.orders); }
  catch (error) { failure = { name: error.name, message: error.message }; }
  const store = { ...input, ordersRisk: { ...prior } };
  logs.length = 0;
  new context.api.OrdersRiskProfilePostprocessor().process(store);
  if (!failure || Object.keys(store.ordersRisk).length || logs.length !== 1) throw Error('source risk recovery must visibly clear every prior row: '+key+'/'+field+' '+JSON.stringify({failure,risk:store.ordersRisk,logs}));
  rows.push({ key, field, input, failure, cleared: store.ordersRisk, logged: logs[0] });
}
for (const key of Object.getOwnPropertyNames(Object.prototype)) {
  for (const field of ['symbol', 'id']) for (const price of [90, 110]) {
    recovery({ ...seed, orders: [...seed.orders, { ...order('bad', price), [field]: key }] }, key, field);
  }

}
const controls = [];
for (const key of Object.getOwnPropertyNames(Object.prototype)) {
  // Source symbolsWithOpenOrders is a Set: position-only prototype names are
  // excluded even if a ticker is present. Do not widen membership to inheritance.
  controls.push({ input: { ...seed, positions: [...seed.positions, { ...position, symbol: key }], tickers: [...seed.tickers, { ...ticker, symbol: key }] }, expected: prior });
  for (const input of [{ ...seed, orders: [...seed.orders, { ...order('closed-symbol', 110), symbol: key, status: 'closed' }] }, { ...seed, orders: [...seed.orders, { ...order(key, 110), status: 'closed' }] }, { ...seed, positions: [], orders: [order(key, 110)] }]) {
    controls.push({ input, expected: context.api.calculateOrdersRisk(input.markets, input.tickers, input.positions, input.orders) });
  }
}
for (const key of [...Object.getOwnPropertyNames(Object.prototype), 'ordinary-skipped']) {
  const input = { ...seed, positions: [{ ...position, positionMode: 'hedged' }], orders: [{ ...order(key, 110), reduceOnly: false }] };
  controls.push({ input, expected: context.api.calculateOrdersRisk(input.markets, input.tickers, input.positions, input.orders) });
}
const ordering = [];
for (const ids of [['z', 'a', '10', '2'], ['2', '10', 'a', 'z'], ['a', 'z', '01', '1']]) {
  const input = { ...seed, orders: ids.map((id, i) => order(id, 101 + i)) };
  ordering.push({ input, expectedKeys: Object.keys(context.api.calculateOrdersRisk(input.markets, input.tickers, input.positions, input.orders)) });
}
const formats = [];
for (const precision of [-1, 101, Infinity, -Infinity, -0.5, 100.9, NaN]) for (const value of [1, NaN]) {
  try { formats.push({ precision: String(precision), value: String(value), expected: context.api.formatPnl(value, precision) }); }
  catch (error) { formats.push({ precision: String(precision), value: String(value), failure: { name: error.name, message: error.message } }); }
}
// Capture numeric observations before the JSON transport can erase -0/NaN.
function bits(value) { const bytes = Buffer.alloc(8); bytes.writeDoubleBE(value); return bytes.readBigUInt64BE().toString(); }
for (const control of controls) {
  control.nativeNumbers = Object.fromEntries(Object.entries(control.expected).map(([key, profiles]) => [key, profiles.map(profile => Object.fromEntries(['orderPnl', 'totalRealizedPnl', 'rollingUnrealizedPnl', 'positionSize', 'fee'].filter(field => Object.hasOwn(profile, field)).map(field => [field, bits(profile[field])])))]));
}
process.stdout.write(JSON.stringify({ rows, controls, formats, ordering }));
