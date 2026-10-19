import assert from 'node:assert/strict';
import { createHmac, createPrivateKey, sign } from 'node:crypto';
import { test } from 'node:test';
import {
  bodyBytes,
  replayStoreReady,
  replayBooksReady,
  ReplayClock,
  canonicalBody,
  canonicalUrl,
  sameCommandResults,
  Tape,
  withTimeout,
} from './tape.mjs';
import { venueOf } from './venues.mjs';
const URL = 'https://example.test/order';
const record = (method, requestBody) => ({
  type: 'http',
  method,
  url: URL,
  status: 200,
  body: '{}',
  ...(requestBody === undefined ? {} : { requestBody: JSON.stringify(requestBody) }),
});
test('writes are consumed once and missing writes remain visible; GET seeds repeat', () => {
  const tape = new Tape([record('POST', { quantity: '1.00' }), record('GET')], {}, false);
  assert.equal(tape.missingWrites, 1);
  assert.equal(tape.respond('POST', URL, JSON.stringify({ quantity: '1' })).status, 200);
  assert.equal(tape.missingWrites, 0);
  assert.equal(tape.respond('POST', URL, JSON.stringify({ quantity: '1' })).status, 599);
  assert.equal(tape.unserved, 1);
  assert.equal(tape.respond('GET', URL).status, 200);
  assert.equal(tape.respond('GET', URL).status, 200);
});
test('body matching preserves wire types, exact decimals and caller IDs', () => {
  assert.equal(canonicalBody({ quantity: '1.2300' }), canonicalBody({ quantity: '1.23' }));
  assert.notEqual(canonicalBody({ quantity: '1' }), canonicalBody({ quantity: 1 }));
  assert.notEqual(canonicalBody({ price: '9007199254740992.00' }), canonicalBody({ price: '9007199254740993.00' }));
  assert.notEqual(canonicalBody({ orderId: '9007199254740992' }), canonicalBody({ orderId: '9007199254740993' }));
  const caller = new Set(['42']);
  assert.notEqual(canonicalBody({ clientId: 42 }, caller), canonicalBody({ clientId: 43 }, caller));
  assert.notEqual(canonicalBody({ clientId: 42 }, caller), canonicalBody({ clientId: '42' }, caller));
});
test('two sockets at one URL route by actual subscription; wrong topic times out', async () => {
  const tape = new Tape([], { streamOf: (text) => JSON.parse(text).stream }, false, 200);
  const one = tape.connect(
    'wss://example.test',
    () => {},
    () => {},
  );
  const two = tape.connect(
    'wss://example.test',
    () => {},
    () => {},
  );
  tape.send(one, JSON.stringify({ method: 'SUBSCRIBE', params: ['ticker.SOL_USDC_PERP'] }));
  tape.send(two, JSON.stringify({ method: 'SUBSCRIBE', params: ['account.orderUpdate'] }));
  assert.equal(await tape.socketFor('wss://example.test', JSON.stringify({ stream: 'account.orderUpdate' })), two);
  assert.equal(await tape.socketFor('wss://example.test', JSON.stringify({ stream: 'account.orderUpdate' }), 1), two);
  await assert.rejects(
    tape.socketFor('wss://example.test', JSON.stringify({ stream: 'account.orderUpdate' }), 0),
    /timeout/,
  );
  await assert.rejects(tape.socketFor('wss://example.test', JSON.stringify({ stream: 'depth.missing' })), /timeout/);
  await assert.rejects(withTimeout(new Promise(() => {}), 'readiness', 1), /readiness/);
});
test('independent ED25519 verification rejects changed REST bodies and bad WS auth', () => {
  const venue = venueOf('backpack');
  const seed = Buffer.from(venue.replayCredentials.secret, 'base64');
  const privateKey = createPrivateKey({
    key: Buffer.concat([Buffer.from('302e020100300506032b657004220420', 'hex'), seed]),
    format: 'der',
    type: 'pkcs8',
  });
  const signature = sign(
    null,
    Buffer.from(
      'instruction=orderExecute&orderType=Market&quantity=1.00&side=Bid&symbol=SOL_USDC_PERP&timestamp=1000&window=60000',
    ),
    privateKey,
  ).toString('base64');
  const request = {
    method: 'POST',
    url: 'https://api.backpack.exchange/api/v1/order',
    headers: {
      'X-API-Key': venue.replayCredentials.key,
      'X-Broker-Id': '1200',
      'X-Signature': signature,
      'X-Timestamp': '1000',
      'X-Window': '60000',
    },
    body: JSON.stringify({ orderType: 'Market', quantity: '1.00', side: 'Bid', symbol: 'SOL_USDC_PERP' }),
  };
  assert.equal(venue.verifyRequest(request), true);
  assert.equal(venue.verifyRequest({ ...request, body: request.body.replace('1.00', '2.00') }), false);
  const auth = {
    method: 'SUBSCRIBE',
    params: ['account.orderUpdate'],
    signature: [
      venue.replayCredentials.key,
      sign(null, Buffer.from('instruction=subscribe&timestamp=1000&window=5000'), privateKey).toString('base64'),
      '1000',
      '5000',
    ],
  };
  assert.equal(venue.verifySubscribe(auth), true);
  assert.equal(venue.verifySubscribe({ ...auth, signature: [] }), false);
  const tape = new Tape([], venue, true);
  const socket = tape.connect(
    'wss://ws.backpack.exchange',
    () => {},
    () => {},
  );
  assert.throws(() => tape.send(socket, JSON.stringify({ ...auth, signature: [] })), /signature/);
  assert.equal(socket.subscriptions.size, 0);
});

test('Backpack rejects replacement before its recorded cancellation', () => {
  const tape = new Tape(
    [record('DELETE', { orderId: '1' }), record('POST', { quantity: '1' })],
    { compareWriteOrder: true },
    false,
  );
  assert.equal(tape.respond('POST', URL, JSON.stringify({ quantity: '1' })).status, 599);
  assert.equal(tape.missingWrites, 2);
  assert.equal(tape.respond('DELETE', URL, JSON.stringify({ orderId: '1' })).status, 200);
  assert.equal(tape.respond('POST', URL, JSON.stringify({ quantity: '1' })).status, 200);
  assert.equal(tape.missingWrites, 0);
});

