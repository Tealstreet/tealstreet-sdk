import assert from 'node:assert/strict';
import test from 'node:test';
import { coinbasePrivateTape } from './private-tape-coinbase.mjs';
import { fixCoinbase, COINBASE_TS_FIXES } from './known-ts-fixes-coinbase.mjs';
import { selectResponse } from './requests.mjs';

const root = 'https://api.coinbase.com/api/v3/brokerage';
const market = {
  product_id: 'BTC-USD',
  product_type: 'SPOT',
  base_increment: '0.001',
  price_increment: '0.01',
  base_min_size: '0.001',
  base_max_size: '100',
  price: '100',
  volume_24h: '1',
  price_percentage_change_24h: '0',
};
const meta = { type: 'meta', private: true, profile: 'spot' };
const http = (path, body, extra = {}) => ({
  type: 'http',
  method: 'GET',
  url: root + path,
  status: 200,
  body: JSON.stringify(body),
  ...extra,
});
const row = (id, filled, left, average, status = 'OPEN') => ({
  order_id: id,
  client_order_id: `client-${id}`,
  product_id: 'BTC-USD',
  order_side: 'BUY',
  order_type: 'Limit',
  status,
  cumulative_quantity: String(filled),
  leaves_quantity: String(left),
  limit_price: '100',
  stop_price: '',
  avg_price: String(average),
});
const ws = (kind, orders, sequence = 0) => ({
  type: 'ws',
  url: 'wss://advanced-trade-ws-user.coinbase.com',
  text: JSON.stringify({ channel: 'user', sequence_num: sequence, events: [{ type: kind, orders }] }),
});
const account = () => ({
  fills: [],
  orders: [],
  settings: [],
  balance: { total: 0, free: 0, used: 0, upnl: 0 },
  positions: [],
});
const fill = {
  trade_id: 't1',
  order_id: 'one',
  product_id: 'BTC-USD',
  trade_type: 'FILL',
  trade_time: '2025-12-15T12:08:00.001Z',
  price: '100',
  size: '0.1',
  size_in_quote: false,
  commission: '0.01',
  liquidity_indicator: 'MAKER',
  side: 'BUY',
};
const request = (op, args = {}) => ({ type: 'command', op, ...args });

