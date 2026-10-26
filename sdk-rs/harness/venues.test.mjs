import { test } from 'node:test';
import assert from 'node:assert/strict';
import { createHmac } from 'node:crypto';
import { venueOf } from './venues.mjs';
import { subscriptionKeys } from './requests.mjs';
const venue = venueOf('cryptocom');
const privateUrl = 'wss://stream.crypto.com/exchange/v1/user';
const publicUrl = 'wss://stream.crypto.com/exchange/v1/market';
const auth = () => {
  const method = 'public/auth',
    id = Date.now(),
    nonce = id,
    api_key = 'replay-key';
  const sig = createHmac('sha256', 'replay-secret')
    .update(method + id + api_key + nonce)
    .digest('hex');
  return { method, id, nonce, api_key, sig };
};
const subscriptions = ['user.order', 'user.advance.order', 'user.positions', 'user.balance', 'user.trade'];
test('HMAC check binds the signed method to the REST route', () => {
  const body = (method) => {
    const id = Date.now(),
      nonce = id,
      api_key = 'replay-key',
      params = { client_oid: 'caller' };
    const sig = createHmac('sha256', 'replay-secret')
      .update(method + id + api_key + 'client_oidcaller' + nonce)
      .digest('hex');
    return JSON.stringify({ method, id, nonce, api_key, params, sig });
  };
  const url = 'https://api.crypto.com/exchange/v1/private/create-order';
  assert.equal(venue.checkRequest('POST', url, body('private/create-order')), true);
  assert.equal(venue.checkRequest('POST', url, body('private/wrong-method')), false);
  assert.equal(venue.checkRequest('GET', url, body('private/create-order')), false);
});
test('socket audit checks auth, subscriptions and heartbeat identity', () => {
  const audit = venue.createSocketAudit('rust', { isPrivate: true, books: ['BTCUSD-PERP'] });
  audit.sent(privateUrl, JSON.stringify(auth()));
  audit.received(privateUrl, JSON.stringify({ method: 'public/auth', code: 0 }));
  audit.sent(privateUrl, JSON.stringify({ method: 'subscribe', params: { channels: subscriptions } }));
  audit.sent(publicUrl, JSON.stringify({ method: 'subscribe', params: { channels: ['book.BTCUSD-PERP.50'] } }));
  audit.received(privateUrl, JSON.stringify({ method: 'public/heartbeat', id: 123 }));
  audit.sent(privateUrl, JSON.stringify({ method: 'public/respond-heartbeat', id: 123 }));
  assert.deepEqual(audit.errors(), []);
});
test('bad signatures and missing channels cannot pass the socket audit', () => {
  const audit = venue.createSocketAudit('rust', { isPrivate: true, books: [] });
  audit.sent(privateUrl, JSON.stringify({ ...auth(), sig: 'invalid' }));
  audit.sent(privateUrl, JSON.stringify({ method: 'subscribe', params: { channels: subscriptions.slice(0, -1) } }));
  assert(audit.errors().some((e) => e.includes('signature')));
  assert(audit.errors().some((e) => e.includes('user.trade')));
});

test('HMAC check preserves exact nanosecond integer parameters', () => {
  const method = 'private/get-trades',
    id = Date.now(),
    nonce = id,
    api_key = 'replay-key';
  const payload = method + id + api_key + 'end_time1610000000000000001' + nonce;
  const sig = createHmac('sha256', 'replay-secret').update(payload).digest('hex');
  const body = JSON.stringify({ method, id, nonce, api_key, sig, params: {} }).replace(
    '"params":{}',
    '"params":{"end_time":1610000000000000001}',
  );
  const url = 'https://api.crypto.com/exchange/v1/private/get-trades';
  assert.equal(venue.checkRequest('POST', url, body), true);
  assert.equal(venue.checkRequest('POST', url, body.replace('1610000000000000001', '1610000000000000002')), false);
});

