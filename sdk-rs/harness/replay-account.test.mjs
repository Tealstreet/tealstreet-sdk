import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import test from 'node:test';
import vm from 'node:vm';

const source = readFileSync(new URL('./replay.mjs', import.meta.url), 'utf8');

test('Rust account comparison excludes scalar placeholders and retains independent leverage rows', () => {
  const accountSource = source.slice(source.indexOf('function rustAccount('), source.indexOf('\nasync function runTs()'));
  const account = (venue, settings) => vm.runInNewContext(`${accountSource}\nrustAccount`, {
    venue, values: { venue: venue.rust },
  })({
    markets: { symbol: (index) => `MARKET${index}` },
    balance: { used: 1, free: 9, total: 10, upnl: 0 },
    settings, fills: [],
  });
  const settings = {
    count: 7,
    market: [0, 1, 2, 3, 4, 5, 6],
    leverage: [0, NaN, -Infinity, undefined, -2, 5, 20],
    marginType: (index) => index % 2 ? 'isolated' : 'cross',
  };
  assert.deepEqual(structuredClone(account({ rust: 'breakoutprop' }, settings).settings), [
    { symbol: 'MARKET5', marginType: 'isolated', leverage: 5 },
    { symbol: 'MARKET6', marginType: 'cross', leverage: 20 },
  ]);
  assert.deepEqual(structuredClone(account({ rust: 'mexc', independentLeverage: true }, {
    count: 2, market: [0, 1], leverage: [0, NaN],
    buyLeverage: [5, 10], sellLeverage: [15, 20], marginType: () => 'cross',
  }).settings), [
    { symbol: 'MARKET0', marginType: 'cross', buyLeverage: 5, sellLeverage: 15 },
    { symbol: 'MARKET1', marginType: 'cross', buyLeverage: 10, sellLeverage: 20 },
  ]);
});

test('ordinary TS venue fetch always uses the tape and preserves Request and response metadata', async () => {
  const fetchSource = source.slice(source.indexOf('  globalThis.WebSocket = TapeWebSocket;'),
    source.indexOf('  // Header-capable Node sockets'));
  const calls = [];
  let networkCalls = 0;
  const context = {
    venue: { rust: 'grvt' }, values: { venue: 'grvt' }, TapeWebSocket: class {}, Request, Response, Headers,
    globalThis: { fetch: () => { networkCalls++; throw new Error('network must remain unused'); } },
    tape: { respond: (...args) => {
      calls.push(args);
      return calls.length === 1
        ? { status: 201, body: '{"from":"tape"}', headers: 'content-type: application/json\nx-replay: exact:with:colons' }
        : { status: 204, body: 'discarded for a bodyless status', headers: 'x-replay: empty' };
    } },
  };
  vm.runInNewContext(fetchSource, context);
  const request = new Request('https://edge.grvt.io/full/v1/account?scope=positions', {
    method: 'POST', body: '{"account":"caller"}',
    headers: { 'Content-Type': 'application/json', 'X-Auth': 'fixture-signature' },
  });
  const response = await context.globalThis.fetch(request);
  assert.equal(response.status, 201);
  assert.equal(response.headers.get('x-replay'), 'exact:with:colons');
  assert.equal(response.headers.get('content-type'), 'application/json');
  assert.deepEqual(await response.json(), { from: 'tape' });
  assert.equal(request.bodyUsed, false, 'reading a Request for replay must clone its body');
  const empty = await context.globalThis.fetch(request, {
    method: 'PUT', body: 'override-body', headers: { 'X-Override': 'explicit' },
  });
  assert.equal(empty.status, 204);
  assert.equal(empty.body, null);
  assert.equal(empty.headers.get('x-replay'), 'empty');
  assert.equal(await empty.text(), '');
  assert.deepEqual(structuredClone(calls), [
    ['POST', request.url, '{"account":"caller"}', {
      'content-type': 'application/json', 'x-auth': 'fixture-signature',
    }],
    ['PUT', request.url, 'override-body', { 'x-override': 'explicit' }],
  ]);
  assert.equal(networkCalls, 0);
});
