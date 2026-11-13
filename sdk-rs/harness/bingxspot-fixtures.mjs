// Offline Spot v3 examples, adapted from the independent Spot oracle's fixtures.
import { readFileSync, mkdirSync, writeFileSync } from 'node:fs';
import { dirname, resolve } from 'node:path';
import { pathToFileURL } from 'node:url';
import { gzipSync } from 'node:zlib';
import { parseArgs } from 'node:util';

export const SPOT_REST = 'https://open-api.bingx.com';
export const SPOT_SOCKET = 'wss://open-api-ws.bingx.com/market';
export const SPOT_PRIVATE_SOCKET = `${SPOT_SOCKET}?listenKey=spot-fixture-session`;
const source = new URL('../venues/bingx/tests/fixtures/spot/', import.meta.url);
const fixture = name => readFileSync(new URL(`${name}.json`, source), 'utf8');
export function spotReplayRecords(privateAccount = false) {
  const records = [{ t: 0, type: 'meta', epoch: 1702720824000, private: privateAccount,
    provenance: 'BingX Spot v3 documented REST examples and synthetic Spot WS/command sequence; no live account' }];
  const http = (method, path, body, extra = {}) => records.push({ t: 0, type: 'http', method,
    url: SPOT_REST + path, status: 200, body, ...extra });
  const ok = data => JSON.stringify({ code: 0, data });
  http('GET', '/openApi/spot/v1/common/symbols', fixture('markets'));
  http('GET', '/openApi/spot/v2/quote/ticker', fixture('tickers'));
  const ws = (t, url, data) => records.push({ t, type: 'ws', url, text: typeof data === 'string' ? data : JSON.stringify(data) });
  if (!privateAccount) {
    for (let index = 0; index < 120; index++) {
      const price = 42215 + index / 100;
      ws(10 + index * 10, SPOT_SOCKET, { code: 0, dataType: 'BTC-USDT@ticker', data: {
        s: 'BTC-USDT', c: String(price), P: '2.91', v: '5099.21', q: '214948771.34',
        B: String(price - 0.5), A: String(price + 0.5),
      } });
      ws(11 + index * 10, SPOT_SOCKET, { code: 0, dataType: 'BTC-USDT@depth50', data: {
        bids: [[String(price - 0.5), '0.5'], [String(price - 1), '1']],
        asks: [[String(price + 1), '2'], [String(price + 0.5), '0.75']],
      } });
    }
    ws(1250, SPOT_SOCKET, 'Ping');
  } else {
    http('POST', '/openApi/user/auth/userDataStream', JSON.stringify({ listenKey: 'spot-fixture-session' }));
    http('GET', '/openApi/spot/v1/account/balance', fixture('balances'));
    http('GET', '/openApi/spot/v1/trade/openOrders', ok({ orders: [] }));
    const command = (t, value) => records.push({ t, type: 'command', ...value });
    const order = { symbol: 'BTC-USDT', orderId: '9007199254740993', price: '41000',
      origQty: '0.002', executedQty: '0', status: 'NEW', type: 'LIMIT', side: 'BUY', clientOrderID: 'spot_limit' };
    command(100, { op: 'placeOrders', orders: [{ symbol: 'BTCUSDT', type: 'limit', side: 'buy',
      amount: 0.002, price: 41000, clientOrderId: 'spot_limit' }], expected: [order.orderId] });
    http('POST', '/openApi/spot/v1/trade/order', ok(order), { afterCommand: 1,
      requestBody: JSON.stringify({ symbol: 'BTC-USDT', side: 'BUY', type: 'LIMIT',
        quantity: 0.002, price: 41000, timeInForce: 'GTC', newClientOrderId: 'spot_limit' }) });
    const report = (status, filled, last, trade = '9007199254740995') => ({ code: 0,
      dataType: 'spot.executionReport', data: { s: 'BTC-USDT', i: JSON.rawJSON(order.orderId), C: 'spot_limit',
        S: 'BUY', o: 'LIMIT', X: status, q: '0.002', z: filled, p: '41000',
        l: last, L: '41000', n: '0.01', t: JSON.rawJSON(trade), T: 1702720824200 } });
    ws(200, SPOT_PRIVATE_SOCKET, report('PARTIALLY_FILLED', '0.001', '0.001'));
    ws(250, SPOT_PRIVATE_SOCKET, report('PARTIALLY_FILLED', '0.001', '0.001'));
    command(350, { op: 'cancelOrders', ids: [order.orderId], expected: [] });
    http('POST', '/openApi/spot/v1/trade/cancel', ok({ ...order, status: 'CANCELED' }), { afterCommand: 2,
      requestBody: JSON.stringify({ symbol: 'BTC-USDT', orderId: order.orderId }) });
    ws(400, SPOT_PRIVATE_SOCKET, report('CANCELED', '0.001', '0'));
    command(500, { op: 'placeOrders', orders: [{ symbol: 'BTCUSDT', type: 'market', side: 'buy',
      amount: 0.002, price: 41000, clientOrderId: 'spot_market_buy' }], expected: ['9007199254740997'] });
    http('POST', '/openApi/spot/v1/trade/order', ok({ ...order, orderId: '9007199254740997',
      type: 'MARKET', side: 'BUY', status: 'FILLED' }), { afterCommand: 3,
      requestBody: JSON.stringify({ symbol: 'BTC-USDT', side: 'BUY', type: 'MARKET',
        quoteOrderQty: 82, newClientOrderId: 'spot_market_buy' }) });
    command(600, { op: 'placeOrders', orders: [{ symbol: 'BTCUSDT', type: 'market', side: 'sell',
      amount: 0.002, clientOrderId: 'spot_market_sell' }], expected: ['9007199254740999'] });
    http('POST', '/openApi/spot/v1/trade/order', ok({ ...order, orderId: '9007199254740999',
      type: 'MARKET', side: 'SELL', status: 'FILLED' }), { afterCommand: 4,
      requestBody: JSON.stringify({ symbol: 'BTC-USDT', side: 'SELL', type: 'MARKET',
        quantity: 0.002, newClientOrderId: 'spot_market_sell' }) });
    command(700, { op: 'fetchCandles', symbol: 'BTCUSDT', interval: '1m', limit: 2 });
    http('GET', '/openApi/spot/v2/quote/klines?symbol=BTC-USDT&interval=1m&limit=2&timeZone=0', fixture('candles'));
    command(800, { op: 'fetchFills', symbol: 'BTCUSDT', limit: 10 });
    http('GET', '/openApi/spot/v1/trade/myTrades?symbol=BTC-USDT&limit=10', fixture('fills'));
    command(900, { op: 'fetchOrderBook', symbol: 'BTCUSDT' });
    http('GET', '/openApi/spot/v1/market/depth?symbol=BTC-USDT&limit=50', fixture('depth'));
    ws(1000, SPOT_PRIVATE_SOCKET, { code: 0, dataType: 'ACCOUNT_UPDATE', data: {} });
    ws(1100, SPOT_PRIVATE_SOCKET, 'Ping');
  }
  const compressed = records.find(r => r.type === 'ws' && r.text.startsWith('{'));
  compressed.binary = gzipSync(Buffer.from(compressed.text)).toString('base64');
  delete compressed.text;
  return records.sort((a, b) => a.t - b.t);
}

if (import.meta.url === pathToFileURL(process.argv[1] ?? '').href) {
  const { values } = parseArgs({ options: { public: { type: 'boolean', default: false }, out: { type: 'string' } } });
  const out = values.out ?? (values.public ? 'sdk-rs/harness/fixtures/bingxspot-public.jsonl.gz' : 'sdk-rs/harness/tapes/bingxspot-private.jsonl');
  const bytes = Buffer.from(spotReplayRecords(!values.public).map(JSON.stringify).join('\n') + '\n');
  mkdirSync(dirname(resolve(out)), { recursive: true });
  writeFileSync(out, out.endsWith('.gz') ? gzipSync(bytes) : bytes);
  console.log(JSON.stringify({ out, records: spotReplayRecords(!values.public).length }));
}