test('public audit requires ticker subscriptions before replay', () => {
  const audit = venue.createSocketAudit('rust', {
    isPrivate: false,
    books: ['BTCUSD-PERP'],
    tickers: ['BTCUSD-PERP', 'ETHUSD-PERP'],
  });
  const send = (channels) => audit.sent(publicUrl, JSON.stringify({ method: 'subscribe', params: { channels } }));
  send(['book.BTCUSD-PERP.50']);
  assert.equal(audit.ready(), false);
  assert(audit.errors().some((e) => e.includes('missing ticker subscription')));
  const ready = venue.createSocketAudit('rust', { isPrivate: false, books: [], tickers: ['BTCUSD-PERP'] });
  ready.sent(publicUrl, JSON.stringify({ method: 'subscribe', params: { channels: ['ticker.BTCUSD-PERP'] } }));
  assert.equal(ready.ready(), true);
  assert.deepEqual(ready.errors(), []);
});

test('Bybit startup topics derive all successful seed pages independently of sent subscriptions', async () => {
  const { subscriptionKeys } = await import('./requests.mjs');
  const bybit = venueOf('bybit');
  const row = (symbol, quoteCoin) => ({ symbol, quoteCoin });
  const seed = (list, extra = {}) => ({
    type: 'http',
    method: 'GET',
    status: 200,
    url: 'https://api.bybit.com/v5/market/instruments-info?category=linear&limit=1000',
    body: JSON.stringify({ retCode: 0, result: { list } }),
    ...extra,
  });
  const required = bybit.startupSubscriptions(
    [
      seed([row('BTCUSDT', 'USDT'), row('BTCUSD', 'USD'), row('BTCUSDT', 'USDT')]),
      seed([row('ETHUSDC', 'USDC')], { url: 'https://api.bybit.com/v5/market/instruments-info?cursor=page2' }),
      seed([row('NEXTUSDT', 'USDT')], { url: 'https://api.bybit.com/v5/market/instruments-info?status=PreLaunch' }),
      seed([row('FAILEDUSDT', 'USDT')], { status: 503 }),
      seed([row('FAILEDUSDT', 'USDT')], {
        body: JSON.stringify({ retCode: 10001, result: { list: [row('FAILEDUSDT', 'USDT')] } }),
      }),
      seed([row('OTHERUSDT', 'USDT')], { url: 'https://api.bybit.com/v5/market/tickers' }),
    ],
    ['BTCUSDT', 'ETHUSDC'],
  );
  const socket = 'wss://stream.bybit.com/v5/public/linear';
  assert.deepEqual(required, [
    `WS ${socket}? {"args":["tickers.BTCUSDT"],"op":"subscribe"}`,
    `WS ${socket}? {"args":["tickers.ETHUSDC"],"op":"subscribe"}`,
    `WS ${socket}? {"args":["tickers.NEXTUSDT"],"op":"subscribe"}`,
    `WS ${socket}? {"args":["orderbook.1000.BTCUSDT"],"op":"subscribe"}`,
    `WS ${socket}? {"args":["orderbook.1000.ETHUSDC"],"op":"subscribe"}`,
  ]);
  const pending = new Set(required);
  const sent = (url, args) => {
    for (const key of subscriptionKeys(url, JSON.stringify({ op: 'subscribe', args }))) pending.delete(key);
  };
  sent('wss://stream.bybit.com/v5/private', ['tickers.BTCUSDT']);
  assert.equal(pending.size, 5, 'wrong native socket cannot satisfy readiness');
  sent(socket + '?requestId=generated', ['tickers.BTCUSDT', 'orderbook.1000.BTCUSDT']);
  assert.equal(pending.size, 3, 'a partial outgoing batch cannot satisfy readiness');
  sent(socket, ['tickers.ETHUSDC', 'tickers.NEXTUSDT', 'orderbook.1000.ETHUSDC']);
  assert.equal(pending.size, 0);
});

