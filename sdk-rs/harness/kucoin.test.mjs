import { test } from 'node:test';
import assert from 'node:assert/strict';
import { createHmac } from 'node:crypto';
import { kucoin } from './kucoin.mjs';
const hmac = (text) => createHmac('sha256', 'replay-secret').update(text).digest('base64');
const sign = (method, path, body = '') => {
  const timestamp = String(Date.now());
  return { 'KC-API-KEY': 'replay-key', 'KC-API-TIMESTAMP': timestamp, 'KC-API-KEY-VERSION': '2', 'KC-API-PASSPHRASE': hmac('replay-passphrase'), 'KC-API-SIGN': hmac(timestamp + method + path + body) };
};
test('KuCoin signature audit binds raw query, body, timestamp and signed passphrase', () => {
  const path = '/api/v1/orders?clientOid=a b', url = 'https://api-futures.kucoin.com/api/v1/orders?clientOid=a%20b';
  const headers = sign('GET', path);
  assert.equal(kucoin.checkRequest('GET', url, undefined, headers), true);
  assert.equal(kucoin.checkRequest('DELETE', url, undefined, headers), false);
  assert.equal(kucoin.checkRequest('GET', url, undefined, { ...headers, 'KC-API-PASSPHRASE': 'replay-passphrase' }), false);
  assert.equal(kucoin.checkRequest('GET', url.replace('a%20b', 'ab'), undefined, headers), false);
  const body = '{"size":"2"}', post = sign('POST', '/api/v1/orders', body);
  assert.equal(kucoin.checkRequest('POST', 'https://api-futures.kucoin.com/api/v1/orders', body, post), true);
  assert.equal(kucoin.checkRequest('POST', 'https://api-futures.kucoin.com/api/v1/orders', '{"size":"3"}', post), false);
  assert.equal(kucoin.checkRequest('POST', 'https://api-futures.kucoin.com/api/v1/bullet-private', undefined, {}), false);
  assert.equal(kucoin.isWrite('POST', 'https://api-futures.kucoin.com/api/v1/bullet-private'), false);
});
const endpoint = 'wss://example.test/socket';
const records = ['public', 'private'].map((kind) => ({ type: 'http', url: `https://api-futures.kucoin.com/api/v1/bullet-${kind}`, body: JSON.stringify({ code: '200000', data: { token: kind + '-token', instanceServers: [{ endpoint }] } }) }));
test('KuCoin token audit validates endpoint, private topics and no follow-up login', () => {
  const audit = kucoin.createSocketAudit('rust', { isPrivate: true, books: [], records });
  const url = endpoint + '?token=private-token&connectId=123';
  audit.received(url, '{"type":"welcome"}');
  for (const topic of ['/contractMarket/tradeOrders', '/contractMarket/advancedOrders', '/contract/positionAll', '/contractAccount/wallet', '/contract/marginMode', '/contract/crossLeverage']) audit.sent(url, JSON.stringify({ type: 'subscribe', topic, privateChannel: true }));
  assert.equal(audit.ready(), true);
  assert.deepEqual(audit.errors(), []);
  audit.sent(url, '{"type":"login"}');
  assert(audit.errors().some((e) => e.includes('authentication')));
});
test('KuCoin rejects wrong token, unsubscribed frames and session topic overflow', () => {
  const audit = kucoin.createSocketAudit('rust', { isPrivate: false, books: [], records });
  const url = endpoint + '?token=public-token&connectId=123';
  audit.received(url, '{"type":"welcome"}');
  audit.received(url, '{"type":"message","topic":"/contractMarket/level2:XBTUSDTM"}');
  for (let i = 0; i < 5; i++) audit.sent(url, JSON.stringify({ type: 'subscribe', topic: '/contractMarket/snapshot:' + Array.from({ length: 100 }, (_, j) => `S${i * 100 + j}`).join(','), privateChannel: false }));
  audit.sent(endpoint + '?token=bad&connectId=123', '{"type":"ping","id":1}');
  assert(audit.errors().some((e) => e.includes('unsubscribed')));
  assert(audit.errors().some((e) => e.includes('400')));
  assert(audit.errors().some((e) => e.includes('bullet token')));
});

test('KuCoin readiness requires active bid/ask and mark/index topics for each book', () => {
  const audit = kucoin.createSocketAudit('rust', { isPrivate: false, books: ['BTCUSDT'], records });
  const url = endpoint + '?token=public-token&connectId=123';
  audit.received(url, '{"type":"welcome"}');
  const send = (topic) => audit.sent(url, JSON.stringify({ type: 'subscribe', topic, privateChannel: false }));
  send('/contractMarket/level2:XBTUSDTM');
  assert.equal(audit.ready(), false);
  send('/contractMarket/tickerV2:XBTUSDTM');
  assert.equal(audit.ready(), false);
  send('/contract/instrument:XBTUSDTM');
  assert.equal(audit.ready(), true);
});