test('public candle command mismatches are checked', () => {
  const ts = [{ ok: [{ timestamp: 1000, open: 1, high: 2, low: 1, close: 2, volume: 5 }] }];
  assert.equal(sameCommandResults(ts, structuredClone(ts)), true);
  assert.equal(sameCommandResults(ts, [{ ok: [] }]), false);
  assert.equal(sameCommandResults(ts, [{ error: 'unavailable' }]), false);
});

test('declared TS wire defect does not relax the Rust request body', () => {
  const row = {
    ...record('POST', { orderType: 'Market', stopLossTriggerPrice: '120' }),
    tsRequestBody: JSON.stringify({ orderType: 'Market', stopLossTriggerPrice: '120', stopLossLimitPrice: '120' }),
  };
  const rust = new Tape([row], {}, false, 30000, 'rust');
  assert.equal(rust.respond('POST', URL, row.tsRequestBody).status, 599);
  assert.equal(rust.respond('POST', URL, row.requestBody).status, 200);
  const ts = new Tape([row], {}, false, 30000, 'ts');
  assert.equal(ts.respond('POST', URL, row.tsRequestBody).status, 200);
});

test('Bybit per-connection requestId is volatile for HTTP and sockets', async () => {
  const response = { ...record('GET'), url: 'https://example.test/order?symbol=BTCUSDT&requestId=recorded' };
  const tape = new Tape([response], {}, false, 200);
  assert.equal(tape.respond('GET', 'https://example.test/order?requestId=live&symbol=BTCUSDT').status, 200);
  const socket = tape.connect(
    'wss://example.test/v5/public/linear?requestId=live',
    () => {},
    () => {},
  );
  assert.equal(await tape.socketFor('wss://example.test/v5/public/linear?requestId=recorded'), socket);
});

test('staged reads advance only after their command and preserve response headers', () => {
  const early = { ...record('GET'), body: 'early' };
  const later = { ...record('GET'), body: 'later', afterCommand: 1, headers: 'x-stage: later' };
  const write = { ...record('POST', { quantity: '2' }), afterCommand: 1 };
  const tape = new Tape([early, later, write], {}, false);
  assert.equal(tape.respond('GET', URL).body, 'early');
  assert.equal(tape.respond('POST', URL, write.requestBody).status, 599);
  assert.equal(tape.missingWrites, 1);
  tape.commandIndex = 1;
  assert.deepEqual(tape.respond('GET', URL), { status: 200, headers: 'x-stage: later', body: 'later' });
  assert.equal(tape.respond('POST', URL, write.requestBody).status, 200);
  tape.assertWritesConsumed();
});

test('OKX generated IDs normalize without weakening caller IDs or wire types', () => {
  const venue = {
    matchRequestBody: true,
    generatedBodyId: (key, value) => key === 'clOrdId' && value.startsWith('broker'),
  };
  assert.equal(
    canonicalBody({ clOrdId: 'broker-one', sz: '1.00' }, new Set(), venue),
    canonicalBody({ clOrdId: 'broker-two', sz: '1' }, new Set(), venue),
  );
  const caller = new Set(['broker-caller']);
  assert.notEqual(
    canonicalBody({ clOrdId: 'broker-caller' }, caller, venue),
    canonicalBody({ clOrdId: 'broker-other' }, caller, venue),
  );
  assert.notEqual(canonicalBody({ sz: '1' }, caller, venue), canonicalBody({ sz: 1 }, caller, venue));
  assert.equal(sameCommandResults([{ ok: { b: 2, a: 1 } }], [{ ok: { a: 1, b: 2 } }]), true);
  assert.equal(sameCommandResults([{ ok: [1, 2] }], [{ ok: [2, 1] }]), false);
});

test('OKX login, subscription coverage and ping behavior survive shared Tape extraction', async () => {
  const venue = { validateLogin: true, respondToPing: true };
  const tape = new Tape([], venue, true);
  const messages = [];
  const socket = tape.connect(
    'wss://example.test/ws/v5/private',
    () => {},
    (text) => messages.push(text),
  );
  const timestamp = String(Math.floor(Date.now() / 1000));
  const arg = {
    apiKey: 'replay-key',
    passphrase: 'replay-passphrase',
    timestamp,
    sign: createHmac('sha256', 'replay-secret').update(`${timestamp}GET/users/self/verify`).digest('base64'),
  };
  assert.throws(
    () => tape.send(socket, JSON.stringify({ op: 'login', args: [{ ...arg, sign: 'invalid' }] })),
    /signature/,
  );
  assert.equal(socket.loginVerified, false);
  tape.send(socket, JSON.stringify({ op: 'login', args: [arg] }));
  assert.equal(socket.loginVerified, true);
  const topic = { channel: 'orders', instType: 'SWAP' };
  tape.send(socket, JSON.stringify({ op: 'subscribe', args: [topic] }));
  tape.assertSubscribed(socket, { arg: { channel: 'orders' }, data: [{ instId: 'BTC-USDT-SWAP' }] });
  assert.throws(
    () => tape.assertSubscribed(socket, { arg: { channel: 'orders' }, data: [{ instId: 'BTC-USDT' }] }),
    /Unsubscribed/,
  );
  tape.send(socket, JSON.stringify({ op: 'unsubscribe', args: [topic] }));
  assert.throws(() => tape.assertSubscribed(socket, { arg: topic, data: [] }), /Unsubscribed/);
  tape.send(socket, 'ping');
  await new Promise(setImmediate);
  assert.deepEqual(messages, ['pong']);
});