test('unrecorded economic websocket methods fail the Crypto.com protocol audit', () => {
  const audit = venue.createSocketAudit('rust', { isPrivate: false, books: [] });
  audit.sent(privateUrl, JSON.stringify({ id: 1, method: 'private/create-order', params: { quantity: '1' } }));
  assert(audit.errors().some((error) => error.includes('unexpected outbound WS method private/create-order')));
});

test('WEEX signed REST and private upgrade headers bind path, body and account', () => {
  const weex = venueOf('weex');
  const time = String(Date.now());
  const method = 'POST';
  const url = 'https://api-contract.weex.com/capi/v3/account/income';
  const body = '{"symbol":"BTCUSDT"}';
  const headers = {
    'ACCESS-TIMESTAMP': time,
    'ACCESS-KEY': 'replay-key',
    'ACCESS-PASSPHRASE': 'replay-passphrase',
    'ACCESS-SIGN': createHmac('sha256', 'replay-secret')
      .update(`${time}${method}/capi/v3/account/income${body}`)
      .digest('base64'),
  };
  assert.doesNotThrow(() => weex.validateRequest({ method, url, body, headers }));
  assert.throws(() => weex.validateRequest({ method, url, body: '{}', headers }));
  assert.throws(() => weex.validateRequest({ method, url, body, headers: { ...headers, 'ACCESS-KEY': 'another' } }));
  const privateUrl = 'wss://ws-contract.weex.com/v3/ws/private';
  const upgrade = {
    ...headers,
    'ACCESS-SIGN': createHmac('sha256', 'replay-secret').update(`${time}/v3/ws/private`).digest('base64'),
  };
  assert.doesNotThrow(() => weex.validateSocket(privateUrl, upgrade));
  assert.throws(() => weex.validateSocket(privateUrl, { ...upgrade, 'ACCESS-SIGN': 'invalid' }));
  assert.equal(weex.isWrite(method, url), false);
  assert.equal(weex.isWrite('POST', 'https://api-contract.weex.com/capi/v3/algoOrder'), true);
});

for (const [label, timestamp] of [
  ['missing', undefined],
  ['empty', ''],
  ['nonnumeric', 'undefined'],
  ['fractional', '1800000000000.5'],
  ['unsafe integer', '9007199254740992'],
  ['stale', '1799999969999'],
  ['future', '1800000030001'],
]) {
  for (const transport of ['REST', 'upgrade']) {
    test(`WEEX ${transport} rejects ${label} timestamps even with a matching signature`, () => {
      const weex = venueOf('weex');
      const now = 1800000000000;
      const headers = {
        'ACCESS-KEY': 'replay-key',
        'ACCESS-PASSPHRASE': 'replay-passphrase',
        ...(timestamp === undefined ? {} : { 'ACCESS-TIMESTAMP': timestamp }),
      };
      const method = 'POST',
        path = '/capi/v3/account/income',
        body = '{}';
      const payload = transport === 'REST' ? `${timestamp}${method}${path}${body}` : `${timestamp}/v3/ws/private`;
      headers['ACCESS-SIGN'] = createHmac('sha256', 'replay-secret').update(payload).digest('base64');
      if (transport === 'REST') {
        assert.throws(() =>
          weex.validateRequest({ method, url: 'https://api-contract.weex.com' + path, body, headers, now }),
        );
      } else {
        assert.throws(() => weex.validateSocket('wss://ws-contract.weex.com/v3/ws/private', headers, now));
      }
    });
  }
}

test('WEEX timestamp freshness accepts both edges of the replay clock window', () => {
  const weex = venueOf('weex'),
    now = 1800000000000;
  for (const offset of [-30000, 0, 30000]) {
    const timestamp = String(now + offset);
    const headers = {
      'ACCESS-TIMESTAMP': timestamp,
      'ACCESS-KEY': 'replay-key',
      'ACCESS-PASSPHRASE': 'replay-passphrase',
    };
    const method = 'POST',
      path = '/capi/v3/account/income',
      body = '{}';
    headers['ACCESS-SIGN'] = createHmac('sha256', 'replay-secret')
      .update(`${timestamp}${method}${path}${body}`)
      .digest('base64');
    assert.doesNotThrow(() =>
      weex.validateRequest({ method, url: 'https://api-contract.weex.com' + path, body, headers, now }),
    );
    headers['ACCESS-SIGN'] = createHmac('sha256', 'replay-secret')
      .update(`${timestamp}/v3/ws/private`)
      .digest('base64');
    assert.doesNotThrow(() => weex.validateSocket('wss://ws-contract.weex.com/v3/ws/private', headers, now));
  }
});

