import assert from 'node:assert/strict';
import { mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { spawnSync } from 'node:child_process';
import { gunzipSync } from 'node:zlib';
import test from 'node:test';
import { callerIdsOf, requestKey, subscriptionKey, subscriptionKeys } from './requests.mjs';

const info = 'https://api.hyperliquid.xyz/info';
const exchange = 'https://api.hyperliquid.xyz/exchange';
const cloid = (n) => `0x${String(n).padStart(32, '0')}`;
const callerIds = new Set([cloid(1), cloid(2), 'caller-a', 'caller-b']);

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