test('shared Tape correlates signed envelopes, preserves exact cursors and reuses POST reads', () => {
  const read = 'https://api.example.test/private/get-trades';
  const write = 'https://api.example.test/private/create-order';
  const venue = {
    matchRequestBody: true,
    requestBodyParams: true,
    checkRequest: (_method, _url, body) => JSON.parse(body).sig === 'valid',
    isWrite: (_method, url) => url.endsWith('/create-order'),
  };
  const envelope = (params) => `{"id":123,"nonce":456,"sig":"valid","method":"private/get-trades","params":${params}}`;
  const cursor = '{"end_time":1610000000000000001}';
  const response = (url, requestBody) => ({ ...record('POST'), url, requestBody });
  const tape = new Tape([response(read, cursor), response(write, '{"quantity":"1"}')], venue, true);
  assert.equal(tape.missingWrites, 1);
  assert.equal(tape.respond('POST', read, envelope(cursor)).status, 200);
  assert.equal(tape.respond('POST', read, envelope(cursor)).status, 200);
  assert.equal(tape.respond('POST', read, envelope('{"end_time":1610000000000000002}')).status, 599);
  assert.equal(tape.respond('POST', write, envelope('{"quantity":"1"}').replace('valid', 'invalid')).status, 599);
  assert.equal(tape.respond('POST', write, envelope('{"quantity":"1"}')).status, 200);
  assert.equal(tape.respond('POST', write, envelope('{"quantity":"1"}')).status, 599);
  assert.equal(tape.missingWrites, 0);
  assert.equal(tape.unserved, 3);
});

test('shared Tape invokes socket audit hooks for object subscription params', async () => {
  const events = [];
  const tape = new Tape(
    [],
    {
      createSocketAudit: (impl, { books }) => {
        assert.equal(impl, 'rust');
        assert.deepEqual(books, ['BTCUSD-PERP']);
        return {
          sent: (_url, text) => events.push(['sent', text]),
          received: (_url, text) => events.push(['received', text]),
        };
      },
    },
    true,
    200,
    'rust',
    ['BTCUSD-PERP'],
  );
  const socket = tape.connect(
    'wss://example.test/user',
    () => {},
    () => {},
  );
  const message = JSON.stringify({ method: 'subscribe', params: { channels: ['user.order'] } });
  tape.send(socket, message);
  socket.onMessage('auth-ack');
  assert.deepEqual(events, [
    ['sent', message],
    ['received', 'auth-ack'],
  ]);
  await new Promise(setImmediate);
});

test('timed read templates remain reusable and response IDs echo the actual generated request', () => {
  const read = { ...record('GET'), effectiveAt: 0, body: 'old' };
  const newer = { ...read, effectiveAt: 1000, body: 'new' };
  const write = { ...record('POST', { quantity: '1' }), body: '{"clientOrderId":"$clientAlgoId"}' };
  const tape = new Tape([read, newer, write], { matchRequestBody: true, volatileBodyFields: true }, false);
  assert.equal(tape.respond('GET', URL).body, 'old');
  assert.equal(tape.respond('GET', URL).body, 'old');
  tape.time = 1000;
  assert.equal(tape.respond('GET', URL).body, 'new');
  assert.equal(tape.respond('GET', URL).body, 'new');
  assert.equal(
    tape.respond('POST', URL, '{"quantity":"1","clientAlgoId":"generated"}').body,
    '{"clientOrderId":"generated"}',
  );
  assert.equal(tape.missingWrites, 0);
});

test('volatile ID normalization preserves caller IDs inside bodies and query JSON', () => {
  const caller = new Set(['caller']);
  const venue = { matchRequestBody: true, volatileBodyFields: true };
  assert.notEqual(
    canonicalBody({ newClientOrderId: 'caller' }, caller, venue),
    canonicalBody({ newClientOrderId: 'generated' }, caller, venue),
  );
  const left =
    'https://example.test/order?batchOrders=' + encodeURIComponent('[{"newClientOrderId":"caller","quantity":"1"}]');
  const right =
    'https://example.test/order?batchOrders=' + encodeURIComponent('[{"newClientOrderId":"changed","quantity":"1"}]');
  assert.notEqual(canonicalUrl(left, caller, venue), canonicalUrl(right, caller, venue));
  assert.notEqual(
    canonicalUrl('https://example.test/order?newClientOrderId=caller', caller, venue),
    canonicalUrl('https://example.test/order?newClientOrderId=changed', caller, venue),
  );
});

test('venue request validation sees exact signed headers before matching a response', () => {
  const tape = new Tape(
    [record('GET')],
    {
      validateRequest: ({ headers }) => {
        if (headers?.signature !== 'valid') throw new Error('invalid signature');
      },
    },
    true,
  );
  assert.throws(() => tape.respond('GET', URL, undefined, { signature: 'invalid' }), /invalid signature/);
  assert.equal(tape.respond('GET', URL, undefined, { signature: 'valid' }).status, 200);
});

test('a future-only timed read cannot expose a snapshot before its effective time', () => {
  const future = { ...record('GET'), effectiveAt: 1000, body: 'future' };
  const tape = new Tape([future], {}, false);
  assert.equal(tape.respond('GET', URL).status, 599);
  tape.time = 999;
  assert.equal(tape.respond('GET', URL).status, 599);
  assert.equal(tape.unserved, 2);
  tape.time = 1000;
  assert.equal(tape.respond('GET', URL).body, 'future');
  assert.equal(tape.respond('GET', URL).body, 'future');
  assert.equal(tape.missingWrites, 0);
});

test('an untimed baseline remains available before a later timed snapshot', () => {
  const initial = { ...record('GET'), body: 'initial' };
  const future = { ...record('GET'), effectiveAt: 1000, body: 'future' };
  const tape = new Tape([initial, future], {}, false);
  assert.equal(tape.respond('GET', URL).body, 'initial');
  assert.equal(tape.respond('GET', URL).body, 'initial');
  tape.time = 1000;
  assert.equal(tape.respond('GET', URL).body, 'future');
});