test('Bybit startup topics include all successful stablecoin pages and requested books', () => {
  const bybit = venueOf('bybit');
  const seed = (list, extra = {}) => ({
    type: 'http',
    method: 'GET',
    status: 200,
    url: 'https://api.bybit.com/v5/market/instruments-info?category=linear',
    body: JSON.stringify({ retCode: 0, result: { list } }),
    ...extra,
  });
  const rows = [
    seed([
      { symbol: 'BTCUSDT', quoteCoin: 'USDT' },
      { symbol: 'BTCUSD', quoteCoin: 'USD' },
    ]),
    seed([{ symbol: 'ETHUSDC', quoteCoin: 'USDC' }], {
      url: 'https://api.bybit.com/v5/market/instruments-info?cursor=next',
    }),
    seed(
      [
        { symbol: 'NEWUSDT', quoteCoin: 'USDT' },
        { symbol: 'BTCUSDT', quoteCoin: 'USDT' },
      ],
      { url: 'https://api.bybit.com/v5/market/instruments-info?category=linear&status=PreLaunch' },
    ),
    seed([{ symbol: 'FAILEDUSDT', quoteCoin: 'USDT' }], { status: 503 }),
    seed([], { body: '{"retCode":10001,"result":{"list":[{"symbol":"FAILEDUSDT","quoteCoin":"USDT"}]}}' }),
    seed([{ symbol: 'SPOTUSDT', quoteCoin: 'USDT' }], {
      url: 'https://api.bybit.com/v5/market/instruments-info?category=spot',
    }),
  ];
  const url = 'wss://stream.bybit.com/v5/public/linear?requestId=taped&max_active_time=1m';
  rows.push({ type: 'ws', url });
  const expected = bybit.startupSubscriptions(rows, ['BTCUSDT', 'ETHUSDC']);
  assert.deepEqual(
    expected,
    subscriptionKeys(url, {
      op: 'subscribe',
      args: [
        'tickers.BTCUSDT',
        'tickers.ETHUSDC',
        'tickers.NEWUSDT',
        'orderbook.1000.BTCUSDT',
        'orderbook.1000.ETHUSDC',
      ],
    }),
  );
  const pending = new Map(expected.map((key) => [key, false]));
  const sent = (socket, topics) => {
    for (const key of subscriptionKeys(socket, { op: 'subscribe', args: topics }))
      if (pending.has(key)) pending.set(key, true);
  };
  const ready = () => [...pending.values()].every(Boolean);
  sent(url.replace('requestId=taped', 'requestId=generated'), ['orderbook.1000.BTCUSDT', 'orderbook.1000.ETHUSDC']);
  assert.equal(ready(), false, 'books alone do not drain paced ticker sends');
  sent('wss://stream.bybit.com/v5/private', ['tickers.BTCUSDT', 'tickers.ETHUSDC', 'tickers.NEWUSDT']);
  assert.equal(ready(), false);
  sent(url, ['tickers.BTCUSDT', 'tickers.ETHUSDC']);
  assert.equal(ready(), false);
  sent(url, ['tickers.NEWUSDT']);
  assert.equal(ready(), true);
  assert.equal(pending.size, 5, 'retain startup backing storage across memory samples');
  assert.equal(bybit.startupSubscriptions(rows, []).length, 3, 'no books still requires every ticker');
});