test('KuCoin public replay waits for reads queued before the rate limiter', async () => {
  let release;
  const exchange = { xhr: { get: () => new Promise((resolve) => { release = resolve; }) }, cancelSymbolOrders: async () => {} };
  kucoin.configureTs(exchange);
  const read = exchange.xhr.get('/api/v1/level2/snapshot');
  let settled = false;
  const settling = kucoin.settleTs(exchange, { isPrivate: false }).then(() => { settled = true; });
  await new Promise((resolve) => setTimeout(resolve, 10));
  assert.equal(settled, false);
  release({ data: { code: '200000' } });
  await Promise.all([read, settling]);
  assert.equal(settled, true);
});


test('KuCoin queued recovery drains under the deterministic replay clock', async () => {
  const { ReplayClock } = await import('./tape.mjs');
  const clock = new ReplayClock(1700000000000, true);
  clock.install();
  try {
    const exchange = { xhr: { get: () => new Promise((resolve) => setTimeout(resolve, 20)) }, cancelSymbolOrders: async () => {} };
    kucoin.configureTs(exchange);
    const read = exchange.xhr.get('/api/v1/level2/snapshot');
    clock.start();
    await kucoin.settleTs(exchange, { isPrivate: false, clock });
    await read;
    assert(clock.time >= clock.epoch + 20);
  } finally { clock.restore(); }
});


test('stalled queued reads leave the real replay deadline runnable', async () => {
  const { ReplayClock } = await import('./tape.mjs');
  const { setTimeout: realTimeout } = await import('node:timers');
  const clock = new ReplayClock(1700000000000);
  clock.install();
  let release;
  let settling;
  try {
    const exchange = { xhr: { get: () => new Promise((resolve) => { release = resolve; }) }, cancelSymbolOrders: async () => {} };
    kucoin.configureTs(exchange);
    exchange.xhr.get('/api/v1/level2/snapshot');
    clock.start();
    settling = kucoin.settleTs(exchange, { isPrivate: false, clock });
    const result = await Promise.race([settling.then(() => 'settled'), new Promise((resolve) => realTimeout(() => resolve('deadline'), 15))]);
    assert.equal(result, 'deadline');
  } finally {
    release?.();
    await settling;
    clock.restore();
  }
});

test('Rust KuCoin subscription audit rejects sends before recorded welcome', () => {
  const audit = kucoin.createSocketAudit('rust', { isPrivate: false, books: [], records });
  const url = endpoint + '?token=public-token&connectId=123';
  audit.sent(url, '{"type":"subscribe","topic":"/contractMarket/snapshot:XBTUSDTM","privateChannel":false}');
  assert(audit.errors().some((error) => error.includes('before welcome')));
});


const contractFixture = async () => {
  const { mkdtempSync, readFileSync, rmSync } = await import('node:fs');
  const { tmpdir } = await import('node:os');
  const { join } = await import('node:path');
  const { spawnSync } = await import('node:child_process');
  const dir = mkdtempSync(join(tmpdir(), 'kucoin-contracts-'));
  try {
    const file = join(dir, 'fixture.jsonl');
    const generated = spawnSync(process.execPath, [new URL('./private-tape-kucoin.mjs', import.meta.url).pathname, '--out', file], { encoding: 'utf8' });
    assert.equal(generated.status, 0, generated.stderr);
    return readFileSync(file, 'utf8').trim().split('\n').map(JSON.parse);
  } finally { rmSync(dir, { recursive: true, force: true }); }
};
const { applyKucoinRequestContracts } = await import('./kucoin-request-contracts.mjs');
const { requestKey } = await import('./requests.mjs');
const fixtureReads = (records) => {
  const nativePages = new Set(records.map((record) => record.sameReadAs).filter(Boolean));
  const keys = (selected) => [...new Set(selected.map((record) => requestKey('GET', record.url)))];
  const reads = records.filter((record) => record.type === 'http' && record.method === 'GET');
  return {
    ts: keys(reads.filter((record) => !record.nativeRead && !nativePages.has(record.url))),
    rust: keys(reads.filter((record) => !record.sameReadAs)),
  };
};

test('KuCoin read contracts use independent fixture declarations and preserve strict request equality', async () => {
  const records = await contractFixture();
  assert.equal(records.filter((record) => record.nativeRead).length, 7);
  const { ts, rust } = fixtureReads(records);
  const adjusted = applyKucoinRequestContracts(records, ts, rust);
  assert.deepEqual(adjusted.tsRequests, adjusted.rustRequests);
  const extra = requestKey('GET', 'https://api-futures.kucoin.com/api/v1/fills?wrong=true');
  assert.notDeepEqual(applyKucoinRequestContracts(records, ts, [...rust, extra]).rustRequests, adjusted.tsRequests);
  const write = requestKey('DELETE', 'https://api-futures.kucoin.com/api/v1/orders/bogus');
  assert.notDeepEqual(applyKucoinRequestContracts(records, ts, [...rust, write]).rustRequests, adjusted.tsRequests);
  for (const record of records.filter((record) => record.nativeRead)) {
    const missing = rust.filter((key) => key !== requestKey('GET', record.url));
    assert.throws(() => applyKucoinRequestContracts(records, ts, missing), /required native GET/);
    assert.throws(() => applyKucoinRequestContracts(records, ts, [...missing, requestKey('GET', record.url + '&wrong=1')]), /required native GET/);
  }
});