test('BingX request-key hook preserves exact query JSON and single-use writes', async () => {
  const { requestKey } = await import('./requests.mjs');
  const route = 'https://open-api.bingx.com/openApi/swap/v2/trade/order';
  const expected =
    route + '?timestamp=1&quantity=1&clientOrderId=caller&takeProfit=' + encodeURIComponent('{"stopPrice":"2"}');
  const tape = new Tape(
    [
      { type: 'command', orders: [{ clientOrderId: 'caller' }] },
      { ...record('POST'), url: expected },
    ],
    { requestKey: (method, url, body, ids) => requestKey(method, url, body, ids) },
    false,
  );
  const actual = expected.replace('timestamp=1', 'timestamp=2');
  assert.equal(tape.respond('POST', actual.replace('caller', 'changed')).status, 599);
  assert.equal(
    tape.respond('POST', actual.replace(encodeURIComponent('{"stopPrice":"2"}'), encodeURIComponent('{"stopPrice":2}')))
      .status,
    599,
  );
  assert.equal(tape.respond('POST', actual).status, 200);
  assert.equal(tape.respond('POST', actual).status, 599);
  assert.equal(tape.missingWrites, 0);
});

test('recorded socket closes deliver later frames to a new subscribed generation', async () => {
  const url = 'wss://example.test/public';
  const tape = new Tape(
    [
      { type: 'ws', t: 0, url, text: 'first' },
      { type: 'close', t: 1, url },
      { type: 'ws', t: 2, url, text: 'second' },
    ],
    { startupSubscriptions: () => ['wss://example.test/public subscribe tickers.BTC'] },
    false,
    200,
  );
  const messages = [];
  const connect = () => {
    const socket = tape.connect(
      url,
      () => tape.send(socket, '{"op":"subscribe","args":["tickers.BTC"]}'),
      (text) => messages.push(text),
      connect,
    );
    return socket;
  };
  connect();
  assert.equal(await tape.play(), 3);
  assert.deepEqual(messages, ['first', 'second']);
  assert.deepEqual([...tape.protocolSubscriptions].sort(), [
    '1 wss://example.test/public subscribe tickers.BTC',
    '2 wss://example.test/public subscribe tickers.BTC',
  ]);
  assert.equal(tape.pendingStartupSubscriptions, 0);
});