test('BloFin login checks account, fresh timestamp, nonce and hex-base64 signature before subscriptions', () => {
  const venue = venueOf('blofin');
  const url = 'wss://openapi.blofin.com/ws/private';
  const now = Date.now();
  const message = (patch = {}) => {
    const arg = {
      apiKey: 'replay-key',
      passphrase: 'replay-passphrase',
      timestamp: now,
      nonce: 'fresh-nonce',
      ...patch,
    };
    arg.sign ??= Buffer.from(
      createHmac('sha256', 'replay-secret').update(`/users/self/verifyGET${arg.timestamp}${arg.nonce}`).digest('hex'),
    ).toString('base64');
    return JSON.stringify({ op: 'login', args: [arg] });
  };
  const socket = { url };
  const feeds = '{"op":"subscribe","args":[{"channel":"orders","instType":"SWAP","instId":"ANY"}]}';
  assert.throws(() => venue.validateSocketMessage(socket, feeds, now));
  assert.throws(() => venue.validateSocketFrame(socket, '{"event":"login","code":"0"}'));
  for (const patch of [
    { sign: 'wrong' },
    { apiKey: 'other' },
    { passphrase: 'other' },
    { nonce: '' },
    { timestamp: now - 30001 },
    { timestamp: now + 30001 },
    { timestamp: 'bad' },
  ]) {
    assert.throws(() => venue.validateSocketMessage({ url }, message(patch), now));
  }
  venue.validateSocketMessage(socket, message(), now);
  assert.throws(() => venue.validateSocketMessage(socket, feeds, now));
  venue.validateSocketFrame(socket, '{"event":"login","code":"0"}');
  venue.validateSocketMessage(socket, feeds, now);
});

test('Bybit variant startup topics keep native categories, depths and configured origins', () => {
  for (const [name, symbol, quoteCoin, category, depth, origin] of [
    ['bybitinverse', 'BTCUSD', 'USD', 'inverse', 1000, 'wss://stream.bybit.com'],
    ['bybitspot', 'BTCUSDT', 'USDT', 'spot', 200, 'wss://stream.bybit.com'],
    ['bybitdemo', 'BTCUSDT', 'USDT', 'linear', 1000, 'wss://stream.bybit.com'],
    ['hyrotrader', 'BTCUSDT', 'USDT', 'linear', 1000, 'wss://challenge.example.test'],
  ]) {
    const publicWs = `${origin}/v5/public/${category}`;
    const records = [
      {
        type: 'http',
        method: 'GET',
        status: 200,
        url: `https://example.test/v5/market/instruments-info?category=${category}`,
        body: JSON.stringify({ retCode: 0, result: { list: [{ symbol, quoteCoin }] } }),
      },
      { type: 'ws', url: `${publicWs}?requestId=generated`, text: '{}' },
    ];
    if (name === 'hyrotrader') records.unshift({ type: 'config', options: { endpoints: { publicWs } } });
    assert.deepEqual(
      venueOf(name).startupSubscriptions(records, [symbol]),
      [
        `WS ${publicWs}? {"args":["tickers.${symbol}"],"op":"subscribe"}`,
        `WS ${publicWs}? {"args":["orderbook.${depth}.${symbol}"],"op":"subscribe"}`,
      ],
      name,
    );
  }
});


const phemexUrl = 'wss://ws.phemex.com';
const phemexAuth = () => {
  const expiry = Math.round(Date.now() / 1000 + 60);
  return JSON.stringify({ id: 999, method: 'user.auth', params: [
    'API', 'replay-key', createHmac('sha256', 'replay-secret').update('replay-key' + expiry).digest('hex'), expiry,
  ] });
};
const phemexAck = JSON.stringify({ id: 999, error: null, result: { status: 'success' } });
const phemexSubscribe = JSON.stringify({ method: 'aop_p.subscribe', params: [] });
const phemexAudit = () => {
  const audit = venueOf('phemex').createSocketAudit('rust', { isPrivate: true, books: ['BTCUSDT'] });
  audit.connected?.(phemexUrl, 1);
  audit.sent(phemexUrl, JSON.stringify({ method: 'perp_market24h_pack_p.subscribe', params: [] }), 1);
  audit.sent(phemexUrl, JSON.stringify({ method: 'orderbook_p.subscribe', params: ['BTCUSDT', false, 0] }), 1);
  audit.connected?.(phemexUrl, 2);
  return audit;
};