test('tapes cover every product profile and native JWT algorithm with bounded exact commands', () => {
  for (const profile of ['intx', 'cfm', 'spot'])
    for (const algorithm of ['ES256', 'EdDSA']) {
      const tape = coinbasePrivateTape({ profile, algorithm });
      assert.equal(tape.commands, 15);
      assert.ok(tape.seconds < 90);
      assert.equal(tape.records[0].profile, profile);
      assert.equal(tape.records[0].algorithm, algorithm);
      assert.ok(
        tape.records.some(
          (r) => r.type === 'http' && r.method === 'POST' && r.requestBody.includes('CoinbaseCaller-30'),
        ),
      );
      const config = tape.records.find((r) => r.type === 'config');
      assert.equal(config.options.spotOnly, false);
      const privateFrames = tape.records
        .filter((r) => r.type === 'ws' && r.url.includes('-ws-user.'))
        .map((r) => JSON.parse(r.text));
      assert.deepEqual(
        privateFrames.map((r) => r.sequence_num),
        privateFrames.map((_, i) => i),
      );
      if (profile === 'spot') {
        const probes = tape.records.filter((r) => r.type === 'http' && r.url.includes('/cfm/'));
        assert.equal(probes.length, 1);
        assert.equal(probes[0].status, 403);
        assert.ok(probes[0].url.endsWith('/cfm/balance_summary'));
      }
    }
  for (const algorithm of ['ES256', 'EdDSA']) {
    const saved = coinbasePrivateTape({ profile: 'spot', venue: 'coinbasespot', algorithm });
    assert.equal(saved.venue, 'coinbasespot');
    assert.equal(saved.records.find((r) => r.type === 'config').options.spotOnly, true);
    assert.ok(
      saved.records
        .filter((r) => r.type === 'http')
        .every((r) => !r.url.includes('/cfm/') && !r.url.includes('/intx/')),
    );
  }
  assert.throws(() => coinbasePrivateTape({ profile: 'cfm', venue: 'coinbasespot' }), /requires the spot/);
  assert.throws(() => coinbasePrivateTape({ algorithm: 'none' }), /Expected algorithm/);
});
test('query corrections use input payloads and preserve transport failures and unexpected reads', () => {
  const records = [
    meta,
    http('/products', { products: [market] }),
    request('fetchFills', { symbol: 'BTC-USD', limit: 2 }),
    http('/orders/historical/fills?limit=2&sort_by=TRADE_TIME&product_types=SPOT&product_ids=BTC-USD', {
      fills: [fill],
      cursor: '',
      proof_token_required: false,
    }),
  ];
  const ts = {
    commands: [{ ok: [] }],
    requests: ['GET https://example.test/unexpected?'],
    invalidSignatures: 4,
    unserved: 2,
    missingWrites: 3,
    protocolErrors: ['failure'],
  };
  fixCoinbase(ts, records);
  assert.deepEqual(ts.commands[0], {
    ok: [
      {
        id: 't1',
        timestamp: Date.parse(fill.trade_time) / 1000,
        symbol: 'BTC-USD',
        orderId: 'one',
        price: 100,
        qty: 0.1,
        side: 'buy',
        commission: 0.01,
        maker: true,
      },
    ],
  });
  assert.equal(ts.invalidSignatures, 4);
  assert.equal(ts.unserved, 2);
  assert.equal(ts.missingWrites, 3);
  assert.deepEqual(ts.protocolErrors, ['failure']);
  assert.ok(ts.requests.includes('GET https://example.test/unexpected?'));
  assert.equal(fixCoinbase.length, 2, 'oracle has no native result argument');
  assert.throws(() => fixCoinbase({ commands: [{ ok: [{ price: 999 }] }] }, records), /unexpected TS/);
  assert.throws(() => fixCoinbase({ commands: [{ error: 'native query broke' }] }, records), /unexpected TS/);
});
test('query corrections reject failed response proof and never heal protocol counters', () => {
  const records = [
    meta,
    http('/products', { products: [market] }),
    request('fetchFills', { symbol: 'BTC-USD' }),
    http('/orders/historical/fills?limit=100&sort_by=TRADE_TIME&product_types=SPOT&product_ids=BTC-USD', {
      fills: [fill],
      proof_token_required: true,
    }),
  ];
  assert.throws(() => fixCoinbase({ commands: [{ ok: [] }] }, records), /proof token/);
  records.at(-1).status = 503;
  assert.throws(() => fixCoinbase({ commands: [{ ok: [] }] }, records), /failed Coinbase/);
});
test('incremental fill oracle pins legacy terminal totals and exact cumulative execution values', () => {
  const records = [
    meta,
    http('/products', { products: [market] }),
    ws('snapshot', [row('one', 0, 3, 0)]),
    ws('update', [row('one', 1, 2, 100)]),
    ws('update', [row('one', 3, 0, 102, 'FILLED')]),
    ws('update', [row('one', 3, 0, 102, 'FILLED')]),
  ];
  const legacy = { id: 'one', symbol: 'BTC-USD', side: 'buy', amount: 3, price: '102' };
  const a = account();
  a.fills = [legacy, { ...legacy }];
  fixCoinbase({ account: a }, records);
  assert.deepEqual(a.fills, [
    { id: 'one', symbol: 'BTC-USD', side: 'buy', amount: 1, price: 100 },
    { id: 'one', symbol: 'BTC-USD', side: 'buy', amount: 2, price: 103 },
  ]);
  const wrong = account();
  wrong.fills = [{ ...legacy, price: 999 }, { ...legacy }];
  assert.throws(() => fixCoinbase({ account: wrong }, records), /unexpected TS/);
});
test('source changes alter oracle execution price independently of any native implementation', () => {
  const records = [
    meta,
    http('/products', { products: [market] }),
    ws('snapshot', [row('one', 0, 3, 0)]),
    ws('update', [row('one', 1, 2, 100)]),
    ws('update', [row('one', 3, 0, 104, 'FILLED')]),
  ];
  const a = account();
  a.fills = [{ id: 'one', symbol: 'BTC-USD', side: 'buy', amount: 3, price: '104' }];
  fixCoinbase({ account: a }, records);
  assert.equal(a.fills[1].price, 106);
});
test('currency correction changes only the documented held-total defect', () => {
  const records = [
    meta,
    http('/products', { products: [market] }),
    http('/portfolios/test', {
      breakdown: { spot_positions: [{ asset: 'BTC', total_balance_crypto: 1.5, available_to_trade_crypto: 1 }] },
    }),
  ];
  const a = account();
  a.balance.currencies = { BTC: { total: 1, free: 1, used: 0.5, upnl: 0 } };
  fixCoinbase({ account: a }, records);
  assert.deepEqual(a.balance.currencies.BTC, { total: 1.5, free: 1, used: 0.5, upnl: 0 });
  a.balance.currencies.BTC.total = 999;
  assert.throws(() => fixCoinbase({ account: a }, records), /unexpected TS/);
});