{
  const mexc = venueOf('mexc');
  const mexcBody = (body) => canonicalBody(body, new Set(), mexc);
  const mexcUrl = (url) => canonicalUrl(url, new Set(), mexc);
  class MexcTape extends Tape {
    constructor(records, options = {}) {
      super(records, { ...mexc, ...options }, options.isPrivate ?? false);
    }
  }
  const url = 'https://api.mexc.com/api/v1/private/order/cancel';
  test('POST matching preserves snowflakes and consumes every accepted write once', () => {
    const tape = new MexcTape([
      {
        type: 'http',
        method: 'POST',
        url,
        status: 200,
        body: 'accepted',
        requestBody: '{"id":"739113577038255617","vol":2}',
      },
    ]);
    assert.equal(tape.respond('POST', url, '{"vol":2,"id":739113577038255617}').body, 'accepted');
    assert.equal(tape.respond('POST', url, '{"id":"739113577038255617","vol":2}').status, 599);
    assert.notEqual(mexcBody('[739113577038255617]'), mexcBody('[739113577038255618]'));
  });

  test('body matching ignores only generated externalOid and keeps nested business fields', () => {
    assert.equal(
      mexcBody('[{"externalOid":"random1","price":"59000","vol":60}]'),
      mexcBody('[{"vol":60,"price":"59000","externalOid":"random2"}]'),
    );
    assert.notEqual(mexcBody('{"vol":60}'), mexcBody('{"vol":61}'));
    assert.notEqual(mexcBody('{"symbol":"BTC_USDT"}'), mexcBody('{"symbol":"ETH_USDT"}'));
  });

  test('only GET may reuse its last recorded snapshot', () => {
    const tape = new MexcTape([
      { type: 'http', method: 'GET', url, status: 200, body: 'one' },
      { type: 'http', method: 'GET', url, status: 200, body: 'two' },
    ]);
    assert.deepEqual(
      [tape.respond('GET', url).body, tape.respond('GET', url).body, tape.respond('GET', url).body],
      ['one', 'two', 'two'],
    );
  });

  test('rolling reconciliation windows keep exact widths; historical queries keep absolute bounds', () => {
    const active = 'https://api.mexc.com/api/v1/private/planorder/list/orders';
    assert.equal(
      mexcUrl(`${active}?start_time=10&end_time=100&states=1`),
      mexcUrl(`${active}?states=1&end_time=200&start_time=110`),
    );
    assert.notEqual(mexcUrl(`${active}?start_time=10&end_time=100`), mexcUrl(`${active}?start_time=11&end_time=100`));
    const history = 'https://api.mexc.com/api/v1/private/order/list/history_orders';
    assert.notEqual(
      mexcUrl(`${history}?start_time=10&end_time=100`),
      mexcUrl(`${history}?start_time=110&end_time=200`),
    );
    assert.equal(
      mexcUrl('https://fapi.binance.com/fapi/v1/order?symbol=BTCUSDT&timestamp=1&signature=a'),
      mexcUrl('https://fapi.binance.com/fapi/v1/order?signature=b&timestamp=2&symbol=BTCUSDT'),
    );
  });

  test('shared-URL sockets and reconnects deliver lifecycle events in tape order', { timeout: 1000 }, async () => {
    const socketUrl = 'wss://contract.mexc.com/edge';
    const records = [
      { type: 'open', t: 0, url: socketUrl, socket: 0 },
      { type: 'ws', t: 1, url: socketUrl, socket: 1, text: 'private' },
      { type: 'close', t: 2, url: socketUrl, socket: 0, code: 1006 },
      { type: 'open', t: 3, url: socketUrl, socket: 2 },
      { type: 'ws', t: 4, url: socketUrl, socket: 2, text: 'reconnected' },
    ];
    const tape = new MexcTape(records);
    const seen = [];
    tape.connect(
      socketUrl,
      () => {},
      (text) => seen.push(`public:${text}`),
      (code) => {
        seen.push(`close:${code}`);
        tape.connect(
          socketUrl,
          () => {},
          (text) => seen.push(`public:${text}`),
        );
      },
    );
    tape.connect(
      socketUrl,
      () => {},
      (text) => seen.push(`private:${text}`),
    );
    assert.equal(await tape.play(), 5);
    assert.deepEqual(seen, ['private:private', 'close:1006', 'public:reconnected']);
  });

  test('recorded streamed response bytes remain reusable after collection', async () => {
    async function* chunks() {
      yield new Uint8Array([123, 34]);
      yield new Uint8Array([118, 34, 58, 49, 125]);
    }
    const bytes = await bodyBytes(chunks());
    assert.equal(new TextDecoder().decode(bytes), '{"v":1}');
    assert.equal(await bodyBytes(bytes), bytes);
    assert.equal(new TextDecoder().decode(bytes), '{"v":1}');
  });

  test('public timers advance by recorded time with cancellation and repeated intervals', async () => {
    const clock = new ReplayClock(1700000000000);
    clock.install();
    try {
      clock.start();
      const seen = [];
      const interval = setInterval(() => seen.push(Date.now()), 100);
      const canceled = setTimeout(() => seen.push('canceled'), 150);
      clearTimeout(canceled);
      setTimeout(() => {
        clearInterval(interval);
        setTimeout(() => seen.push('retry'), 50);
      }, 250);
      await clock.advance(299);
      assert.deepEqual(seen, [1700000000100, 1700000000200]);
      await clock.advance(300);
      assert.deepEqual(seen, [1700000000100, 1700000000200, 'retry']);
    } finally {
      clock.restore();
    }
  });

  test('the replay epoch begins after account startup and before its first command', async () => {
    const tape = new MexcTape([{ type: 'command', t: 0, op: 'example' }], { isPrivate: true });
    const seen = [];
    await tape.play(
      undefined,
      async () => {
        seen.push('ready');
      },
      async () => {
        seen.push('command');
        return [];
      },
      undefined,
      () => seen.push('epoch'),
    );
    assert.deepEqual(seen, ['ready', 'epoch', 'command']);
  });

  test('Bybit request IDs are volatile while native client order IDs remain exact', () => {
    assert.equal(
      mexcUrl('wss://stream.bybit.com/v5/private?requestId=recorded'),
      mexcUrl('wss://stream.bybit.com/v5/private?requestId=runtime'),
    );
    assert.equal(
      mexcBody('{"requestId":"recorded","qty":"1","orderLinkId":"caller"}'),
      mexcBody('{"qty":"1","orderLinkId":"caller","requestId":"runtime"}'),
    );
    assert.notEqual(mexcBody('{"orderLinkId":"caller"}'), mexcBody('{"orderLinkId":"different"}'));
    assert.notEqual(mexcBody('{"qty":"1"}'), mexcBody('{"qty":"2"}'));
  });

  test(
    'legacy Bybit sockets match generated URLs and retain latest-connection routing',
    { timeout: 1000 },
    async () => {
      const url = 'wss://stream.bybit.com/v5/private';
      const tape = new MexcTape([{ type: 'ws', t: 0, url, text: 'private' }]);
      const seen = [];
      const first = tape.connect(
        `${url}?requestId=first`,
        () => {},
        (text) => seen.push(`first:${text}`),
      );
      await new Promise(setImmediate);
      tape.close(first);
      tape.connect(
        `${url}?requestId=second`,
        () => {},
        (text) => seen.push(`second:${text}`),
      );
      assert.equal(await tape.play(), 1);
      assert.deepEqual(seen, ['second:private']);
    },
  );

  test('Bybit PostOnly rejection derives from the original TS row, independently of Rust', async () => {
    const { VENUES } = await import('./venues.mjs');
    const reason = VENUES.bybit.tsRejectReason;
    assert.equal(reason({ status: 'canceled', info: { rejectReason: 'EC_PostOnlyWillTakeLiquidity' } }), 'PostOnly');
    assert.equal(reason({ status: 'canceled', info: { rejectReason: 'EC_PerCancelRequest' } }), undefined);
    assert.equal(reason({ status: 'open', info: { rejectReason: 'EC_PostOnlyWillTakeLiquidity' } }), undefined);
  });

  test('public replay starts with market/ticker seed while private replay waits for account state', () => {
    assert.equal(replayStoreReady(12, false), true);
    assert.equal(replayStoreReady(12, true), false);
    assert.equal(replayStoreReady(31, true), true);
    assert.equal(replayStoreReady(8, false), false);
    assert.equal(replayStoreReady(4, false), false);
  });
}

test('MEXC ID normalization preserves other venue body wire types', () => {
  const integer = '{"id":739113577038255617}';
  const text = '{"id":"739113577038255617"}';
  assert.notEqual(canonicalBody(integer), canonicalBody(text));
  assert.notEqual(canonicalBody({ externalOid: 'first' }), canonicalBody({ externalOid: 'second' }));
  assert.equal(canonicalBody(integer, new Set(), venueOf('mexc')), canonicalBody(text, new Set(), venueOf('mexc')));
});

test('requests fired during public clock advancement observe their timer time', async () => {
  const clock = new ReplayClock(1700000000000);
  clock.install();
  try {
    const tape = new Tape(
      [
        { ...record('GET'), body: 'initial' },
        { ...record('GET'), body: 'future', availableAfter: 200 },
      ],
      { checkRequest: () => true },
      false,
    );
    tape.replayClock = clock;
    tape.time = 300;
    clock.start();
    let observed;
    setTimeout(() => {
      observed = tape.respond('GET', URL).body;
    }, 100);
    await clock.advance(300);
    assert.equal(observed, 'initial');
    assert.equal(tape.respond('GET', URL).body, 'future');
  } finally {
    clock.restore();
  }
});

