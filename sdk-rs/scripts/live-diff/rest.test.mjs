import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import test from 'node:test';
import { deserialize, serialize } from 'node:v8';
import { publicRestPlan, resultStatus, tickerCoverage, qualifyUpstream } from './rest-plan.mjs';
import { readKey, isRead } from './recorder.mjs';
import { restVenueHosts } from './rest-venues.mjs';

const spec = { method: 'fetchOHLCV' };
const ok = { value: [{ timestamp: 1, close: 2 }], errors: 0 };
test('timeouts and stopped workers cannot be mislabelled nondeterministic SDK replies', () => {
  assert.equal(resultStatus(spec, ok, { timeout: true }, [], false).status, 'TIMEOUT');
  assert.equal(resultStatus(spec, ok, { notRun: true }, [], false).status, 'NOT_RUN');
  assert.equal(resultStatus(spec, ok, { probeFailure: true }, [], false).status, 'BLOCKED');
  assert.equal(resultStatus(spec, ok, ok, [{ replayMiss: true }], false).status, 'INPUT_DIFF');
});
test('qualification distinguishes empty, rejected, unstable, and successful reads', () => {
  assert.equal(resultStatus(spec, ok, ok, []).status, 'PASS');
  assert.equal(resultStatus(spec, { value: [] }, { value: [] }, []).status, 'EMPTY');
  assert.equal(resultStatus(spec, ok, ok, [], false).status, 'NONDETERMINISTIC');
  assert.equal(resultStatus(spec, { ...ok, clock: [0] }, { ...ok, clock: [0, 1000] }, []).status, 'CLOCK_DIFF');
  assert.equal(resultStatus(spec, ok, ok, [{ status: 403 }]).status, 'BLOCKED');
  assert.equal(resultStatus(spec, ok, { failure: { message: 'Unknown method' } }, []).status, 'DIFF');
  assert.equal(resultStatus(spec, { failure: 'x' }, { failure: 'x' }, []).status, 'BLOCKED');
  assert.equal(resultStatus({ skip: 'WS_REQUIRED' }, ok, ok, []).status, 'UNAVAILABLE');
});
test('plans use closed candle windows and decline inherited empty trade fallbacks', () => {
  const { plan, symbol } = publicRestPlan(
    'binance',
    { markets: [{ active: true, base: 'BTC', quote: 'USDT', symbol: 'BTCUSDT' }], methods: [], tradeOverride: false },
    1710000000000,
  );
  assert.equal(symbol, 'BTCUSDT');
  assert.deepEqual(
    plan.filter((s) => s.method === 'fetchOHLCV').map((s) => s.args[0].interval),
    ['1m', '1h', '1d'],
  );
  assert.ok(plan.filter((s) => s.method === 'fetchOHLCV').every((s) => s.args[0].to < 1710000000000));
  assert.ok(plan.find((s) => s.method === 'fetchPublicTrades').skip);
});
test('literal REST host inventory covers every maintained registry key exactly', () => {
  const source = readFileSync(new URL('../../../packages/safe-cex/src/exchanges/registry.ts', import.meta.url), 'utf8');
  const keys = [...source.matchAll(/^  (?:'([^']+)'|(\w+)): \(\) =>/gm)].map((m) => m[1] ?? m[2]);
  assert.ok(keys.length > 40);
  assert.deepEqual(Object.keys(restVenueHosts).sort(), keys.sort());
});
test('Coinbase public-only reads follow INTX startup without authenticated brokerage probes', () => {
  for (const venue of ['coinbase', 'coinbasespot']) {
    const { plan } = publicRestPlan(
      venue,
      {
        markets: [{ active: true, symbol: 'BTC-INTX' }],
        methods: [],
        overrides: ['fetchTickersIntx', 'fetchTickersSpot', 'fetchTickersCfm'],
      },
      1710000000000,
    );
    assert.equal(plan.find((s) => s.label === 'tickers-funding-open-interest').method, 'fetchTickersIntx');
    assert.equal(plan.find((s) => s.method === 'fetchTickersIntx').skip, undefined);
    assert.ok(
      plan
        .filter((s) => ['fetchOHLCV', 'fetchTickersSpot', 'fetchTickersCfm'].includes(s.method))
        .every((s) => s.skip.startsWith('AUTH_REQUIRED')),
    );
  }
});
test('Orderly broker candles require signing even though markets and tickers are public', () => {
  for (const venue of ['woofi', 'kodiakfi', 'mode', 'salsadex', 'tealstreet-dex', 'what']) {
    const { plan } = publicRestPlan(
      venue,
      { markets: [{ active: true, symbol: 'PERP_BTC_USDC' }], methods: [] },
      1710000000000,
    );
    const candles = plan.filter((s) => s.method === 'fetchOHLCV');
    assert.equal(candles.length, 3);
    assert.ok(candles.every((s) => s.skip.startsWith('AUTH_REQUIRED')));
    assert.equal(plan.find((s) => s.label === 'markets').skip, undefined);
    assert.equal(plan.find((s) => s.label === 'tickers-funding-open-interest').skip, undefined);
  }
});
test('posted public reads preserve all business parameters when removing correlation envelopes', () => {
  const url = new URL('https://api.crypto.com/exchange/v1/public/get-candlestick');
  const key = (body) => readKey('POST', url, JSON.stringify(body));
  assert.equal(
    key({
      id: 1,
      nonce: 3,
      method: 'public/get-candlestick',
      params: { instrument_name: 'BTC_USDT', timeframe: '1m' },
    }),
    key({
      id: 2,
      nonce: 4,
      method: 'public/get-candlestick',
      params: { instrument_name: 'BTC_USDT', timeframe: '1m' },
    }),
  );
  assert.notEqual(key({ params: { timeframe: '1m' } }), key({ params: { timeframe: '1h' } }));
  for (const path of ['kline', 'book'])
    assert.ok(isRead('POST', new URL(`https://market-data.grvt.io/lite/v1/${path}`), '{}'));
  assert.equal(isRead('POST', new URL('https://market-data.grvt.io/lite/v1/create_order'), '{}'), false);
});
test('typed artifacts preserve own undefined, NaN and signed zero; zero OI is coverage only', () => {
  const output = { value: [{ openInterest: 0, fundingRate: NaN, absent: undefined, price: -0 }] };
  const copy = deserialize(serialize(output));
  assert.deepEqual(copy, output);
  assert.ok(Object.hasOwn(copy.value[0], 'absent'));
  assert.equal(tickerCoverage(copy).openInterest.nonzero, 0);
  assert.equal(tickerCoverage(copy).fundingRate.finite, 0);
});

test('offline recorder fails closed without forwarding uncaptured input', async (t) => {
  const { mkdtempSync, rmSync } = await import('node:fs');
  const { tmpdir } = await import('node:os');
  const { join } = await import('node:path');
  const http = await import('node:http');
  const { makeCA, recordingProxy } = await import('./recorder.mjs');
  const directory = mkdtempSync(join(tmpdir(), 'rest-offline-test-'));
  const records = [];
  let forwarded = 0;
  const proxy = await recordingProxy({
    directory,
    ca: makeCA(directory),
    cache: new Map(),
    records,
    implementation: 'rust',
    allowHost: (host) => host === 'api.bybit.com',
    replayOnly: true,
    forward: () => {
      forwarded++;
      throw new Error('must never forward');
    },
  });
  t.after(async () => {
    await proxy.close();
    rmSync(directory, { recursive: true, force: true });
  });
  const status = await new Promise((resolve, reject) => {
    const request = http.request(
      proxy.url,
      { path: 'https://api.bybit.com/v5/market/kline?symbol=UNSEEN' },
      (response) => {
        response.resume();
        response.on('end', () => resolve(response.statusCode));
      },
    );
    request.on('error', reject);
    request.end();
  });
  assert.equal(status, 409);
  assert.equal(forwarded, 0);
  assert.equal(records[0].replayMiss, true);
});

test('driven host unblocks a frozen limiter and repeats the exact clock trace', async () => {
  const { driveOperation } = await import('./rest-clock-driver.mjs');
  const run = async () => {
    let finish,
      turns = 0;
    const s = {
      atMs: 0,
      records: [{ status: undefined }],
      call: async (method, args) => {
        if (method === '$advance') {
          assert.equal(s.records[0].status, 200);
          finish({ value: [args[0]], errors: 0 });
          return {};
        }
        return new Promise((resolve) => {
          finish = resolve;
        });
      },
    };
    const reply = await driveOperation(
      s,
      { method: 'read', args: [] },
      {
        wait: async () => {
          if (++turns === 2) s.records[0].status = 200;
        },
      },
    );
    assert.ok(turns >= 2, 'HTTP remains fenced before advancing the limiter');
    assert.deepEqual(reply.clock, [0, 1000]);
    return reply;
  };
  assert.deepEqual(await run(), await run());
});

test('later matching clocks cannot qualify Store snapshots after a different bootstrap budget', () => {
  const checks = [
    { label: 'bootstrap-markets', status: 'CLOCK_DIFF' },
    { method: '$markets', status: 'DIFF' },
    { method: '$tickers', status: 'PASS' },
    { method: 'fetchOHLCV', status: 'PASS' },
  ];
  assert.deepEqual(
    qualifyUpstream(checks).map((c) => c.status),
    ['CLOCK_DIFF', 'UPSTREAM_BLOCKED', 'UPSTREAM_BLOCKED', 'PASS'],
  );
});
