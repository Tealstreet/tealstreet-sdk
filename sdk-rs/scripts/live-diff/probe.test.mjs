import assert from 'node:assert/strict';
import { EventEmitter } from 'node:events';
import { mkdtempSync, rmSync, readFileSync } from 'node:fs';
import http from 'node:http';
import https from 'node:https';
import net from 'node:net';
import { tmpdir } from 'node:os';
import { test } from 'node:test';
import tls from 'node:tls';
import { byteDifference, differences, outcome } from './compare.mjs';
import { makeCA, recordingProxy, readKey, isRead } from './recorder.mjs';

test('diff retains SDK value distinctions and exact order bytes', () => {
  assert.equal(differences({ value: undefined }, {}).length, 1);
  assert.equal(differences(NaN, null).length, 1);
  assert.equal(differences(-0, 0).length, 1);
  assert.deepEqual(differences({ a: NaN, b: undefined }, { b: undefined, a: NaN }), []);
  assert.equal(byteDifference('{"a":1,"b":2}', '{"b":2,"a":1}').offset, 2);
  assert.equal(byteDifference('é', 'è').offset, 1);
  assert.equal(byteDifference('same', 'same'), null);
});

test('identical empty data, exceptions and HTTP failures cannot pass parity', () => {
  const empty = { value: [], errors: 0 };
  assert.equal(outcome('fetchMarkets', empty, empty, []).status, 'BLOCKED');
  const error = { failure: { name: 'Error', message: 'Not implemented' } };
  assert.equal(outcome('fetchMarkets', error, error, []).status, 'BLOCKED');
  const valid = { value: [{ symbol: 'BTC' }], errors: 0 };
  assert.equal(outcome('fetchTickers', valid, error, []).status, 'DIFF', 'one-sided native rejection is a parity bug');
  assert.equal(outcome('fetchMarkets', valid, valid, [{ status: 403 }]).status, 'BLOCKED');
  assert.equal(
    outcome('fetchMarkets', valid, valid, [{ implementation: 'rust', read: true, cacheHit: false }]).status,
    'INPUT_DIFF',
  );
  assert.equal(
    outcome('fetchMarkets', valid, valid, [{ implementation: 'rust', read: true, cacheHit: true }]).status,
    'PASS',
  );
  assert.equal(outcome('fetchOrders', { value: [], errors: 1 }, empty, []).status, 'BLOCKED');
  const fallback = [{ status: 422, expectedFallback: true }];
  assert.equal(outcome('$startPublic', { value: null }, { value: null }, fallback).status, 'PASS');
  assert.equal(outcome('fetchMarkets', valid, valid, fallback).status, 'BLOCKED');
  const optionalAster = ['ts', 'rust'].map((implementation) => ({
    implementation,
    host: 'fapi.asterdex.com',
    status: 401,
    expectedFallback: true,
  }));
  const degraded = { ...valid, errors: 1 };
  assert.equal(outcome('fetchMarkets', degraded, degraded, optionalAster).status, 'PASS');
  assert.equal(outcome('fetchOrders', degraded, degraded, optionalAster).status, 'BLOCKED');
  assert.equal(outcome('fetchMarkets', { ...degraded, errors: 2 }, degraded, optionalAster).status, 'BLOCKED');
});

test('replay key drops auth freshness/correlation, retaining business inputs', () => {
  const a = new URL('https://api.bybit.com/v5/market/tickers?category=linear&requestId=one');
  const b = new URL('https://api.bybit.com/v5/market/tickers?requestId=two&category=linear');
  assert.equal(readKey('GET', a, ''), readKey('GET', b, ''));
  b.searchParams.set('category', 'inverse');
  assert.notEqual(readKey('GET', a, ''), readKey('GET', b, ''));
  assert.equal(isRead('POST', new URL('https://api.hyperliquid.xyz/info'), '{}'), true);
  assert.equal(isRead('POST', new URL('https://api.hyperliquid.xyz/exchange'), '{}'), false);
  assert.equal(isRead('POST', new URL('https://clob.polymarket.com/midpoints'), '[]'), true);
  assert.equal(isRead('POST', new URL('https://clob.polymarket.com/order'), '{}'), false);
});