test('public commands pump retry timers without reversing time at the next frame', async () => {
  const epoch = 1700000000000;
  const clock = new ReplayClock(epoch);
  clock.install();
  try {
    const tape = new Tape(
      [
        { type: 'command', t: 0, op: 'retry' },
        { type: 'command', t: 50, op: 'next' },
      ],
      {},
      false,
      500,
    );
    const observed = [];
    const ticks = [];
    await tape.play(
      (t) => ticks.push(t),
      async () => {},
      async (frame) => {
        if (frame.op === 'retry') await new Promise((resolve) => setTimeout(resolve, 100));
        observed.push(Date.now() - epoch);
        return [];
      },
      clock,
    );
    assert.deepEqual(observed, [100, 100]);
    assert.deepEqual(ticks, [0, 50]);
  } finally {
    clock.restore();
  }
});

test('a public command with no completion still hits its real timeout', async () => {
  const clock = new ReplayClock(1700000000000);
  clock.install();
  try {
    const tape = new Tape([{ type: 'command', t: 0, op: 'never' }], {}, false, 20);
    await assert.rejects(
      tape.play(
        undefined,
        async () => {},
        () => new Promise(() => {}),
        clock,
      ),
      /replay timeout: command never/,
    );
  } finally {
    clock.restore();
  }
});

test('public routing waits pump subscription timers and still require the actual topic', async () => {
  const epoch = 1700000000000;
  const clock = new ReplayClock(epoch);
  clock.install();
  try {
    const tape = new Tape(
      [
        { type: 'command', t: 0, op: 'subscribeLater' },
        { type: 'ws', t: 0, url: 'wss://example.test', socket: 0, text: '{"stream":"account.orderUpdate"}' },
      ],
      { streamOf: (text) => JSON.parse(text).stream },
      false,
      500,
    );
    const delivered = [];
    const socket = tape.connect(
      'wss://example.test',
      () => {},
      () => delivered.push(Date.now() - epoch),
    );
    await tape.play(
      undefined,
      async () => {},
      () => {
        setTimeout(() => tape.send(socket, '{"method":"SUBSCRIBE","params":["ticker.SOL_USDC_PERP"]}'), 10);
        setTimeout(() => tape.send(socket, '{"method":"SUBSCRIBE","params":["account.orderUpdate"]}'), 25);
        return [];
      },
      clock,
    );
    assert.deepEqual(delivered, [25]);
  } finally {
    clock.restore();
  }
});

test('BloFin typed request matching preserves nested caller IDs and single-use writes', () => {
  const venue = venueOf('blofin');
  const route = 'https://openapi.blofin.com/api/v1/trade/batch-orders';
  const caller = 'b3cdedc0a20880baCaller123';
  const expected = [{ size: '1', clientOrderId: caller }];
  const tape = new Tape(
    [
      { type: 'command', op: 'amendOrder', update: { nested: { clientOrderId: caller } } },
      { ...record('POST'), url: route, requestBody: JSON.stringify(expected) },
    ],
    venue,
  );
  assert.equal(tape.respond('POST', route, [{ size: '1', clientOrderId: 'b3cdedc0a20880baMinted1' }]).status, 599);
  assert.equal(tape.respond('POST', route, [{ size: 1, clientOrderId: caller }]).status, 599);
  assert.equal(tape.respond('POST', route, [{ ...expected[0], triggerPriceType: 'last' }]).status, 200);
  assert.equal(tape.respond('POST', route, expected).status, 599);
  assert.equal(tape.missingWrites, 0);
});

test('BloFin socket request comparison preserves credentials and native topics', () => {
  const tape = new Tape([], venueOf('blofin'));
  const route = 'wss://openapi.blofin.com/ws/private';
  tape.sent(route, JSON.stringify({ op: 'login', args: [{ apiKey: 'key', timestamp: '1', nonce: 'a', sign: 'a' }] }));
  tape.sent(route, JSON.stringify({ op: 'login', args: [{ apiKey: 'key', timestamp: '2', nonce: 'b', sign: 'b' }] }));
  assert.equal(tape.sentRequests.size, 1);
  tape.sent(route, JSON.stringify({ op: 'login', args: [{ apiKey: 'wrong', sign: 'b' }] }));
  assert.equal(tape.sentRequests.size, 2);
  tape.sent(route, JSON.stringify({ op: 'subscribe', args: [{ channel: 'orders', instType: 'SWAP' }] }));
  tape.sent(route, JSON.stringify({ op: 'subscribe', args: [{ instType: 'SWAP', channel: 'orders' }] }));
  assert.equal(tape.sentRequests.size, 3);
  tape.sent(route, JSON.stringify({ op: 'subscribe', args: [{ channel: 'positions', instType: 'SWAP' }] }));
  assert.equal(tape.sentRequests.size, 4);
});

import { Responses } from './responses.mjs';
test('explicit lifecycle retains startup opens and verifies local closes', async () => {
  const url = 'wss://example.test/public';
  const records = [
    { type: 'connect', t: 0, url, connection: 0 },
    { type: 'open', t: 1, url, connection: 0 },
    { type: 'close', t: 2, url, connection: 0, local: true },
  ];
  const tape = new Tape(records, { explicitOpens: true, legacyLatest: false });
  const seen = [];
  const socket = tape.connect(url, () => seen.push('open'), () => {}, () => seen.push('close'));
  await new Promise(setImmediate);
  assert.deepEqual(seen, []);
  await tape.lifecycle(tape.frames[0]);
  assert.deepEqual(seen, ['open']);
  await assert.rejects(tape.lifecycle(tape.frames[1]), /local close was not requested/);
  tape.close(socket, 1000, true);
  await tape.lifecycle(tape.frames[1]);
  assert.deepEqual(seen, ['open']);
});

test('frame URL preparation shares canonical spelling and preserves both ordinal formats', () => {
  const url = 'wss://example.test/shared';
  let canonicalizations = 0;
  const tape = new Tape([
    { type: 'ws', t: 0, url, socket: 0, text: '{}' },
    { type: 'ws', t: 1, url, connection: 1, text: '{}' },
  ], { canonicalize: (value) => { canonicalizations++; return canonicalUrl(value); } });
  assert.equal(canonicalizations, 1);
  assert.deepEqual(tape.frames.map((frame) => frame.connection), [0, 1]);
  assert.equal(tape.frames[0].url, tape.frames[1].url);
});