test('quote-sized spot order correction requires exact native detail proof and legacy quantities', () => {
  const raw = { ...row('quote-order', 1, 100, 100), client_order_id: '' };
  const detail = http('/orders/historical/quote-order', {
    order: {
      order_id: 'quote-order',
      product_id: 'BTC-USD',
      order_configuration: { limit_limit_gtc: { quote_size: '200', limit_price: '100' } },
    },
  });
  const records = [meta, http('/products', { products: [market] }), ws('snapshot', [raw]), detail];
  const legacy = { id: 'quote-order', type: 'limit', price: 100, amount: 101, filled: 1, remaining: 100 };
  const a = account();
  a.orders = [{ ...legacy }];
  fixCoinbase({ account: a }, records);
  assert.deepEqual(a.orders, [{ ...legacy, amount: 2, remaining: 1 }]);
  const withoutProof = account();
  withoutProof.orders = [{ ...legacy }];
  fixCoinbase({ account: withoutProof }, records.slice(0, -1));
  assert.deepEqual(withoutProof.orders, [legacy]);
  const failedProof = structuredClone(records);
  failedProof.at(-1).status = 503;
  const failed = account();
  failed.orders = [{ ...legacy }];
  fixCoinbase({ account: failed }, failedProof);
  assert.deepEqual(failed.orders, [legacy]);
  const wrong = account();
  wrong.orders = [{ ...legacy, remaining: 999 }];
  assert.throws(() => fixCoinbase({ account: wrong }, records), /unexpected TS/);
  const wrongIdentity = structuredClone(records);
  const body = JSON.parse(wrongIdentity.at(-1).body);
  body.order.product_id = 'ETH-USD';
  wrongIdentity.at(-1).body = JSON.stringify(body);
  const mismatched = account();
  mismatched.orders = [{ ...legacy }];
  assert.throws(() => fixCoinbase({ account: mismatched }, wrongIdentity), /product mismatch/);
});
test('CFM PnL and spot leverage repairs require exact known legacy values', () => {
  const records = [
    { ...meta, profile: 'cfm' },
    http('/products', { products: [market] }),
    http('/cfm/balance_summary', { balance_summary: { unrealized_pnl: { value: '1.5' } } }),
  ];
  const a = account();
  a.settings = [{ symbol: 'BTC-USD', leverage: 10, marginType: 'cross' }];
  fixCoinbase({ account: a }, records);
  assert.equal(a.balance.upnl, 1.5);
  assert.equal(a.settings[0].leverage, 1);
  a.balance.upnl = 50;
  assert.throws(() => fixCoinbase({ account: a }, records), /unexpected TS/);
});
test('product pagination request correction retains unrelated reads and exact write sequence', () => {
  const records = [{ type: 'meta', private: false }, http('/products?limit=1000', { products: [market] })];
  const write = 'POST https://example.test/orders? {"quantity":2}';
  const ts = {
    requests: [`GET ${root}/products?`, write, 'GET https://example.test/unexpected?'],
    writes: [write],
    writeCount: 1,
  };
  fixCoinbase(ts, records);
  assert.ok(ts.requests.includes(`GET ${root}/products?limit=1000`));
  assert.ok(ts.requests.includes('GET https://example.test/unexpected?'));
  assert.deepEqual(ts.writes, [write]);
  assert.equal(ts.writeCount, 1);
});
test('native client ID correction refuses an unexpected observed client identity', () => {
  const records = [meta, http('/products', { products: [market] }), ws('snapshot', [row('one', 0, 1, 0)])];
  const a = account();
  a.orders = [{ id: 'one', clientOrderId: null }];
  fixCoinbase({ account: a }, records);
  assert.equal(a.orders[0].clientOrderId, 'client-one');
  a.orders[0].clientOrderId = 'wrong';
  assert.throws(() => fixCoinbase({ account: a }, records), /unexpected TS/);
});
test('all documented corrections have explanatory labels', () => {
  assert.ok(COINBASE_TS_FIXES.length >= 10);
  assert.ok(COINBASE_TS_FIXES.every((s) => s.length > 30));
});

