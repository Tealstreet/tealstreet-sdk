// Native OKX product contracts, independently derived from its API schemas.
import { test } from 'node:test';
import { createHmac } from 'node:crypto';
import assert from 'node:assert/strict';
import { execFileSync } from 'node:child_process';
import { mkdtempSync, readFileSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { venueOf } from './venues.mjs';
import { requestKey } from './requests.mjs';
import { Tape } from './tape.mjs';
test('product request matching preserves native kind/instrument fields and refuses category aliases', () => {
  for (const name of ['okx', 'okxdemo', 'okxeu', 'okxeudemo']) {
    const venue = venueOf(name), base = `https://${name.startsWith('okxeu') ? 'eea.okx.com' : name.endsWith('demo') ? 'openapi.okx.com' : 'www.okx.com'}/api/v5/trade/orders-history`;
    const native = `${base}?instType=FUTURES&instId=BTC-USD-UM&limit=100`;
    for (const other of [native.replace('FUTURES', 'SWAP'), native.replace('instType=', 'category='), native.replace('instId=', 'symbol=')])
      assert.notEqual(requestKey('GET', native, undefined, new Set(), venue), requestKey('GET', other, undefined, new Set(), venue));
  }
});

test('EEA history reference keeps real product mappers and joins newest whole income bill groups', async () => {
  const requests = [];
  const exchange = {
    fetchMarkets: async () => [],
    store: { marketsById: { 'BTC-USD-UM': { info: { instType: 'FUTURES' } } } },
    getMarketInstrumentType: (market) => market.info.instType,
    xhr: { get: async (url, config) => { requests.push(config.params); return config.params.instType; } },
    fetchHistoricalOrders: async (options) => {
      const source = await exchange.xhr.get('/api/v5/trade/orders-history', { params: { instType: 'SWAP', ...(options.symbol ? { instId: 'BTC-USD-UM' } : {}) } });
      return [{ id: source, createdTime: source === 'FUTURES' ? 2000 : 1000 }];
    },
    fetchIncomeHistory: async () => {
      const source = await exchange.xhr.get('/api/v5/account/bills', { params: { instType: 'SWAP' } });
      return source === 'FUTURES' ? [{ tranId: '2:pnl', timestamp: 2000 }, { tranId: '2:fee', timestamp: 2000 }] : [{ tranId: '1:pnl', timestamp: 1000 }];
    },
  };
  venueOf('okxeu').prepareTs(exchange);
  assert.deepEqual((await exchange.fetchHistoricalOrders({ limit: 1 })).map((row) => row.id), ['FUTURES']);
  assert.deepEqual(requests.splice(0).map((p) => p.instType), ['SWAP', 'FUTURES']);
  assert.deepEqual((await exchange.fetchIncomeHistory({ limit: 1 })).map((row) => row.tranId), ['2:pnl', '2:fee']);
  assert.deepEqual(requests.splice(0).map((p) => p.instType), ['SWAP', 'FUTURES']);
  await exchange.fetchHistoricalOrders({ symbol: 'BTCUSDUM' });
  assert.equal(requests[0].instType, 'FUTURES');
});

test('private tape generators retain OKX inverse/EEA native startup surface', () => {
  const directory = mkdtempSync(join(tmpdir(), 'phase9-product-tapes-'));
  const generate = (script, args, name) => {
    const out = join(directory, name + '.jsonl');
    execFileSync('corepack', ['yarn', '-s', script, ...args, '--out', out], { stdio: 'pipe' });
    return readFileSync(out, 'utf8').trim().split('\n').map(JSON.parse);
  };
  try {
    for (const name of ['okx', 'okxdemo', 'okxeu', 'okxeudemo']) {
      const records = generate('rust:private-tape-okx', ['--venue', name], name);
      const catalogs = records.filter((r) => r.type === 'http' && r.url.includes('/instruments?'));
      const rows = catalogs.flatMap((r) => JSON.parse(r.body).data);
      assert(rows.some((r) => r.instId === 'BTC-USD-SWAP' && r.ctType === 'inverse' && r.ctVal === '100'));
      assert(records.some((r) => r.type === 'command' && r.op === 'fetchFills' && r.symbol === 'BTCUSD'));
      if (name.startsWith('okxeu')) {
        assert(catalogs.every((r) => r.url.includes('/account/instruments?')));
        assert(rows.some((r) => r.instId === 'BTC-USD_UM_XPERP-310404' && r.instType === 'FUTURES' && r.ruleType === 'xperp' && r.ctVal === '0.0001'));
        assert(records.some((r) => r.url?.endsWith('/account/positions?instType=FUTURES')));
        assert(records.some((r) => r.type === 'command' && r.op === 'fetchOrderHistory' && r.symbol === 'BTCUSD_UM_XPERP310404'));
        const startup = records.filter((record) => record.type === 'http' && /^\/api\/v5\/(account\/(account-position-risk|positions)|trade\/(orders-pending|orders-algo-pending))$/.test(new URL(record.url).pathname));
        assert(startup.length > 0);
        for (const scoped of startup) {
          const url = new URL(scoped.url), scope = url.searchParams.get('instType'), nativeRows = JSON.parse(scoped.body).data;
          assert(['SWAP', 'FUTURES'].includes(scope), 'startup reads always retain their exact native product');
          const balance = url.pathname.endsWith('/account-position-risk');
          if (balance) assert.equal(scope, 'SWAP', 'account collateral uses the same single risk scope as TS');
          else assert(nativeRows.every((row) => row.instType === scope), 'product reply contains only its declared native rows');
          for (const instType of balance ? ['SWAP'] : ['SWAP', 'FUTURES']) {
            url.searchParams.set('instType', instType);
            const key = requestKey('GET', url.href, undefined, new Set(), venueOf(name));
            const filtered = startup.find((record) => record.afterCommand === scoped.afterCommand
              && requestKey('GET', record.url, undefined, new Set(), venueOf(name)) === key);
            assert(filtered, `real TS ${instType} read must have its exact native reply: ${url}`);
          }
          for (const missing of [undefined, 'OPTION', ...(balance ? ['FUTURES'] : [])]) {
            if (missing === undefined) url.searchParams.delete('instType'); else url.searchParams.set('instType', missing);
            assert(!startup.some((record) => requestKey('GET', record.url, undefined, new Set(), venueOf(name))
              === requestKey('GET', url.href, undefined, new Set(), venueOf(name))), 'missing or unrecorded product cannot borrow a scoped reply');
          }
        }
        for (const endpoint of ['/account/positions', '/trade/orders-pending']) {
          const sources = startup.filter((record) => new URL(record.url).pathname.endsWith(endpoint));
          assert(sources.some((record) => JSON.parse(record.body).data.some((row) => row.instId === 'BTC-USD-SWAP' && row.instType === 'SWAP')));
          assert(sources.some((record) => JSON.parse(record.body).data.some((row) => row.instId === 'BTC-USD_UM_XPERP-310404' && row.instType === 'FUTURES')));
        }
      }
      const tape = new Tape([], venueOf(name));
      for (const record of records.filter((record) => record.type === 'ws')) {
        const message = JSON.parse(record.text);
        if (!message.arg) continue;
        const { channel } = message.arg;
        const topics = channel === 'account' ? [{ channel }]
          : record.url.endsWith('/public') ? rows.map((row) => ({ channel, instId: row.instId }))
          : (name.startsWith('okxeu') ? ['SWAP', 'FUTURES'] : ['SWAP']).map((instType) => ({ channel, instType }));
        const socket = { url: record.url, topics: new Map(topics.map((topic, index) => [index, topic])) };
        assert.doesNotThrow(() => tape.assertSubscribed(socket, message), 'native channel/product must match every pushed row');
        if (message.arg.instType === 'FUTURES') {
          assert(message.data.every((row) => row.instType === 'FUTURES'));
          assert.throws(() => tape.assertSubscribed(socket, { ...message, arg: { ...message.arg, instType: 'SWAP' } }), /Unsubscribed/);
        }
        if (record.url.endsWith('/public')) assert(message.data.every((row) => row.instId === message.arg.instId));
        if (channel === 'account') assert.equal(message.arg.instType, undefined);
      }
    }
  } finally { rmSync(directory, { recursive: true, force: true }); }
});

test('OKX reference repairs only absent native preopen caps and retains malformed supplied limits', async () => {
  // The same independently recorded public CRML row drives the native regression.
  const raw = JSON.parse(readFileSync(new URL('../venues/okx/tests/fixtures/crml-preopen.json', import.meta.url), 'utf8'));
  const mapped = (info) => ({ id: info.instId, active: false, info, limits: { amount: { min: 1, max: Number.NaN }, leverage: { min: 1, max: 20 } } });
  const rows = [mapped(raw), ...['bad', 'NaN', 'inf', '-1', '0'].map((value) => mapped({ ...raw, maxTriggerSz: value })), mapped({})];
  const exchange = {
    fetchMarkets: async () => rows,
    xhr: { get: async () => {} }, fetchHistoricalOrders: async () => [], fetchIncomeHistory: async () => [],
  };
  venueOf('okx').prepareTs(exchange);
  const result = await exchange.fetchMarkets();
  assert.equal(result[0].limits.amount.max, 20000);
  assert.equal(result[0].active, false);
  assert.deepEqual(result[0].info, raw, 'raw venue metadata remains unchanged');
  assert.equal(result[0].limits.amount.min, 1);
  for (const row of result.slice(1)) assert(Number.isNaN(row.limits.amount.max), 'malformed or unproven limit must not be repaired');
  assert(Number.isNaN(rows[0].limits.amount.max), 'reference correction does not mutate producer output');
});

test('OKX independent raw HTTP proof checks HMAC, credentials, freshness, native aliases and demo scope before matching', () => {
  for (const name of ['okx', 'okxdemo', 'okxeu', 'okxeudemo']) {
    const venue = venueOf(name), demo = name.endsWith('demo');
    const host = name.startsWith('okxeu') ? 'eea.okx.com' : demo ? 'openapi.okx.com' : 'www.okx.com';
    const sign = (method, path, body = '', time = Date.now()) => {
      const timestamp = new Date(time).toISOString();
      return { method, url: `https://${host}${path}`, body, headers: {
        'OK-ACCESS-KEY': 'replay-key', 'OK-ACCESS-PASSPHRASE': 'replay-passphrase', 'OK-ACCESS-TIMESTAMP': timestamp,
        'OK-ACCESS-SIGN': createHmac('sha256', 'replay-secret').update(timestamp + method + path + body).digest('base64'),
        ...(demo ? { 'x-simulated-trading': '1' } : {}),
      } };
    };
    const valid = sign('GET', '/api/v5/trade/orders-history?instType=FUTURES&instId=BTC-USD-UM&limit=100');
    assert.doesNotThrow(() => venue.validateRequest(valid));
    for (const mutate of [
      (r) => { r.headers['OK-ACCESS-SIGN'] = 'wrong'; },
      (r) => { r.headers['OK-ACCESS-KEY'] = 'other'; },
      (r) => { r.headers['OK-ACCESS-PASSPHRASE'] = 'other'; },
      (r) => { r.headers['OK-ACCESS-TIMESTAMP'] = new Date(Date.now() - 31000).toISOString(); },
      (r) => { r.url = r.url.replace('FUTURES', 'SWAP'); },
      (r) => { r.headers['x-simulated-trading'] = demo ? '0' : '1'; },
    ]) { const changed = structuredClone(valid); mutate(changed); assert.throws(() => venue.validateRequest(changed)); }
    assert.throws(() => venue.validateRequest(sign('GET', '/api/v5/account/positions', '', Date.now() - 31000)), /timestamp/);
    const body = JSON.stringify({ instId: 'BTC-USD-SWAP', ordId: '5' });
    const post = sign('POST', '/api/v5/trade/cancel-order', body);
    assert.doesNotThrow(() => venue.validateRequest(post));
    assert.throws(() => venue.validateRequest({ ...post, body: body.replace('"5"', '"6"') }), /signature/);
    for (const alias of ['c', 'clientOrderId', 'requestId', 'category', 'symbol']) {
      const query = sign('GET', `/api/v5/account/positions?${alias}=ignored`);
      const value = sign('POST', '/api/v5/trade/cancel-order', JSON.stringify({ instId: 'BTC-USD-SWAP', [alias]: 'ignored' }));
      assert.throws(() => venue.validateRequest(query), /Foreign OKX query alias/);
      assert.throws(() => venue.validateRequest(value), /Foreign OKX body alias/);
    }
  }
});
