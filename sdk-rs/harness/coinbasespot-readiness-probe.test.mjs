import assert from 'node:assert/strict';
import { test } from 'node:test';
import { readOnlyTransport } from './coinbasespot-readiness-probe.mjs';

test('all brokerage GET reads reach the transport while writes and foreign targets refuse', async () => {
  const calls = [];
  const logs = [];
  const transport = readOnlyTransport({ request: async (request) => { calls.push(request); return { status: 200 }; } }, { log: (line) => logs.push(line) });
  // The startup probe now admits every brokerage GET, including reads not in its former list.
  for (const path of ['portfolios', 'portfolios/default-one', 'products?limit=1000',
    'key_permissions', 'orders/historical/batch?order_status=OPEN', 'orders/historical/123',
    'market/products', 'time', 'accounts', 'transaction_summary', 'intx/positions/default-one']) {
    await transport.request({ method: 'GET', url: `https://api.coinbase.com/api/v3/brokerage/${path}` });
  }
  assert.equal(calls.length, 11);
  for (const [method, url, body] of [
    ['POST', 'https://api.coinbase.com/api/v3/brokerage/orders', '{}'],
    ['DELETE', 'https://api.coinbase.com/api/v3/brokerage/orders/123'],
    ['PUT', 'https://api.coinbase.com/api/v3/brokerage/orders/123', '{}'],
    ['POST', 'https://api.coinbase.com/api/v3/brokerage/orders/batch_cancel', '{}'],
    ['GET', 'https://example.com/api/v3/brokerage/portfolios'],
    ['GET', 'https://api.coinbase.com/api/v3/brokerage/portfolios', '{}'],
    ['GET', 'https://api.coinbase.com/api/v3/brokerage/portfolios', ''],
    ['GET', 'http://api.coinbase.com/api/v3/brokerage/portfolios'],
    ['GET', 'https://api.coinbase.com/api/v3/time'],
    ['GET', 'https://private:private@api.coinbase.com/api/v3/brokerage/portfolios'],
    ['GET', 'https://api.coinbase.com/api/v3/brokerage/portfolios#private'],
  ]) {
    await assert.rejects(transport.request({ method, url, body }), /Read-only probe refused/);
  }
  assert.equal(calls.length, 11);
  assert.equal(logs.filter((line) => line.startsWith('REFUSED')).length, 11);
  assert.equal(logs.filter((line) => line.startsWith('COMPLETED')).length, 11);
  assert.ok(logs.every((line) => !/[?#@]/.test(line) && !line.includes('private')));
});

test('proxy routing preserves the signed request and logs only original path/status', async () => {
  const logs = [];
  const calls = [];
  const response = { status: 403, body: 'private-response', headers: { private: 'response-header' } };
  const transport = readOnlyTransport({ request: async (request) => { calls.push(request); return response; } }, {
    proxyPrefix: 'https://proxies.tyo.tealstreet.io/api/exchange_proxy/', log: (line) => logs.push(line),
  });
  const request = { method: 'GET', url: 'https://api.coinbase.com/api/v3/brokerage/accounts?cursor=private-query',
    headers: { Authorization: 'private-header' }, signal: new AbortController().signal };
  assert.equal(await transport.request(request), response);
  assert.equal(calls[0].url, `https://proxies.tyo.tealstreet.io/api/exchange_proxy/${request.url}`);
  assert.equal(calls[0].headers, request.headers);
  assert.equal(calls[0].signal, request.signal);
  assert.equal(request.url, 'https://api.coinbase.com/api/v3/brokerage/accounts?cursor=private-query');
  await assert.rejects(transport.request({ ...request, method: 'POST' }), /Read-only probe refused/);
  assert.equal(calls.length, 1);
  assert.deepEqual(logs, ['COMPLETED 403 /api/v3/brokerage/accounts', 'REFUSED POST /api/v3/brokerage/accounts']);
  for (const prefix of ['http://example.com/', 'https://private@example.com/', 'https://example.com/?private=query'])
    assert.throws(() => readOnlyTransport({}, { proxyPrefix: prefix }), /HTTPS proxy prefix/);
});

test('transport exceptions and malformed URLs never disclose raw error or query text', async () => {
  const logs = [];
  const transport = readOnlyTransport({ request: async () => { throw new Error('private-transport-exception'); } }, { log: (line) => logs.push(line) });
  await assert.rejects(transport.request({ method: 'GET', url: 'https://api.coinbase.com/api/v3/brokerage/products?private-query' }), /^Error: Read-only probe transport failed$/);
  await assert.rejects(transport.request({ method: 'GET', url: 'invalid-private-url' }), /Read-only probe refused/);
  assert.deepEqual(logs, ['FAILED GET /api/v3/brokerage/products', 'REFUSED GET [invalid-path]']);
});

test('only Coinbase heartbeat/user subscriptions reach the socket', () => {
  const sends = [];
  const transport = readOnlyTransport({ connect: () => ({ send: (text) => sends.push(text), close() {} }) });
  assert.throws(() => transport.connect('wss://example.com', {}), /Read-only probe refused/);
  for (const url of ['wss://advanced-trade-ws.coinbase.com', 'wss://advanced-trade-ws-user.coinbase.com']) {
    const connection = transport.connect(url, {});
    for (const channel of ['heartbeats', 'user']) connection.send(JSON.stringify({ type: 'subscribe', channel }));
    assert.throws(() => connection.send(JSON.stringify({ type: 'placeOrder', channel: 'user' })), /Read-only probe refused/);
    assert.throws(() => connection.send(JSON.stringify({ type: 'subscribe', channel: 'orders' })), /Read-only probe refused/);
  }
  assert.equal(sends.length, 4);
});