test('explicit remote closes notify once and prevent delivery to the old connection', async () => {
  const url = 'wss://example.test/private';
  const tape = new Tape([{ type: 'close', t: 0, url, connection: 0, code: 1006 }]);
  const seen = [];
  tape.connect(url, () => {}, () => {}, (code) => seen.push(code));
  await new Promise(setImmediate);
  await tape.lifecycle(tape.frames[0]);
  assert.deepEqual(seen, [1006]);
  await assert.rejects(tape.lifecycle(tape.frames[0]), /before recorded remote close/);
});

test('timer HTTP reads and mutations use the timer time while advancing toward a later frame', async () => {
  const read = { type: 'http', method: 'GET', url: 'https://example.test/state', status: 200 };
  const write = { type: 'http', method: 'POST', url: 'https://example.test/change', status: 200, body: 'written', effectiveAt: 3000 };
  const records = [
    { ...read, body: 'initial', effectiveAt: 0 },
    { ...read, body: 'early', effectiveAt: 500 },
    { ...read, body: 'later', effectiveAt: 3000 }, write,
    { type: 'command', t: 5000, op: 'finish' },
  ];
  const responses = new Responses(records, (method, url) => method + url);
  const tape = new Tape(records, { responses });
  const clock = new ReplayClock(1700000000000);
  const seen = [];
  clock.install();
  try {
    await tape.play(undefined, async () => {
      for (const time of [1000, 4000]) setTimeout(() => {
        seen.push([tape.respond('GET', read.url).body, tape.respond('POST', write.url).status]);
      }, time);
    }, async () => [], clock);
    assert.deepEqual(seen, [['early', 599], ['later', 200]]);
    assert.equal(responses.unusedWrites(), 0);
  } finally { clock.restore(); }
});

test('legacy shared-URL initial sockets auto-open but recorded reconnects wait for their open', async () => {
  const url = 'wss://example.test/shared';
  const tape = new Tape([
    { type: 'open', t: 0, url, socket: 0 },
    { type: 'open', t: 1, url, socket: 1 },
    { type: 'close', t: 2, url, socket: 0, code: 1006 },
    { type: 'open', t: 50, url, socket: 2 },
  ]);
  const seen = [];
  tape.connect(url, () => seen.push('public'), () => {}, () => {
    tape.connect(url, () => seen.push('reconnect'), () => {});
  });
  tape.connect(url, () => seen.push('private'), () => {});
  await new Promise(setImmediate);
  assert.deepEqual(seen, ['public', 'private']);
  await tape.lifecycle(tape.frames[2]);
  await new Promise(setImmediate);
  assert.deepEqual(seen, ['public', 'private']);
  await tape.lifecycle(tape.frames[3]);
  assert.deepEqual(seen, ['public', 'private', 'reconnect']);
});

test('recorded startup opens can finish asynchronous socket and subscription bootstrap before tape time', async () => {
  const url = 'wss://example.test/explicit';
  const clock = new ReplayClock(1700000000000);
  const tape = new Tape([
    { type: 'connect', t: 0, url, connection: 0 },
    { type: 'open', t: 702, url, connection: 0 },
  ], { explicitOpens: true });
  const seen = [];
  let subscribed;
  const ready = new Promise((resolve) => { subscribed = resolve; });
  clock.install();
  try {
    const constructed = new Promise((resolve) => setTimeout(() => {
      tape.connect(url, () => setTimeout(() => {
        seen.push('subscribed');
        subscribed();
      }, 5), () => {});
      resolve();
    }, 5));
    // Playback may encounter the first recorded open before async startup runs.
    // An inactive clock leaves those timers attached to real time.
    await clock.advance(702);
    assert.equal(clock.active, false);
    await constructed;
    await tape.lifecycle(tape.frames[0]);
    await ready;
    assert.deepEqual(seen, ['subscribed']);
    clock.start(702);
    assert.equal(Date.now(), 1700000000702);
    setTimeout(() => seen.push(Date.now()), 100);
    await clock.advance(801);
    assert.deepEqual(seen, ['subscribed']);
    await clock.advance(802);
    assert.deepEqual(seen, ['subscribed', 1700000000802]);
  } finally { clock.restore(); }
});

test('active public HTTP completion follows its recorded time while consumption stays at request time', async () => {
  const response = { type: 'http', t: 3000, method: 'POST', url: 'https://example.test/change', status: 200, body: 'done' };
  const responses = new Responses([response], (method, url) => method + url);
  const tape = new Tape([], { responses });
  const clock = new ReplayClock(1700000000000);
  tape.replayClock = clock;
  const seen = [];
  clock.install();
  try {
    clock.start();
    setTimeout(() => {
      const reply = tape.respond('POST', response.url);
      assert.equal(responses.unusedWrites(), 0);
      assert.equal(tape.respond('POST', response.url).status, 599);
      tape.waitForResponse(reply).then(() => seen.push(Date.now()));
    }, 1000);
    await clock.advance(2000);
    assert.deepEqual(seen, []);
    await clock.advance(3000);
    assert.deepEqual(seen, [1700000003000]);
  } finally { clock.restore(); }
  const startup = new Tape([]);
  const privateTape = new Tape([], { isPrivate: true });
  privateTape.replayClock = { active: true, epoch: 0, time: 0 };
  const reply = { recordedAt: 3000 };
  assert.equal(await startup.waitForResponse(reply), reply);
  assert.equal(await privateTape.waitForResponse(reply), reply);
});

