const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const test = require('node:test');
const ts = require('typescript');
const root = path.resolve(__dirname, '../../..');
const models = path.join(root, 'packages/safe-cex/src/generated');

test('generated market data matches the public contract in both directions', () => {
  for (const name of ['Candle', 'OrderBook', 'OrderBookOrders']) {
    assert.ok(fs.readFileSync(path.join(models, `${name}.ts`), 'utf8').includes(`export type ${name}`));
  }
  const fixture = path.join(models, '__market_data_contract_check__.ts');
  const source = `
import type { Candle } from './Candle';
import type { OrderBook } from './OrderBook';
import type { OrderBookOrders } from './OrderBookOrders';
import { OrderSide } from '../types';
import type { Candle as PublicCandle, OrderBook as PublicBook, OrderBookOrders as PublicLevel } from '../types';
type Equal<A, B> = (<T>() => T extends A ? 1 : 2) extends (<T>() => T extends B ? 1 : 2) ? true : false;
type Assert<T extends true> = T;
type CandleShape = Assert<Equal<Candle, {
 readonly timestamp: number; readonly open: number; readonly high: number; readonly low: number;
 readonly close: number; readonly volume: number; readonly lastSide?: OrderSide;
}>>;
type LevelShape = Assert<Equal<OrderBookOrders, { price: number; amount: number; total: number }>>;
type BookShape = Assert<Equal<OrderBook, { bids: OrderBookOrders[]; asks: OrderBookOrders[] }>>;
type CandleParity = Assert<Equal<Candle, PublicCandle>>;
type BookParity = Assert<Equal<OrderBook, PublicBook>>;
type LevelParity = Assert<Equal<OrderBookOrders, PublicLevel>>;
declare const generatedCandle: Candle;
declare const publicCandle: PublicCandle;
const a: PublicCandle = generatedCandle;
const b: Candle = publicCandle;
const c: Candle = { timestamp: 0, open: 0, high: 0, low: 0, close: 0, volume: 0 };
const sell: Candle = { ...c, lastSide: OrderSide.Sell };
// @ts-expect-error A runtime enum member, not a bare string, is required.
const bare: Candle = { ...c, lastSide: 'sell' };
// @ts-expect-error Absence does not permit null.
const nullable: Candle = { ...c, lastSide: null };
// @ts-expect-error Candle prices stay readonly.
c.close = 1;
// @ts-expect-error Candle timestamps stay readonly.
c.timestamp = 1;
// @ts-expect-error Optional fields stay readonly.
c.lastSide = OrderSide.Buy;
declare let book: OrderBook;
declare let level: OrderBookOrders;
book.bids = [];
book.asks.push(level);
level.price = 0;
level.amount = 0;
level.total = 0;
// @ts-expect-error The cumulative total is required.
const incomplete: OrderBookOrders = { price: 0, amount: 0 };
// @ts-expect-error Sides are required and non-nullable.
const missing: OrderBook = { bids: null };
`;
  const configFile = path.join(root, 'packages/safe-cex/tsconfig.json');
  const config = ts.readConfigFile(configFile, ts.sys.readFile);
  const parsed = ts.parseJsonConfigFileContent(config.config, ts.sys, path.dirname(configFile));
  const host = ts.createCompilerHost(parsed.options);
  const original = host.getSourceFile.bind(host);
  host.getSourceFile = (file, version, ...args) => file === fixture
    ? ts.createSourceFile(file, source, version, true) : original(file, version, ...args);
  const program = ts.createProgram([fixture], { ...parsed.options, incremental: false }, host);
  assert.deepEqual(ts.getPreEmitDiagnostics(program).map(d => ts.flattenDiagnosticMessageText(d.messageText, '\n')), []);
});
