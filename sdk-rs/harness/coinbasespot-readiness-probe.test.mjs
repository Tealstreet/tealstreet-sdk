import assert from 'node:assert/strict';
import { test } from 'node:test';
import { readOnlyTransport } from './coinbasespot-readiness-probe.mjs';

test('only the closed Coinbase GET reads reach the actual transport', async () => {
  const calls = [];
  const transport = readOnlyTransport({ request: async (request) => { calls.push(request); return { status: 200 }; } });
  for (const path of ['portfolios', 'portfolios/default-one', 'products?limit=1000',
    'key_permissions', 'orders/historical/batch?order_status=OPEN', 'orders/historical/123']) {
    await transport.request({ method: 'GET', url: `https://api.coinbase.com/api/v3/brokerage/${path}` });
  }
  assert.equal(calls.length, 6);
  for (const [method, url, body] of [
    ['POST', 'https://api.coinbase.com/api/v3/brokerage/orders', '{}'],
    ['DELETE', 'https://api.coinbase.com/api/v3/brokerage/orders/123'],
    ['PUT', 'https://api.coinbase.com/api/v3/brokerage/orders/123', '{}'],
    ['GET', 'https://api.coinbase.com/api/v3/brokerage/orders/batch_cancel'],
    ['GET', 'https://api.coinbase.com/api/v3/brokerage/intx/positions/default-one'],
    ['GET', 'https://example.com/api/v3/brokerage/portfolios'],
    ['GET', 'https://api.coinbase.com/api/v3/brokerage/portfolios', '{}'],
  ]) {
    await assert.rejects(transport.request({ method, url, body }), /Read-only probe refused/);
  }
  assert.equal(calls.length, 6);
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