test('serialized public commands complete future HTTP replies without advancing past the command', { timeout: 1000 }, async () => {
  const http = { type: 'http', t: 200, method: 'GET', url: 'https://example.test/candles', status: 200, body: 'candles' };
  const records = [http, { type: 'command', t: 100, op: 'fetchCandles' }, { type: 'command', t: 300, op: 'reject' }];
  const responses = new Responses(records, (method, url) => method + url);
  const tape = new Tape(records, { responses });
  const clock = new ReplayClock(1700000000000);
  const times = [];
  clock.install();
  try {
    await tape.play(undefined, undefined, async (frame) => {
      if (frame.op === 'reject') throw new Error('rejected');
      const result = await tape.waitForResponse(tape.respond('GET', http.url));
      times.push(clock.time - clock.epoch);
      return result.body;
    }, clock);
    assert.deepEqual(times, [100]);
    assert.deepEqual(tape.results, [{ ok: 'candles' }, { error: 'rejected' }]);
    assert.equal(tape.commandIndex, 2);
    assert.equal(tape.activeCommands, 0);
    let completed = false;
    const background = tape.waitForResponse({ recordedAt: 500 }).then(() => { completed = true; });
    await clock.advance(499);
    assert.equal(completed, false);
    await clock.advance(500);
    await background;
    assert.equal(completed, true);
  } finally { clock.restore(); }
});

test('play drains selected HTTP completions past the last frame without draining future watchdogs', async () => {
  const http = { type: 'http', t: 200, method: 'GET', url: 'https://example.test/book', status: 200, body: 'book' };
  const tape = new Tape([http, { type: 'command', t: 100, op: 'noop' }], {
    responses: new Responses([http], (method, url) => method + url),
  });
  const clock = new ReplayClock(1700000000000);
  let delivered = false;
  let watchdog = false;
  clock.install();
  try {
    await tape.play(undefined, async () => {
      setTimeout(() => {
        tape.waitForResponse(tape.respond('GET', http.url)).then(() => { delivered = true; });
      }, 50);
      setTimeout(() => { watchdog = true; }, 1000);
    }, async () => [], clock);
    assert.equal(delivered, true);
    assert.equal(watchdog, false);
    assert.equal(clock.time - clock.epoch, 200);
    assert.equal(tape.time, 200);
    assert.equal(tape.pendingResponses.size, 0);
  } finally { clock.restore(); }
});

test('an explicit recorded open waiting before socket construction resolves before opening', async () => {
  const url = 'wss://example.test/explicit-late';
  const tape = new Tape([
    { type: 'connect', t: 0, url, connection: 0 },
    { type: 'open', t: 10, url, connection: 0 },
  ], { explicitOpens: true }, false, 100);
  const frame = tape.frames[0];
  const waiting = tape.socketFor(frame);
  let opens = 0;
  setImmediate(() => tape.connect(url, () => opens++, () => {}));
  const socket = await waiting;
  assert.equal(socket.opened, false);
  assert.equal(opens, 0);
  await tape.lifecycle(frame);
  assert.equal(socket.opened, true);
  assert.equal(opens, 1);
});

test('declared TS body repair changes only the comparison of the exactly matched fixture write', () => {
  const intended = { orderType: 'Market', quantity: '2.00', clientId: 'caller', stopLossTriggerPrice: '120.00' };
  const declared = { ...intended, stopLossLimitPrice: '120.00' };
  const row = { ...record('POST', intended), tsRequestBody: JSON.stringify(declared) };
  const other = record('POST', { orderType: 'Limit', quantity: '1', price: '100' });
  const records = [{ type: 'command', orders: [{ clientOrderId: 'caller' }] }, row, other];
  const ts = new Tape(records, { compareWriteOrder: true }, false, 1000, 'ts');
  for (const changed of [{ ...declared, reduceOnly: true }, { ...declared, clientId: 'changed' }, { ...declared, quantity: '3.00' }]) {
    const rejected = ts.respond('POST', URL, changed);
    assert.equal(rejected.status, 599);
    assert.equal(Object.hasOwn(rejected, 'comparisonRequestBody'), false);
  }
  assert.equal(ts.missingWrites, 2);
  const accepted = ts.respond('POST', URL, declared);
  assert.equal(accepted.status, 200);
  assert.equal(accepted.comparisonRequestBody, row.requestBody);
  assert.equal(ts.missingWrites, 1);
  assert.equal(ts.respond('POST', URL, { ...JSON.parse(other.requestBody), stopLossLimitPrice: '120.00' }).status, 599);
  const ordinary = ts.respond('POST', URL, other.requestBody);
  assert.equal(ordinary.status, 200);
  assert.equal(Object.hasOwn(ordinary, 'comparisonRequestBody'), false);
  assert.equal(ts.missingWrites, 0);
  const rust = new Tape(records, { compareWriteOrder: true }, false, 1000, 'rust');
  assert.equal(rust.respond('POST', URL, declared).status, 599);
  const native = rust.respond('POST', URL, intended);
  assert.equal(native.status, 200);
  assert.equal(Object.hasOwn(native, 'comparisonRequestBody'), false);
});

test('Backpack legacy startup waits for every requested REST book before activating tape time', async () => {
  assert.equal(venueOf('backpack').initialBookSnapshots, true);
  assert.equal(replayBooksReady(['SOL', 'BTC'], ['SOL', 'ETH']), false);
  assert.equal(replayBooksReady([], []), true);
  const url = 'wss://example.test/bootstrap';
  const tape = new Tape([{ type: 'ws', t: 1637, url, text: '{}' }]);
  const clock = new ReplayClock(1700000000000);
  const books = new Set(['SOL']);
  const seen = [];
  clock.install();
  try {
    tape.connect(url, () => {}, () => seen.push([...books]));
    setTimeout(() => { assert.equal(clock.active, false); books.add('BTC'); }, 20);
    await tape.play(undefined, async () => {
      while (!replayBooksReady(['SOL', 'BTC'], books)) await new Promise((resolve) => setTimeout(resolve, 1));
    }, undefined, clock);
    assert.deepEqual(seen, [['SOL', 'BTC']]);
    assert.equal(clock.active, true);
  } finally { clock.restore(); }
});