test('query pagination walks exact cursors, deduplicates overlap and ignores future command stages', () => {
  const command = request('fetchFills', { symbol: 'BTC-USD', limit: 2, t: 1000 });
  const second = { ...fill, trade_id: 't2', price: '101', trade_time: '2025-12-15T12:08:00.002Z' };
  const records = [
    meta,
    http('/products', { products: [market] }),
    command,
    http('/orders/historical/fills?limit=2&sort_by=TRADE_TIME&product_types=SPOT&product_ids=BTC-USD', {
      fills: [fill],
      cursor: 'two',
      has_next: true,
    }),
    http('/orders/historical/fills?limit=2&sort_by=TRADE_TIME&product_types=SPOT&product_ids=BTC-USD&cursor=two', {
      fills: [fill, second],
      cursor: '',
    }),
    http(
      '/orders/historical/fills?limit=2&sort_by=TRADE_TIME&product_types=SPOT&product_ids=BTC-USD',
      { fills: [{ ...fill, price: '999' }], cursor: '' },
      { afterCommand: 2 },
    ),
  ];
  const ts = { commands: [{ ok: [] }], requests: [] };
  fixCoinbase(ts, records);
  assert.deepEqual(
    ts.commands[0].ok.map((r) => [r.id, r.price]),
    [
      ['t2', 101],
      ['t1', 100],
    ],
  );
  assert.equal(ts.requests.length, 2);
  records.splice(4, 1);
  assert.throws(() => fixCoinbase({ commands: [{ ok: [] }] }, records), /missing Coinbase cursor/);
});
test('partial cancellation correction rejects unexpected results and keeps the failed identity visible', () => {
  const records = [
    meta,
    request('cancelOrders', { ids: ['12', '13'] }),
    {
      type: 'http',
      method: 'POST',
      url: root + '/orders/batch_cancel',
      status: 200,
      requestBody: JSON.stringify({ order_ids: ['12', '13'] }),
      body: JSON.stringify({
        results: [
          { order_id: '12', success: true },
          { order_id: '13', success: false, failure_reason: 'UNKNOWN_CANCEL_ORDER' },
        ],
      }),
    },
  ];
  const ts = { commands: [{ ok: [] }], requests: [], writes: ['untouched'] };
  fixCoinbase(ts, records);
  assert.deepEqual(ts.commands, [{ error: '13: UNKNOWN_CANCEL_ORDER' }]);
  assert.deepEqual(ts.writes, ['untouched']);
  assert.throws(() => fixCoinbase({ commands: [{ error: 'other failure' }] }, records), /unexpected TS/);
});

test('command-owned replies cannot serve before the triggering command', () => {
  for (const profile of ['intx', 'cfm', 'spot']) {
    const tape = coinbasePrivateTape({ profile });
    for (const r of tape.records.filter(
      (r) =>
        r.type === 'http' &&
        (r.method !== 'GET' ||
          r.url.includes('sort_by=') ||
          r.url.includes('order_status=FILLED') ||
          r.url.includes('/ticker?') ||
          r.url.includes('/candles?')),
    )) {
      assert.ok(r.afterCommand > 0, `missing command stage ${r.method} ${r.url}`);
      assert.equal(
        selectResponse([r], Infinity, { write: r.method !== 'GET', commandIndex: r.afterCommand - 1 }),
        undefined,
      );
      assert.equal(selectResponse([r], Infinity, { write: r.method !== 'GET', commandIndex: r.afterCommand }), r);
    }
  }
});

