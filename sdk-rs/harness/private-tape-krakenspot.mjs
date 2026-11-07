// Documented Spot REST and WS v2 payloads; no live credentials or private data.
// https://docs.kraken.com/exchange/api-reference/spot-websocket-v2/executions
import { mkdirSync, writeFileSync } from 'node:fs';
import { dirname } from 'node:path';
import { pathToFileURL } from 'node:url';
import { parseArgs } from 'node:util';
export const KRAKEN_SPOT_EPOCH = 1792454400000;
export const krakenSpotPairs = { error: [], result: {
  XXBTZUSD: { altname: 'XBTUSD', wsname: 'XBT/USD', base: 'XXBT', quote: 'ZUSD', pair_decimals: 1, lot_decimals: 3, ordermin: '0.001', status: 'online' },
  XETHZUSD: { altname: 'ETHUSD', wsname: 'ETH/USD', base: 'XETH', quote: 'ZUSD', pair_decimals: 2, lot_decimals: 3, ordermin: '0.001', status: 'online' },
} };
export const krakenSpotTickers = { error: [], result: Object.fromEntries(['XXBTZUSD', 'XETHZUSD'].map((pair) => [pair, { a: ['101', '1', '1'], b: ['99', '1', '1'], c: ['100', '1'], v: ['100', '100'], p: ['100', '100'], t: [2, 2], l: ['90', '90'], h: ['110', '110'], o: '100' }])) };
export function buildKrakenSpotTape() {
  const records = [{ t: 0, type: 'meta', epoch: KRAKEN_SPOT_EPOCH, private: true, source: 'documented-scripted', specification: 'https://docs.kraken.com/exchange/api-reference/spot-websocket-v2/executions' }];
  const http = (method, path, result, requestBody, extra = {}) => records.push({ t: 0, type: 'http', method, url: `https://api.kraken.com${path}`, status: 200, body: JSON.stringify({ error: [], result }), ...(requestBody === undefined ? {} : { requestBody: new URLSearchParams({ ...requestBody, nonce: '0' }).toString() }), ...extra });
  const ws = (t, channel, type, data, socket = 0) => records.push({ t, type: 'ws', url: 'wss://ws-auth.kraken.com/v2', socket, text: JSON.stringify({ channel, type, data }) });
  const command = (t, op, args = {}) => records.push({ t, type: 'command', op, ...args });
  const stage = () => ({ afterCommand: records.filter((r) => r.type === 'command').length });
  http('GET', '/0/public/AssetPairs', krakenSpotPairs.result);
  http('GET', '/0/public/Ticker', krakenSpotTickers.result);
  http('POST', '/0/private/Balance', { ZUSD: '1000', XXBT: '2', 'XXBT.F': '0.01' }, {});
  http('POST', '/0/private/GetWebSocketsToken', { token: 'replay-spot-token', expires: 900 }, {});
  http('POST', '/0/private/GetWebSocketsToken', { token: 'replay-spot-token-2', expires: 900 }, {}, { availableAt: 5000 });
  const order = (price = '90', qty = '2', filled = '0') => ({ descr: { pair: 'XBTUSD', type: 'buy', ordertype: 'limit', price }, vol: qty, vol_exec: filled, oflags: 'post', opentm: KRAKEN_SPOT_EPOCH / 1000 });
  http('POST', '/0/private/OpenOrders', { open: { 'O-100': order() } }, {}, stage());
  const execution = (extra = {}) => ({ order_id: 'O-100', symbol: 'BTC/USD', side: 'buy', order_type: 'limit', limit_price: 90, order_qty: 2, cum_qty: 0, exec_type: 'new', time_in_force: 'GTC', post_only: true, timestamp: new Date(KRAKEN_SPOT_EPOCH).toISOString(), ...extra });
  ws(100, 'executions', 'snapshot', [execution()]);
  ws(200, 'balances', 'snapshot', [{ asset: 'USD', balance: 1000, wallets: [{ type: 'spot', id: 'main', balance: 1000 }] }]);
  command(600, 'placeOrders', { orders: [{ symbol: 'XBT/USD', type: 'limit', side: 'buy', amount: 1, price: 90, clientOrderId: 'caller-not-supported' }] });
  const place = (t, type, price, id, more = {}) => {
    command(t, 'placeOrders', { orders: [{ symbol: 'XBT/USD', type, side: 'sell', amount: 0.25, ...(price === undefined ? {} : { price }), ...more }] });
    const ordertype = { limit: 'limit', market: 'market', stop_market: 'stop-loss', take_profit_market: 'take-profit', trailing_stop_market: 'trailing-stop' }[type];
    http('POST', '/0/private/AddOrder', { txid: [id], descr: { order: 'documented order' } }, { pair: 'XXBTZUSD', type: 'sell', ordertype, volume: '0.25', cl_ord_id: 'a'.repeat(32), ...(price === undefined ? {} : { price: type === 'trailing_stop_market' ? '+5' : String(price) }) }, stage());
  };
  // Off the TS one-second balance poll: TS signs each of its two clients with its
  // own nonce counter, so two requests in one millisecond can go out of order.
  place(1100, 'limit', 110, 'O-201');
  place(1400, 'market', undefined, 'O-202');
  place(1800, 'stop_market', 80, 'O-203');
  place(2200, 'take_profit_market', 120, 'O-204');
  place(2600, 'trailing_stop_market', 95, 'O-205');
  command(3000, 'amendOrder', { id: 'O-100', price: 91, amount: 3 });
  http('POST', '/0/private/AmendOrder', { amend_id: 'amend-1' }, { txid: 'O-100', order_qty: '3', limit_price: '91', post_only: 'true' }, stage());
  http('POST', '/0/private/OpenOrders', { open: { 'O-100': order('91', '3') } }, {}, stage());
  ws(3200, 'executions', 'update', [execution({ exec_type: 'amended', order_qty: 3, limit_price: 91, timestamp: new Date(KRAKEN_SPOT_EPOCH + 3200).toISOString() })]);
  command(3600, 'cancelOrders', { ids: ['O-100'] });
  http('POST', '/0/private/CancelOrder', { count: 1 }, { txid: 'O-100' }, stage());
  http('POST', '/0/private/OpenOrders', { open: {} }, {}, stage());
  ws(3800, 'executions', 'update', [execution({ exec_type: 'canceled', timestamp: new Date(KRAKEN_SPOT_EPOCH + 3800).toISOString() })]);
  for (const op of ['fetchFills', 'fetchOrderHistory', 'fetchIncome']) command(4400, op, { symbol: 'XBT/USD' });
  command(4600, 'fetchCandles', { symbol: 'XBT/USD', interval: '1m', from: KRAKEN_SPOT_EPOCH - 60000, to: KRAKEN_SPOT_EPOCH });
  http('GET', `/0/public/OHLC?pair=XXBTZUSD&interval=1&since=${KRAKEN_SPOT_EPOCH / 1000 - 60}`,
    { XXBTZUSD: [[KRAKEN_SPOT_EPOCH / 1000 - 60, '99', '101', '98', '100', '100', '12', 3]], last: KRAKEN_SPOT_EPOCH / 1000 - 60 });
  records.push({ t: 5000, type: 'close', url: 'wss://ws-auth.kraken.com/v2', socket: 0, code: 1006 });
  ws(11000, 'executions', 'snapshot', [], 1);
  ws(11200, 'balances', 'update', [{ asset: 'USD', balance: 1000 }], 1);
  return records;
}
if (process.argv[1] && import.meta.url === pathToFileURL(process.argv[1]).href) {
  const { values } = parseArgs({ options: { out: { type: 'string', default: 'sdk-rs/harness/tapes/krakenspot-private.jsonl' } } });
  const records = buildKrakenSpotTape();
  mkdirSync(dirname(values.out), { recursive: true });
  writeFileSync(values.out, records.map((r) => JSON.stringify(r)).join('\n') + '\n');
  console.log(JSON.stringify({ out: values.out, records: records.length, source: 'documented-scripted' }));
}
