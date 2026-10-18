// The napi NativeExchange binding end to end: a Binance account on the native host,
// pointed at a local HTTP stand-in through overrideProxyUrl, signing through a JS
// remote signer that holds the secret. Runs in Bun (CI) or Node. Only private-client
// methods are called: the no-proxy public client ignores overrideProxyUrl (as in TS).
import assert from 'node:assert/strict';
import { createHmac } from 'node:crypto';
import { createServer } from 'node:http';
import { createRequire } from 'node:module';

const require = createRequire(import.meta.url);
const native = require('../../dist/node/safe_cex_ffi.node');
const SECRET = 'smoke-secret';
const hmac = (text) => createHmac('sha256', SECRET).update(text).digest('hex');

const seen = [];
const routes = {
  'GET /fapi/v3/account': { totalWalletBalance: '100', availableBalance: '60', totalUnrealizedProfit: '1', assets: [], positions: [] },
  // The actual source ORDER_TYPE has no STOP/TAKE_PROFIT entries, and
  // ORDER_SIDE has no UNKNOWN entry: the mapped own properties are undefined.
  'GET /fapi/v1/openOrders': [['LIMIT', 'BUY'], ['STOP', 'BUY'], ['TAKE_PROFIT', 'BUY'], ['LIMIT', 'UNKNOWN']].map(([type, side], index) => ({ orderId: 77 + index, clientOrderId: `c${77 + index}`, symbol: 'BTCUSDT', side, type, origQty: '1', price: '10', stopPrice: '0', status: 'NEW', executedQty: '0', reduceOnly: false, positionSide: 'BOTH', time: 1 })),
  'GET /fapi/v1/openAlgoOrders': [],
  'GET /fapi/v1/income': [],
};
const server = createServer((request, response) => {
  // overrideProxyUrl prefixes the venue base: /https://fapi.binance.com/fapi/v1/...
  const url = new URL(request.url.replace(/^\/https?:\/\/[^/]+/, ''), 'http://local');
  const key = `${request.method} ${url.pathname}`;
  seen.push({ key, query: url.search.slice(1), apiKey: request.headers['x-mbx-apikey'] });
  const body = routes[key];
  response.writeHead(body ? 200 : 404, { 'content-type': 'application/json' });
  response.end(JSON.stringify(body ?? { code: -1, msg: `no route ${key}` }));
});
await new Promise((resolve) => server.listen(0, '127.0.0.1', resolve));
const origin = `http://127.0.0.1:${server.address().port}`;

const signed = [];
// The browser's side of remote signing: Binance's local signer over the config.
const sign = async (request) => {
  signed.push(request);
  const params = { ...(request.config.params ?? {}), ...(request.config.data ?? {}), timestamp: 1700000000000 };
  const query = new URLSearchParams(Object.entries(params).map(([k, v]) => [k, String(v)])).toString();
  return { id: request.id, config: { params: { ...params, signature: hmac(query) }, headers: request.config.headers } };
};
const events = [];
const stores = [];
const id = await native.nativeAccountOpen(
  { exchange: 'binance', options: { key: 'smoke-key', secret: '', overrideProxyUrl: origin }, accountName: 'smoke-account' },
  (event) => events.push(event),
  (slices) => stores.push(slices),
  sign,
);

const orders = await native.nativeAccountCallMethod(id, 'fetchAndSetOrders', [null]);
assert.deepEqual(orders.map((order) => order.id), ['77', '78', '79', '80']);
assert.equal(orders[0].type, 'limit');
assert.equal(orders[0].side, 'buy');
for (const index of [1, 2]) {
  assert.equal(Object.hasOwn(orders[index], 'type'), true, 'undefined order type stays an own property');
  assert.equal(orders[index].type, undefined);
  assert.equal(orders[index].info.type, index === 1 ? 'STOP' : 'TAKE_PROFIT');
}
assert.equal(Object.hasOwn(orders[3], 'side'), true, 'undefined order side stays an own property');
assert.equal(orders[3].side, undefined);
for (const order of orders) {
  assert.equal(order.price, 10, 'present price stays present');
  assert.equal(Object.hasOwn(order, 'parentId'), false, 'absent optional property stays absent');
}
await new Promise((resolve) => setTimeout(resolve, 50));
assert.ok(stores.some((s) => s.orders.length === 4 && s.orders.some((o) => o.id === '78' && Object.hasOwn(o, 'type') && o.type === undefined)), 'the Store slice reached JS');

const validated = await native.nativeAccountCallMethod(id, 'validateAccount', []);
assert.equal(validated.balance.total, 100, 'validateAccount over the signed private client');
const account = seen.findLast((s) => s.key === 'GET /fapi/v3/account');
const [payload, signature] = account.query.split('&signature=');
assert.equal(signature, hmac(payload), 'server-verified HMAC from the JS remote signer');
assert.equal(account.apiKey, 'smoke-key');
const accountSign = signed.find((r) => r.config.url.endsWith('/fapi/v3/account'));
assert.equal(accountSign.type, 'http');
assert.equal(accountSign.accountName, 'smoke-account');

const history = await native.nativeAccountCallMethod(id, 'fetchHistoricalPositions', [{ account: 'smoke-account', from: 1700000000000, to: 1700000060000 }]);
assert.deepEqual(history, [], 'native journal method is available through the native account surface');
assert.ok(seen.some((s) => s.key === 'GET /fapi/v1/income'), 'journal reads reach the native signed income pager');
assert.ok(seen.every((s) => s.key in routes), `every request had a local route: ${JSON.stringify(seen)}`);

await assert.rejects(native.nativeAccountCallMethod(id, 'noSuchMethod', []), /Unknown method noSuchMethod/);
await native.nativeAccountClose(id);
await assert.doesNotReject(native.nativeAccountClose(id));
await assert.rejects(native.nativeAccountCallMethod(id, 'fetchMarkets', []), { name: 'AccountClosedError', code: 'ACCOUNT_CLOSED' });
server.close();
console.log(`native exchange smoke passed: ${signed.length} remote signatures, ${stores.length} store slices, ${events.length} events`);