test('order history tapes use default sorting and external spot orders have native detail proof', () => {
  for (const profile of ['intx', 'cfm', 'spot']) {
    const tape = coinbasePrivateTape({ profile });
    const history = tape.records.filter((r) => r.type === 'http' && r.url.includes('order_status=FILLED'));
    assert.ok(history.length > 0);
    assert.ok(history.every((r) => !new URL(r.url).searchParams.has('sort_by')));
    if (profile === 'spot') {
      for (const id of ['11', '12', '13', '21']) {
        const proof = tape.records.find((r) => r.type === 'http' && r.url.endsWith(`/orders/historical/${id}`));
        assert.ok(proof);
        assert.ok(JSON.parse(proof.body).order.order_configuration.limit_limit_gtc.base_size);
      }
    }
  }
});

test('CFM public trades preserve contract quantity and quote the base contract notional', () => {
  const marketCFM = {
    ...market,
    product_id: 'BTC-CDE',
    product_type: 'FUTURE',
    future_product_details: { contract_expiry_type: 'EXPIRING', contract_size: '0.01' },
  };
  const records = [
    { ...meta, profile: 'cfm' },
    http('/products', { products: [marketCFM] }),
    request('fetchPublicTrades', { symbol: 'BTC-CDE', limit: 1 }),
    http('/market/products/BTC-CDE/ticker?limit=1', {
      trades: [
        {
          trade_id: 't1',
          product_id: 'BTC-CDE',
          price: '110',
          size: '2',
          side: 'BUY',
          time: '2025-12-15T12:08:00.000Z',
        },
      ],
    }),
  ];
  const ts = { commands: [{ error: 'Not implemented' }], requests: [] };
  fixCoinbase(ts, records);
  assert.equal(ts.commands[0].ok[0].qty, 2);
  assert.equal(ts.commands[0].ok[0].quoteQty, 2.2);
  assert.equal(ts.commands[0].ok[0].side, 'sell');
});

test('active market classification comes only from documented status and disabled flags', () => {
  const rows = [
    { ...market, status: 'online' },
    { ...market, product_id: 'ETH-USD', status: 'TRADING', is_disabled: true },
    { ...market, product_id: 'SOL-USD', status: 'online', trading_disabled: true },
    { ...market, product_id: 'LTC-USD', status: 'delisted' },
  ];
  const records = [{ type: 'meta', private: false }, http('/market/products', { products: rows })];
  const ts = { markets: rows.map((r) => ({ id: r.product_id, active: r.status === 'TRADING' })) };
  fixCoinbase(ts, records);
  assert.deepEqual(
    ts.markets.map((r) => r.active),
    [true, false, false, false],
  );
  assert.throws(() => fixCoinbase({ markets: [{ id: 'LTC-USD', active: true }] }, records), /unexpected TS/);
  const invalid = structuredClone(records);
  invalid[1].body = JSON.stringify({ products: [{ ...rows[0], is_disabled: 'false' }] });
  assert.throws(() => fixCoinbase({ markets: [{ id: 'BTC-USD', active: false }] }, invalid), /disabled flag/);
});

test('successful cancellation has a void result without accepting an unrelated error', () => {
  const records = [
    meta,
    request('cancelOrders', { ids: ['12'] }),
    http(
      '/orders/batch_cancel',
      {
        results: [{ order_id: '12', success: true }],
      },
      { method: 'POST', requestBody: JSON.stringify({ order_ids: ['12'] }) },
    ),
  ];
  const ts = { commands: [{ ok: [] }] };
  fixCoinbase(ts, records);
  assert.deepEqual(ts.commands, [{ ok: null }]);
  assert.throws(() => fixCoinbase({ commands: [{ error: 'other failure' }] }, records), /unexpected TS/);
});