test('KuCoin read metadata rejects mutations, unknown reasons, mismatched command or query parameters', async () => {
  const original = await contractFixture();
  for (const mutate of [
    (record) => { record.method = 'DELETE'; },
    (record) => { record.nativeRead.reason = 'ignore'; },
    (record) => { record.nativeRead.command = 1; },
    (record) => { record.url += '&wrong=1'; },
    (record) => { record.url = record.url.replace('api-futures.kucoin.com', 'other.test'); },
    (record) => { record.requestBody = '{}'; },
    (record) => { record.nativeRead.extra = true; },
  ]) {
    const records = structuredClone(original);
    mutate(records.find((record) => record.nativeRead?.reason === 'query'));
    const { ts, rust } = fixtureReads(records);
    assert.throws(() => applyKucoinRequestContracts(records, ts, rust), /KuCoin read contract/);
  }
  for (const reason of ['query', 'cancel-proof', 'position-mode']) {
    const records = structuredClone(original);
    const row = records.find((record) => record.nativeRead?.reason === reason);
    row.afterCommand = 99;
    const { ts, rust } = fixtureReads(records);
    assert.throws(() => applyKucoinRequestContracts(records, ts, rust), /KuCoin read contract/);
  }
  const records = structuredClone(original);
  const command = records.filter((record) => record.type === 'command')[5];
  delete command.expected;
  const { ts, rust } = fixtureReads(records);
  assert.throws(() => applyKucoinRequestContracts(records, ts, rust), /independently expected/);
});

test('KuCoin first-page aliases require identical response, stage, origin, pagination and bounded census', async () => {
  const original = await contractFixture();
  for (const mutate of [
    (row) => { row.body = row.body.replace('old-order', 'different-order'); },
    (row) => { row.afterCommand = 99; },
    (row) => { row.status = 201; },
    (row) => { row.method = 'POST'; },
    (row) => { row.sameReadAs = row.sameReadAs.replace('pageSize=100', 'pageSize=50'); },
    (row) => { row.sameReadAs = row.sameReadAs.replace('currentPage=1', 'currentPage=2'); },
    (row) => { row.sameReadAs = row.sameReadAs.replace('api-futures.kucoin.com', 'other.test'); },
    (row) => { row.sameReadAs = row.sameReadAs.replace('/orders?', '/stopOrders?'); },
  ]) {
    const records = structuredClone(original);
    mutate(records.find((record) => record.sameReadAs));
    const { ts, rust } = fixtureReads(records);
    assert.throws(() => applyKucoinRequestContracts(records, ts, rust), /KuCoin read contract/);
  }
  const records = structuredClone(original);
  const row = records.find((record) => record.sameReadAs);
  const target = records.find((record) => record.url === row.sameReadAs);
  const body = JSON.parse(row.body);
  body.data.items = Array.from({ length: 51 }, () => body.data.items[0]);
  body.data.totalNum = 51;
  row.body = target.body = JSON.stringify(body);
  const { ts, rust } = fixtureReads(records);
  assert.throws(() => applyKucoinRequestContracts(records, ts, rust), /first-page coverage/);
});

test('KuCoin native read reasons cannot be reused for unrelated account or cancel-proof reads', async () => {
  const original = await contractFixture();
  for (const mutate of [
    (rows) => { rows.find((row) => row.nativeRead?.reason === 'position-mode').url += '?symbol=XBTUSDTM'; },
    (rows) => { rows.find((row) => row.nativeRead?.reason === 'position-mode').nativeRead.command = 1; },
    (rows) => { const row = rows.find((row) => row.nativeRead?.reason === 'cancel-proof'); row.nativeRead.command = 1; row.afterCommand = 1; },
    (rows) => { const row = rows.find((row) => row.nativeRead?.reason === 'cancel-proof'); row.url = row.url.replace('placed-order', 'another-order'); },
    (rows) => { const row = rows.find((row) => row.nativeRead?.reason === 'cancel-proof'); row.body = row.body.replace('"isActive":false', '"isActive":true'); },
    (rows) => { const row = rows.find((row) => row.type === 'http' && row.method === 'DELETE' && row.url.includes('/placed-order?')); row.body = '{"code":"200000","data":{"cancelledOrderIds":[]}}'; },
  ]) {
    const records = structuredClone(original);
    mutate(records);
    const { ts, rust } = fixtureReads(records);
    assert.throws(() => applyKucoinRequestContracts(records, ts, rust), /KuCoin read contract/);
  }
});

test('KuCoin replay preserves the symbol cancel method and never adds a stop-book write', async () => {
  const calls = [];
  const cancelSymbolOrders = async (symbol) => { calls.push(symbol); };
  const exchange = { xhr: { get: async () => ({}) }, cancelSymbolOrders };
  kucoin.configureTs(exchange);
  assert.equal(exchange.cancelSymbolOrders, cancelSymbolOrders);
  await exchange.cancelSymbolOrders('BTCUSDT');
  assert.deepEqual(calls, ['BTCUSDT']);
});
