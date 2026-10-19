import assert from 'node:assert/strict';
import { mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { spawnSync } from 'node:child_process';
import { gunzipSync } from 'node:zlib';
import test from 'node:test';
import { callerIdsOf, canonicalUrl, isWriteRequest, requestKey, selectResponse, subscriptionKey, subscriptionKeys } from './requests.mjs';

const info = 'https://api.hyperliquid.xyz/info';
const exchange = 'https://api.hyperliquid.xyz/exchange';
const cloid = (n) => `0x${String(n).padStart(32, '0')}`;
const callerIds = new Set([cloid(1), cloid(2), 'caller-a', 'caller-b']);

test('untimed HTTP replies retain consume-until-last queue semantics', () => {
  const queue = [{ body: 'first' }, { body: 'last' }];
  assert.equal(selectResponse(queue, 0).body, 'first');
  assert.equal(selectResponse(queue, 1000).body, 'last');
  assert.equal(selectResponse(queue, 2000).body, 'last');
  assert.equal(queue.length, 1);
  assert.equal(selectResponse([], 0), undefined);
  assert.equal(selectResponse(undefined, 0), undefined);
});

test('timed HTTP replies follow recorded state rather than polling frequency', () => {
  const startup = { availableAt: 0, body: '[]' };
  const placed = { availableAt: 500, body: '[{"oid":42}]' };
  const cancelled = { availableAt: 1500, body: '[]' };
  // Selection follows availability, even when records are not time-sorted.
  const queue = [startup, cancelled, placed];
  assert.equal(selectResponse(queue, 0), startup);
  for (let i = 0; i < 20; i++) assert.equal(selectResponse(queue, 499), startup);
  assert.equal(selectResponse(queue, 500), placed);
  for (let i = 0; i < 20; i++) assert.equal(selectResponse(queue, 1000), placed);
  assert.equal(selectResponse(queue, 1499), placed);
  assert.equal(selectResponse(queue, 1500), cancelled);
  assert.equal(selectResponse(queue, 9000), cancelled);
  assert.deepEqual(queue, [startup, cancelled, placed]);
});

test('timed HTTP replies never expose future state and treat unstamped startup state as time zero', () => {
  const future = { availableAt: 1000, body: 'later' };
  assert.equal(selectResponse([future], 0), undefined);
  const startup = { body: 'initial' };
  const first = { availableAt: 1000, body: 'first at threshold' };
  const queue = [startup, first, future];
  assert.equal(selectResponse(queue, 0), startup);
  assert.equal(selectResponse(queue, 999), startup);
  assert.equal(selectResponse(queue, 1000), future);
  assert.equal(queue.length, 3);
});

test('one write acknowledgement cannot satisfy a duplicate economic action', () => {
  const acknowledgement = { body: 'accepted' };
  const queue = [acknowledgement];
  assert.equal(selectResponse(queue, 0, { write: true }), acknowledgement);
  assert.equal(selectResponse(queue, 0, { write: true }), undefined);
  const twoAttempts = [{ body: 'first' }, { body: 'second' }];
  assert.equal(selectResponse(twoAttempts, 0, { write: true }).body, 'first');
  assert.equal(selectResponse(twoAttempts, 0, { write: true }).body, 'second');
  assert.equal(twoAttempts.length, 0);
});

test('POST info reads repeat while trading writes consume; other POSTs remain writes', () => {
  for (const url of [info, 'https://api.hyperliquid-testnet.xyz/info']) {
    assert.equal(isWriteRequest('POST', url), false);
    const queue = [{ body: 'snapshot' }];
    for (let i = 0; i < 10; i++) assert.equal(selectResponse(queue, 0, { write: isWriteRequest('POST', url) }).body, 'snapshot');
    assert.equal(queue.length, 1);
  }
  assert.equal(isWriteRequest('POST', exchange), true);
  assert.equal(isWriteRequest('POST', 'https://www.okx.com/api/v5/trade/order'), true);
  assert.equal(isWriteRequest('POST', 'https://other.example/info'), true);
  assert.equal(isWriteRequest('GET', exchange), false);
});

test('staged reads and writes cannot consume future command acknowledgements', () => {
  const initial = { body: 'initial' };
  const changed = { afterCommand: 1, body: 'changed' };
  const reads = [initial, changed];
  assert.equal(selectResponse(reads, 0), initial);
  assert.equal(selectResponse(reads, 0, { commandIndex: 1 }), changed);
  assert.equal(selectResponse(reads, 0, { commandIndex: 1 }), changed);
  const writes = [{ afterCommand: 2, availableAt: 100, body: 'accepted' }];
  assert.equal(selectResponse(writes, 100, { write: true, commandIndex: 1 }), undefined);
  assert.equal(selectResponse(writes, 99, { write: true, commandIndex: 2 }), undefined);
  assert.equal(writes.length, 1);
  assert.equal(selectResponse(writes, 100, { write: true, commandIndex: 2 }).body, 'accepted');
  assert.equal(writes.length, 0);
});

test('OKX generated broker IDs normalize while broker-prefixed caller IDs remain exact', () => {
  const url = 'https://www.okx.com/api/v5/trade/order';
  const generated = 'e847386590ce4dBC0000000000000000';
  const caller = 'e847386590ce4dBC0000000000000001';
  const options = { generatedBodyId: (key, value) => ['clOrdId', 'algoClOrdId'].includes(key) && value.startsWith('e847386590ce4dBC') };
  const ids = new Set([caller]);
  const key = (id) => requestKey('POST', url, { instId: 'BTC-USDT-SWAP', clOrdId: id }, ids, options);
  assert.equal(key(generated), key('e847386590ce4dBC0000000000000002'));
  assert.notEqual(key(generated), key(caller));
  assert.notEqual(key(generated), key('other-exact-id'));
});

test('OKX login identity normalizes validated clocks/signatures and keeps passphrase', () => {
  const url = 'wss://ws.okx.com:8443/ws/v5/private';
  const login = (timestamp, sign, passphrase = 'passphrase') => ({ op: 'login', args: [{ apiKey: 'key', passphrase, timestamp, sign }] });
  assert.equal(subscriptionKey(url, login('1', 'a')), subscriptionKey(url, login('2', 'b')));
  assert.notEqual(subscriptionKey(url, login('1', 'a')), subscriptionKey(url, login('1', 'a', 'other')));
  assert.deepEqual(subscriptionKeys(url, { op: 'subscribe', args: [{ channel: 'orders', instType: 'SWAP' }, { channel: 'positions', instType: 'ANY' }] }), [
    ...subscriptionKeys(url, { op: 'subscribe', args: [{ channel: 'orders', instType: 'SWAP' }] }),
    ...subscriptionKeys(url, { op: 'subscribe', args: [{ channel: 'positions', instType: 'ANY' }] }),
  ]);
});

test('POST info identity includes every query field and ignores JSON key order', () => {
  const key = (body) => requestKey('post', info, body);
  assert.equal(key('{"user":"alice","type":"userFees"}'), key({ type: 'userFees', user: 'alice' }));
  for (const body of [{ type: 'userFees', user: 'bob' }, { type: 'userFunding', user: 'alice' },
    { type: 'userFees', user: 'alice', dex: 'xyz' }]) {
    assert.notEqual(key(body), key({ type: 'userFees', user: 'alice' }));
  }
  assert.notEqual(key({ type: 'userFunding', startTime: 1 }), key({ type: 'userFunding', startTime: 2 }));
});

test('Hyperliquid generated cloids/signatures normalize while caller IDs and action fields remain exact', () => {
  const body = (id, price = '10', nonce = 1) => ({
    action: { type: 'order', orders: [{ a: 0, p: price, c: id }], grouping: 'na' },
    nonce, signature: { r: `r${nonce}`, s: 's', v: 27 },
  });
  const key = (body) => requestKey('POST', exchange, body, callerIds);
  assert.equal(key(body(cloid(3))), key(body(cloid(4), '10', 2)));
  assert.notEqual(key(body(cloid(1))), key(body(cloid(2))));
  assert.notEqual(key(body(cloid(1))), key(body(cloid(3))));
  assert.notEqual(key(body(cloid(3))), key(body(cloid(3), '11')));
  assert.notEqual(key({ action: { nonce: 1 } }), key({ action: { nonce: 2 } }));
});

test('Binance URL and batch caller IDs survive normalization', () => {
  const key = (id, timestamp = 1) => requestKey('POST',
    `https://fapi.binance.com/fapi/v1/order?symbol=BTCUSDT&newClientOrderId=${id}&timestamp=${timestamp}&signature=s${timestamp}`,
    undefined, callerIds);
  assert.equal(key('generated-a'), key('generated-b', 2));
  assert.equal(key('generated-a'), requestKey('POST', 'https://fapi.binance.com/fapi/v1/order?symbol=BTCUSDT'));
  assert.notEqual(key('caller-a'), key('caller-b'));
  const batch = (id) => requestKey('POST', `https://fapi.binance.com/fapi/v1/batchOrders?batchOrders=${encodeURIComponent(JSON.stringify([
    { symbol: 'BTCUSDT', newClientOrderId: id, quantity: '1' },
  ]))}`, undefined, callerIds);
  assert.equal(batch('generated-a'), batch('generated-b'));
  assert.notEqual(batch('caller-a'), batch('caller-b'));
});

test('large numeric IDs remain distinct in URL and JSON bodies', () => {
  assert.notEqual(requestKey('GET', `${info}?oid=9007199254740992`), requestKey('GET', `${info}?oid=9007199254740993`));
  assert.notEqual(requestKey('POST', info, '{"oid":9007199254740992}'), requestKey('POST', info, '{"oid":9007199254740993}'));
});

test('form body identities normalize clocks and preserve caller IDs', () => {
  assert.equal(requestKey('POST', info, 'symbol=BTC&timestamp=1&signature=a'), requestKey('POST', info, new URLSearchParams('signature=b&timestamp=2&symbol=BTC')));
  assert.notEqual(requestKey('POST', info, 'clientOrderId=caller-a', callerIds), requestKey('POST', info, 'clientOrderId=caller-b', callerIds));
});

test('websocket identity ignores heartbeat and transport IDs but preserves subscriptions', () => {
  const socket = 'wss://api.hyperliquid.xyz/ws';
  assert.equal(subscriptionKey(socket, '{"method":"ping"}'), undefined);
  assert.equal(subscriptionKey(socket, '{"method":"SUBSCRIBE","id":1,"params":["a","b"]}'), subscriptionKey(socket, '{"params":["b","a"],"id":2,"method":"SUBSCRIBE"}'));
  assert.deepEqual(subscriptionKeys(socket, '{"method":"SUBSCRIBE","id":1,"params":["a","b"]}'), [
    ...subscriptionKeys(socket, '{"method":"SUBSCRIBE","id":2,"params":["a"]}'),
    ...subscriptionKeys(socket, '{"method":"SUBSCRIBE","id":3,"params":["b"]}'),
  ]);
  assert.notEqual(subscriptionKey(socket, '{"method":"subscribe","subscription":{"type":"l2Book","coin":"BTC"}}'), subscriptionKey(socket, '{"method":"subscribe","subscription":{"type":"bbo","coin":"BTC"}}'));
});

test('Bybit socket routing ignores cosmetic requestId while preserving other query fields', () => {
  const base = 'wss://stream.bybit.com/v5/public/linear';
  assert.equal(canonicalUrl(`${base}?requestId=recorded`), canonicalUrl(`${base}?requestId=live`));
  assert.equal(canonicalUrl(base), canonicalUrl(`${base}?requestId=live`));
  assert.notEqual(canonicalUrl(`${base}?max_active_time=1m`), canonicalUrl(`${base}?max_active_time=2m`));
});

test('Bybit auth expires/signatures normalize but the account and feed set stay exact', () => {
  const url = 'wss://stream.bybit.com/v5/private';
  const auth = (key, expires, signature) => ({ op: 'auth', args: [key, expires, signature] });
  assert.equal(subscriptionKey(url, auth('key-a', '100', 'a')), subscriptionKey(`${url}?requestId=generated`, auth('key-a', '200', 'b')));
  assert.notEqual(subscriptionKey(url, auth('key-a', '100', 'a')), subscriptionKey(url, auth('key-b', '100', 'a')));
  assert.deepEqual(subscriptionKeys(url, { op: 'subscribe', args: ['order', 'position'], req_id: 'a' }), [
    ...subscriptionKeys(url, { op: 'subscribe', args: ['order'], req_id: 'b' }),
    ...subscriptionKeys(url, { op: 'subscribe', args: ['position'], req_id: 'c' }),
  ]);
  assert.notEqual(subscriptionKey(url, { op: 'subscribe', args: ['order'] }), subscriptionKey(url, { op: 'subscribe', args: ['wallet'] }));
});

test('trim keeps body-specific replies, caller cloids and untimed config', () => {
  const dir = mkdtempSync(join(tmpdir(), 'rust-harness-'));
  try {
    const tape = join(dir, 'input.jsonl');
    const out = join(dir, 'output.jsonl.gz');
    const records = [{ type: 'config', private: true }, ...[1, 2, 3, 4].flatMap((n) => [
      { t: n, type: 'command', orders: [{ clientOrderId: cloid(n) }] },
      { t: n, type: 'http', method: 'POST', url: exchange,
        requestBody: JSON.stringify({ action: { type: 'order', orders: [{ c: cloid(n) }] }, nonce: n }), status: 200, body: '[]' },
    ]), ...[1, 2, 3, 4].map((n) => ({ t: n, type: 'http', method: 'POST', url: info, requestBody: JSON.stringify({ type: `query${n}` }), status: 200, body: '[]' }))];
    assert.equal(callerIdsOf(records).size, 4);
    writeFileSync(tape, records.map((record) => JSON.stringify(record)).join('\n'));
    const result = spawnSync('corepack', ['yarn', '-s', 'rust:trim-tape', '--tape', tape, '--seconds', '1', '--out', out], { encoding: 'utf8' });
    assert.equal(result.status, 0, result.stderr);
    const kept = gunzipSync(readFileSync(out)).toString().trim().split('\n').map((line) => JSON.parse(line));
    assert.equal(kept[0].type, 'config');
    assert.equal(kept.filter((record) => record.type === 'http').length, 8);
  } finally {
    rmSync(dir, { recursive: true, force: true });
  }
});


test('Crypto.com envelope projection keeps exact caller IDs and unsafe nanosecond parameters', () => {
  const url = 'https://api.crypto.com/exchange/v1/private/get-trades';
  const options = { requestBody: (body) => body.params };
  const key = (end, caller = 'caller-a', nonce = 1) => requestKey('POST', url,
    `{"id":${nonce},"method":"private/get-trades","sig":"generated","params":{"end_time":${end},"client_oid":"${caller}"},"nonce":${nonce}}`, callerIds, options);
  assert.equal(key('1610000000000000001'), key('1610000000000000001', 'caller-a', 2));
  assert.notEqual(key('1610000000000000001'), key('1610000000000000002'));
  assert.notEqual(key('1610000000000000001'), key('1610000000000000001', 'caller-b'));
});

test('Crypto.com subscriptions compare channels across batches and normalize audited auth', () => {
  const url = 'wss://stream.crypto.com/exchange/v1/user';
  const options = { cryptoProtocol: true };
  const keys = (message) => subscriptionKeys(url, message, new Set(), options);
  assert.deepEqual(keys({ id: 1, nonce: 100, method: 'subscribe', params: { channels: ['user.order', 'user.trade'] } }), [
    ...keys({ id: 2, nonce: 200, method: 'subscribe', params: { channels: ['user.order'] } }),
    ...keys({ id: 3, nonce: 300, method: 'subscribe', params: { channels: ['user.trade'] } }),
  ]);
  const auth = (api_key, nonce) => ({ id: nonce, nonce, method: 'public/auth', api_key, sig: String(nonce) });
  assert.deepEqual(keys(auth('key-a', 1)), keys(auth('key-a', 2)));
  assert.notDeepEqual(keys(auth('key-a', 1)), keys(auth('key-b', 1)));
  assert.deepEqual(keys({ id: 1, method: 'public/respond-heartbeat' }), []);
});