test('repeated same-symbol reads select exact ranges limits family and order filters', () => {
  const from = Date.parse('2025-12-15T12:08:00.000Z');
  const first = request('fetchFills', { symbol: 'BTC-USD', from, to: from + 100, limit: 1, orderId: 'first' });
  const second = request('fetchFills', {
    symbol: 'BTC-USD',
    from: from + 1,
    to: from + 200,
    limit: 2,
    orderId: 'second',
  });
  const url = (c, family = 'SPOT') =>
    `/orders/historical/fills?limit=${c.limit}&sort_by=TRADE_TIME&product_types=${family}&product_ids=BTC-USD&order_ids=${c.orderId}&start_sequence_timestamp=${encodeURIComponent(new Date(c.from - 1).toISOString())}&end_sequence_timestamp=${encodeURIComponent(new Date(c.to + 1).toISOString())}`;
  const firstRow = { ...fill, trade_id: 'first-fill', order_id: 'first' };
  const secondRow = { ...fill, trade_id: 'second-fill', order_id: 'second' };
  const records = [
    meta,
    http('/products', { products: [market] }),
    first,
    http(url(first), { fills: [firstRow], cursor: '' }, { afterCommand: 1 }),
    second,
    http(url(second), { fills: [secondRow], cursor: '' }, { afterCommand: 2 }),
    http(url(second, 'FUTURE'), { fills: [{ ...fill, trade_id: 'wrong-family' }], cursor: '' }, { afterCommand: 2 }),
  ];
  const ts = { commands: [{ ok: [] }, { ok: [] }], requests: [] };
  fixCoinbase(ts, records);
  assert.deepEqual(
    ts.commands.map((r) => r.ok.map((r) => r.id)),
    [['first-fill'], ['second-fill']],
  );
  const missing = structuredClone(records);
  missing.splice(5, 1);
  assert.throws(() => fixCoinbase({ commands: [{ ok: [] }, { ok: [] }] }, missing), /missing Coinbase documented/);
});

test('USDC alias queries match native USD routes while preserving requested output symbol', () => {
  const records = [
    meta,
    http('/products', { products: [market] }),
    request('fetchFills', { symbol: 'BTC-USDC', limit: 1 }),
    http(
      '/orders/historical/fills?limit=1&sort_by=TRADE_TIME&product_types=SPOT&product_ids=BTC-USD',
      { fills: [fill], cursor: '' },
      { afterCommand: 1 },
    ),
    request('fetchPublicTrades', { symbol: 'BTC-USDC', limit: 1 }),
    http(
      '/market/products/BTC-USD/ticker?limit=1',
      {
        trades: [
          {
            trade_id: 'public-one',
            product_id: 'BTC-USD',
            price: '100',
            size: '1',
            side: 'SELL',
            time: fill.trade_time,
          },
        ],
      },
      { afterCommand: 2 },
    ),
  ];
  const ts = { commands: [{ ok: [] }, { error: 'Not implemented' }], requests: [] };
  fixCoinbase(ts, records);
  assert.equal(ts.commands[0].ok[0].symbol, 'BTC-USDC');
  assert.equal(ts.commands[1].ok[0].symbol, 'BTC-USDC');
  assert.equal(ts.commands[1].ok[0].side, 'buy');
  assert.ok(ts.requests.every((r) => !r.includes('BTC-USDC')));
});

test('symbol-free CFM queries read both exact future and spot families', () => {
  const future = {
    ...market,
    product_id: 'BTC-CDE',
    product_type: 'FUTURE',
    future_product_details: { contract_expiry_type: 'EXPIRING', contract_size: '0.01' },
  };
  const records = [
    { ...meta, profile: 'cfm' },
    http('/products', { products: [market, future] }),
    request('fetchFills', { limit: 2 }),
    http('/orders/historical/fills?limit=2&sort_by=TRADE_TIME&product_types=FUTURE', {
      fills: [{ ...fill, product_id: 'BTC-CDE', trade_id: 'future' }],
      cursor: '',
    }),
    http('/orders/historical/fills?limit=2&sort_by=TRADE_TIME&product_types=SPOT', { fills: [fill], cursor: '' }),
  ];
  const ts = { commands: [{ ok: [] }], requests: [] };
  fixCoinbase(ts, records);
  assert.deepEqual(new Set(ts.commands[0].ok.map((r) => r.id)), new Set(['future', 't1']));
  assert.equal(ts.requests.length, 2);
});