test('Phemex valid bootstrap authenticates and subscribes the private physical socket', () => {
  const audit = phemexAudit();
  audit.sent(phemexUrl, phemexAuth(), 2);
  assert.equal(audit.ready(), false);
  audit.received(phemexUrl, phemexAck, 2);
  assert.equal(audit.ready(), false, 'authentication does not imply account subscription');
  audit.sent(phemexUrl, phemexSubscribe, 2);
  assert.equal(audit.ready(), true);
  assert.deepEqual(audit.errors(), []);
});

test('Phemex a new auth attempt clears the prior ACK and subscription', () => {
  const audit = phemexAudit();
  audit.sent(phemexUrl, phemexAuth(), 2);
  audit.received(phemexUrl, phemexAck, 2);
  audit.sent(phemexUrl, phemexSubscribe, 2);
  assert.equal(audit.ready(), true);
  audit.sent(phemexUrl, phemexAuth(), 2);
  assert.equal(audit.ready(), false);
  audit.sent(phemexUrl, phemexSubscribe, 2);
  assert.match(audit.errors().join(), /preceded auth acknowledgement/);
  audit.received(phemexUrl, phemexAck, 2);
  assert.equal(audit.ready(), false, 'a premature subscription cannot satisfy the new attempt');
  audit.sent(phemexUrl, phemexSubscribe, 2);
  assert.equal(audit.ready(), true);
});

test('Phemex reconnect cannot reuse the old generation ACK on the shared public/private URL', () => {
  const audit = phemexAudit();
  audit.sent(phemexUrl, phemexAuth(), 2);
  audit.received(phemexUrl, phemexAck, 2);
  audit.sent(phemexUrl, phemexSubscribe, 2);
  audit.closed?.(phemexUrl, 2);
  assert.equal(audit.ready(), false);
  audit.connected?.(phemexUrl, 3);
  audit.sent(phemexUrl, phemexAuth(), 3);
  audit.received(phemexUrl, phemexAck, 2);
  audit.sent(phemexUrl, phemexSubscribe, 3);
  assert.equal(audit.ready(), false);
  assert.match(audit.errors().join(), /preceded auth acknowledgement/);
  audit.received(phemexUrl, phemexAck, 3);
  assert.equal(audit.ready(), false);
  audit.sent(phemexUrl, phemexSubscribe, 3);
  assert.equal(audit.ready(), true);
});

for (const ack of [
  { id: 999, error: null },
  { id: 999, error: null, result: {} },
  { id: 999, error: null, result: { status: 'failed' } },
  { id: 999, error: { code: 401 }, result: { status: 'success' } },
  { id: 998, error: null, result: { status: 'success' } },
]) {
  test(`Phemex unconfirmed auth ACK cannot authorize private subscriptions: ${JSON.stringify(ack)}`, () => {
    const audit = phemexAudit();
    audit.sent(phemexUrl, phemexAuth(), 2);
    audit.received(phemexUrl, JSON.stringify(ack), 2);
    audit.sent(phemexUrl, phemexSubscribe, 2);
    assert.equal(audit.ready(), false);
    assert.match(audit.errors().join(), /auth/);
  });
}

test('Phemex successful-looking ACK on a socket without auth cannot authorize another generation', () => {
  const audit = phemexAudit();
  audit.sent(phemexUrl, phemexAuth(), 2);
  audit.received(phemexUrl, phemexAck, 1);
  audit.sent(phemexUrl, phemexSubscribe, 2);
  assert.equal(audit.ready(), false);
  assert.match(audit.errors().join(), /ACK without request/);
});