test('terminal recorder replays one live response, blocks writes, verifies TLS SAN', async (t) => {
  const directory = mkdtempSync(`${tmpdir()}/live-diff-test-`);
  const ca = makeCA(directory),
    records = [],
    cache = new Map();
  let sends = 0;
  const forward = (_url, _options, receive) => {
    sends++;
    const request = new EventEmitter();
    request.end = () =>
      queueMicrotask(() => {
        const response = new EventEmitter();
        response.statusCode = 200;
        response.headers = { 'content-type': 'application/json' };
        receive(response);
        response.emit('data', Buffer.from('{"price":"42"}'));
        response.emit('end');
      });
    return request;
  };
  const proxy = await recordingProxy({
    directory,
    ca,
    cache,
    records,
    implementation: 'ts',
    allowHost: (host) => host === 'api.bybit.com',
    forward,
  });
  t.after(async () => {
    await proxy.close();
    rmSync(directory, { recursive: true, force: true });
  });
  const get = (path, method = 'GET') =>
    new Promise((resolve, reject) => {
      const request = http.request(proxy.url, { path, method }, (response) => {
        let body = '';
        response.on('data', (part) => (body += part));
        response.on('end', () => resolve({ status: response.statusCode, body }));
      });
      request.on('error', reject);
      request.end();
    });
  assert.equal(
    (await get('https://api.bybit.com/v5/market/tickers?category=linear&requestId=one')).body,
    '{"price":"42"}',
  );
  assert.equal(
    (await get('https://api.bybit.com/v5/market/tickers?requestId=two&category=linear')).body,
    '{"price":"42"}',
  );
  assert.equal(sends, 1, 'the same response bytes reach both correlation IDs');
  assert.equal((await get('https://api.bybit.com/v5/order/create', 'POST')).status, 403);
  assert.equal(sends, 1, 'a blocked order never reaches upstream');
  proxy.writes(true);
  assert.equal(
    (await get('https://api.bybit.com/v5/order/create', 'POST')).status,
    403,
    'unimplemented venue writes stay blocked',
  );
  const endpoint = new URL(proxy.url);
  const socket = net.connect(Number(endpoint.port), endpoint.hostname);
  await new Promise((resolve, reject) => {
    socket.once('connect', resolve);
    socket.once('error', reject);
  });
  socket.write('CONNECT api.bybit.com:443 HTTP/1.1\r\nHost: api.bybit.com:443\r\n\r\n');
  await new Promise((resolve) => socket.once('data', resolve));
  const secure = tls.connect({ socket, servername: 'api.bybit.com', ca: readFileSync(ca.cert) });
  await new Promise((resolve, reject) => {
    secure.once('secureConnect', resolve);
    secure.once('error', reject);
  });
  assert.equal(secure.authorized, true, 'ephemeral CA is trusted and exact hostname SAN validates');
  const agent = new https.Agent();
  agent.createConnection = () => secure;
  const reply = await new Promise((resolve, reject) => {
    const request = https.request(
      'https://api.bybit.com/v5/market/tickers?category=linear&requestId=three',
      { agent },
      (response) => {
        let body = '';
        response.on('data', (part) => (body += part));
        response.on('end', () => resolve(body));
      },
    );
    request.on('error', reject);
    request.end();
  });
  assert.equal(reply, '{"price":"42"}');
  assert.equal(sends, 1);
  agent.destroy();
  secure.destroy();
});

test('authenticated replay still validates both real requests and exposes signature rejection', async (t) => {
  let sends = 0;
  const records = [];
  const forward = (_url, _options, receive) => {
    const status = ++sends === 1 ? 200 : 401;
    const request = new EventEmitter();
    request.end = () =>
      queueMicrotask(() => {
        const response = new EventEmitter();
        response.statusCode = status;
        response.headers = { 'content-type': 'application/json' };
        receive(response);
        response.emit('data', Buffer.from(status === 200 ? '{"balance":42}' : '{"error":"invalid signature"}'));
        response.emit('end');
      });
    return request;
  };
  const proxy = await recordingProxy({
    records,
    cache: new Map(),
    implementation: 'rust',
    allowHost: (host) => host === 'fapi.binance.com',
    forward,
    validateLive: true,
  });
  t.after(() => proxy.close());
  const read = () =>
    new Promise((resolve, reject) => {
      const request = http.request(proxy.url, { path: 'https://fapi.binance.com/fapi/v3/account' }, (response) => {
        response.resume();
        response.on('end', () => resolve(response.statusCode));
      });
      request.on('error', reject);
      request.end();
    });
  assert.equal(await read(), 200);
  assert.equal(await read(), 401, 'cached TS success cannot hide a native auth failure');
  assert.equal(sends, 2, 'both signed requests must reach the real upstream');
  assert.equal(records[1].cacheHit, true);
  assert.equal(records[1].liveStatus, 401);
});
