import assert from 'node:assert/strict';
import { chmodSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { spawnSync } from 'node:child_process';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import test from 'node:test';
import { gunzipSync } from 'node:zlib';
import { createHmac } from 'node:crypto';
import { expectedGateCases, validateGateCoverage } from './ci-gate-suites.mjs';
import { replayVenueOf } from './venues.mjs';
import { spotReplayRecords, SPOT_SOCKET, SPOT_PRIVATE_SOCKET } from './bingxspot-fixtures.mjs';

test('Spot uses the independent Spot oracle, with no perpetual adapter fallback', () => {
  const spot = replayVenueOf('bingxspot');
  assert.equal(spot.rust, 'bingxspot');
  assert.equal(spot.ts.className, 'BingxSpotReplayExchange');
  assert.equal(spot.rustFixture, 'bingxSpotReplayFixture');
  assert(spot.ts.module.endsWith('/bingxspot-reference.mjs'));
  const adapter = readFileSync(new URL('./bingxspot-reference.mjs', import.meta.url), 'utf8');
  assert.match(adapter, /reference\/bingxspot\/bingxspot\.exchange\.ts/);
  assert.doesNotMatch(adapter, /reference\/bingx\/|BingxExchange|\/swap\//);
});

test('fixture build flag is opt-in and refuses unrelated families', t => {
  const directory = mkdtempSync(join(tmpdir(), 'bingxspot-build-'));
  t.after(() => rmSync(directory, { recursive: true, force: true }));
  writeFileSync(join(directory, 'cargo'), '#!/bin/sh\nprintf "%s\\n" "$@"\nexit 73\n');
  chmodSync(join(directory, 'cargo'), 0o755);
  const script = new URL('./build.sh', import.meta.url).pathname;
  for (const [args, fixture, status] of [
    [[], false, 73], [['--venue', 'bingx'], false, 73],
    [['--venue', 'bingx', '--bingx-spot-replay'], true, 73],
    [['--bingx-spot-replay'], true, 73],
    [['--venue', 'bybit', '--bingx-spot-replay'], false, 2],
    [['--families', '--bingx-spot-replay'], false, 2],
  ]) {
    const run = spawnSync('bash', [script, ...args], { encoding: 'utf8',
      env: { ...process.env, PATH: `${directory}:${process.env.PATH}` } });
    assert.equal(run.status, status, run.stderr);
    assert.equal(run.stdout.split('\n').includes('bingx-spot-replay'), fixture, args.join(' '));
  }
});

test('Spot signing validation rejects perpetual routes, mutated economics and missing auth', () => {
  const spot = replayVenueOf('bingxspot');
  const params = { quantity: 0.002, side: 'SELL', symbol: 'BTC-USDT', timestamp: Date.now(), type: 'MARKET' };
  const canonical = Object.entries(params).sort(([a], [b]) => a < b ? -1 : 1)
    .map(([key, value]) => `${encodeURIComponent(key)}=${encodeURIComponent(value)}`).join('&');
  const signed = { ...params, signature: createHmac('sha256', 'replay-secret').update(canonical).digest('hex') };
  const request = { method: 'POST', url: 'https://open-api.bingx.com/openApi/spot/v1/trade/order',
    headers: { 'X-BX-APIKEY': 'replay-key' }, body: JSON.stringify(signed) };
  assert.equal(spot.verifyRequest(request), true);
  assert.equal(spot.verifyRequest({ ...request, body: JSON.stringify({ ...signed, quantity: 0.02 }) }), false);
  assert.equal(spot.verifyRequest({ ...request, headers: {} }), false);
  assert.equal(spot.verifyRequest({ ...request, url: request.url.replace('/spot/v1/', '/swap/v2/') }), false);
  assert.equal(spot.verifyRequest({ ...request, url: 'https://other.invalid/openApi/spot/v1/trade/order' }), false);
});

test('Spot protocol audit requires actual ticker/book and explicit private subscriptions', () => {
  const audit = replayVenueOf('bingxspot').createSocketAudit('rust', { isPrivate: true, books: ['BTCUSDT'] });
  const send = (url, dataType) => audit.sent(url, JSON.stringify({ reqType: 'sub', dataType, id: 'fixture' }));
  send(SPOT_SOCKET, 'BTC-USDT@ticker');
  send(SPOT_SOCKET, 'BTC-USDT@depth50');
  assert.equal(audit.ready(), false, 'public data cannot stand in for private qualification');
  send(SPOT_PRIVATE_SOCKET, 'spot.executionReport');
  assert.equal(audit.ready(), false);
  send(SPOT_PRIVATE_SOCKET, 'ACCOUNT_UPDATE');
  assert.equal(audit.ready(), true);
  assert.deepEqual(audit.errors(), []);
  send(SPOT_SOCKET, 'spot.executionReport');
  assert(audit.errors().includes('Spot subscription on wrong socket role'));
});

test('BingX cannot qualify with only its perpetual public/private tapes', () => {
  const cases = expectedGateCases('bingx');
  assert.deepEqual(cases.filter(c => c.venue === 'bingxspot'), [
    { venue: 'bingxspot', tape: 'sdk-rs/harness/fixtures/bingxspot-public.jsonl.gz', books: 'BTCUSDT' },
    { venue: 'bingxspot', tape: 'sdk-rs/harness/tapes/bingxspot-private.jsonl', books: '' },
  ]);
  const perpOnly = cases.filter(c => c.venue !== 'bingxspot');
  assert.equal(validateGateCoverage('bingx', perpOnly).filter(e => e.startsWith('missing gate case:')).length, 2);
  assert.deepEqual(validateGateCoverage('bingx', cases), []);
});

test('Spot fixtures retain distinct Spot REST/WS identities and exact private command economics', () => {
  for (const privateAccount of [false, true]) {
    const records = spotReplayRecords(privateAccount);
    assert.equal(records[0].private, privateAccount);
    for (const r of records.filter(r => r.type === 'http')) {
      const path = new URL(r.url).pathname;
      assert(path.startsWith('/openApi/spot/') || path === '/openApi/user/auth/userDataStream');
      assert(!r.url.includes('/swap/'));
    }
    for (const r of records.filter(r => r.type === 'ws')) assert.equal(r.url, privateAccount ? SPOT_PRIVATE_SOCKET : SPOT_SOCKET);
  }
  const records = spotReplayRecords(true);
  const writes = records.filter(r => r.type === 'http' && r.url.endsWith('/trade/order')).map(r => JSON.parse(r.requestBody));
  assert.deepEqual(writes.map(r => [r.side, r.type, r.quantity, r.quoteOrderQty]), [
    ['BUY', 'LIMIT', 0.002, undefined], ['BUY', 'MARKET', undefined, 82], ['SELL', 'MARKET', 0.002, undefined],
  ]);
  assert.equal(records.find(r => r.type === 'command').expected[0], '9007199254740993');
  const text = r => r.binary ? gunzipSync(Buffer.from(r.binary, 'base64')).toString('utf8') : r.text;
  const reports = records.filter(r => r.type === 'ws' && text(r).includes('PARTIALLY_FILLED'));
  assert.equal(reports.length, 2, 'duplicate private fill is replayed, not silently removed from the tape');
  assert(text(reports[0]).includes('"i":9007199254740993'), 'WS order ID is a raw integer above JS safe range');
  assert(text(reports[0]).includes('"t":9007199254740995'), 'WS execution ID is also exact');
});
