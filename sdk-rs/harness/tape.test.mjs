import assert from 'node:assert/strict';
import test from 'node:test';
import { createTape, bodyBytes, canonicalBody, canonicalUrl, replayStoreReady, ReplayClock } from './tape.mjs';
const venue = { rust: 'bitget' };
const url = 'https://api.bitget.com/api/v2/mix/order/modify-order';
const response = (body, overrides = {}) => ({
  type: 'http',
  method: 'POST',
  url,
  requestBody: body,
  status: 200,
  body: '{"code":"00000","data":{"orderId":"2"}}',
  ...overrides,
});

test('mutation replies are consumed once; omitted writes remain visible', () => {
  const tape = createTape([response({ orderId: '1', newClientOid: 'expected' })], venue, true);
  assert.equal(tape.unusedWrites, 1);
  assert.equal(tape.respond('POST', url, { orderId: '1', newClientOid: 'fresh' }).status, 200);
  assert.equal(tape.unusedWrites, 0);
  assert.equal(tape.respond('POST', url, { orderId: '1', newClientOid: 'fresh' }).status, 599);
  assert.equal(tape.unserved, 1);
});

test('generated IDs preserve field presence, type, and freshness', () => {
  for (const body of [{ orderId: '1' }, { orderId: '1', newClientOid: null }, { orderId: '1', newClientOid: '' }]) {
    const tape = createTape([response({ orderId: '1', newClientOid: 'expected' })], venue, true);
    assert.equal(tape.respond('POST', url, body).status, 599);
  }
  const tape = createTape(
    [response({ orderId: '1', newClientOid: 'a' }), response({ orderId: '2', newClientOid: 'b' })],
    venue,
    true,
  );
  tape.respond('POST', url, { orderId: '1', newClientOid: 'fresh' });
  assert.throws(() => tape.respond('POST', url, { orderId: '2', newClientOid: 'fresh' }), /reused/);
});

test('caller IDs compare verbatim and margin correction is Classic batch only', () => {
  const place = 'https://api.bitget.com/api/v3/trade/place-order';
  const records = [
    { type: 'command', orders: [{ clientOrderId: 'caller' }] },
    response({ clientOid: 'caller' }, { url: place }),
  ];
  assert.equal(createTape(records, venue, true).respond('POST', place, { clientOid: 'wrong' }).status, 599);
  assert.equal(createTape(records, venue, true).respond('POST', place, { clientOid: 'caller' }).status, 200);
  assert.equal(
    createTape([response({ marginMode: 'crossed' }, { url: place })], venue, true).respond('POST', place, {
      marginMode: 'cross',
    }).status,
    599,
  );
  const classic = 'https://api.bitget.com/api/v2/mix/order/batch-place-order';
  assert.equal(
    createTape([response({ marginMode: 'crossed' }, { url: classic })], venue, true).respond('POST', classic, {
      marginMode: 'cross',
    }).status,
    200,
  );
});

test('signature rejection permits exactly the taped same-ID retry; reads can poll', () => {
  const body = { orderId: '1', newClientOid: 'fresh' };
  const tape = createTape([response(body, { body: '{"code":"40009"}' }), response(body)], venue, true);
  assert.equal(tape.respond('POST', url, body).status, 200);
  assert.equal(tape.respond('POST', url, body).status, 200);
  const reads = createTape([response(undefined, { method: 'GET' })], venue, false);
  assert.equal(reads.respond('GET', url).status, 200);
  assert.equal(reads.respond('GET', url).status, 200);
});

test('same-URL sockets receive only exact product/topic subscriptions', async () => {
  const tape = createTape([], venue, false);
  const ws = 'wss://ws.bitget.com/v2/ws/public';
  let correct = 0;
  let wrong = 0;
  const good = tape.connect(
    ws,
    () => {},
    () => correct++,
  );
  const bad = tape.connect(
    ws,
    () => {},
    () => wrong++,
  );
  tape.sent(
    good,
    JSON.stringify({ op: 'subscribe', args: [{ instType: 'USDT-FUTURES', channel: 'books', instId: 'BTCUSDT' }] }),
  );
  tape.sent(
    bad,
    JSON.stringify({ op: 'subscribe', args: [{ instType: 'USDC-FUTURES', channel: 'books', instId: 'BTCUSDT' }] }),
  );
  await tape.deliver({
    url: ws,
    text: JSON.stringify({ arg: { instType: 'USDT-FUTURES', channel: 'books', instId: 'BTCUSDT' } }),
  });
  assert.equal(correct, 1);
  assert.equal(wrong, 0);
  await assert.rejects(
    tape.deliver({ url: ws, text: JSON.stringify({ arg: { instType: 'UTA', topic: 'books', symbol: 'BTCUSDT' } }) }),
    /no subscribed/,
  );
});

test('existing client IDs are exact identities; only created aliases can translate', () => {
  const cancel = 'https://api.bitget.com/api/v3/trade/cancel-strategy-order';
  const tape = createTape([response({ clientOid: 'resting-client' }, { url: cancel })], venue, true);
  assert.equal(tape.respond('POST', cancel, { clientOid: 'wrong-order' }).status, 599);
  assert.equal(tape.unusedWrites, 1);
  assert.equal(tape.respond('POST', cancel, { clientOid: 'resting-client' }).status, 200);
  const place = 'https://api.bitget.com/api/v3/trade/place-order';
  const linked = createTape(
    [response({ clientOid: 'taped-minted' }, { url: place }), response({ clientOid: 'taped-minted' }, { url: cancel })],
    venue,
    true,
  );
  assert.equal(linked.respond('POST', place, { clientOid: 'actual-minted' }).status, 200);
  assert.equal(linked.respond('POST', cancel, { clientOid: 'unrelated' }).status, 599);
  assert.equal(linked.respond('POST', cancel, { clientOid: 'actual-minted' }).status, 200);
});

test('modify-by-client-id preserves an existing generated identity', () => {
  const place = 'https://api.bitget.com/api/v3/trade/place-order';
  const tape = createTape(
    [
      response({ clientOid: 'taped-old' }, { url: place }),
      response({ clientOid: 'taped-old', newClientOid: 'taped-new' }),
    ],
    venue,
    true,
  );
  assert.equal(tape.respond('POST', place, { clientOid: 'actual-old' }).status, 200);
  assert.equal(tape.respond('POST', url, { clientOid: 'actual-old', newClientOid: 'actual-new' }).status, 200);
});

test('Crypto.com POST reads reuse staged snapshots and retain exact integer parameters', () => {
  const read = 'https://api.crypto.com/exchange/v1/private/get-trades';
  const body = (id, end = '1610000000000000001') =>
    `{"id":${id},"method":"private/get-trades","params":{"end_time":${end}},"sig":"volatile"}`;
  const rpcVenue = {
    rust: 'cryptocom',
    matchRequestBody: true,
    isWrite: (_method, requestUrl) => requestUrl.includes('/create-'),
  };
  const tape = createTape(
    [
      response(body(1), { url: read, body: 'old', headers: 'stage: 0' }),
      response(body(2), { url: read, body: 'new', headers: 'stage: 2', afterCommand: 2 }),
    ],
    rpcVenue,
    true,
  );
  assert.deepEqual(tape.respond('POST', read, body(100)), { status: 200, headers: 'stage: 0', body: 'old' });
  assert.equal(tape.respond('POST', read, body(101)).body, 'old');
  assert.equal(tape.respond('POST', read, body(102, '1610000000000000002')).status, 599);
  tape.commandIndex = 2;
  assert.deepEqual(tape.respond('POST', read, body(103)), { status: 200, headers: 'stage: 2', body: 'new' });
  assert.equal(tape.unserved, 1);
  assert.equal(tape.unusedWrites, 0);
  tape.assertWritesConsumed();
});

test('venue request checks reject a matching write without consuming its response', () => {
  const write = 'https://api.crypto.com/exchange/v1/private/create-order';
  const body = (sig) => JSON.stringify({ method: 'private/create-order', params: { quantity: '1' }, sig });
  const rpcVenue = {
    rust: 'cryptocom',
    matchRequestBody: true,
    checkRequest: (_method, _url, requestBody) => JSON.parse(requestBody).sig === 'valid',
    isWrite: () => true,
  };
  const tape = createTape([response(body('recorded'), { url: write })], rpcVenue, true);
  assert.equal(tape.respond('POST', write, body('invalid')).status, 599);
  assert.equal(tape.unusedWrites, 1);
  assert.equal(tape.respond('POST', write, body('valid')).status, 200);
  assert.equal(tape.unusedWrites, 0);
  assert.equal(tape.respond('POST', write, body('valid')).status, 599);
});

test('factory supplies socket audit context and audits frames before venue delivery', async () => {
  const seen = [];
  const books = ['BTCUSD-PERP'];
  const auditVenue = {
    rust: 'cryptocom',
    publicTickers: () => ['BTCUSD-PERP', 'ETHUSD-PERP'],
    createSocketAudit: (impl, context) => {
      assert.equal(impl, 'rust');
      assert.deepEqual(context, { isPrivate: true, books, tickers: ['BTCUSD-PERP', 'ETHUSD-PERP'] });
      return {
        sent: (_url, text) => seen.push(['sent', text]),
        received: (_url, text) => seen.push(['received', text]),
      };
    },
  };
  const tape = createTape([], auditVenue, true, { impl: 'rust', books });
  const socketUrl = 'wss://stream.crypto.com/exchange/v1/user';
  const socket = tape.connect(
    socketUrl,
    () => {},
    (text) => seen.push(['delivered', text]),
  );
  tape.sent(socket, '{"method":"public/auth"}');
  await tape.deliver({ url: socketUrl, text: '{"method":"public/auth","code":0}' });
  assert.deepEqual(seen, [
    ['sent', '{"method":"public/auth"}'],
    ['received', '{"method":"public/auth","code":0}'],
    ['delivered', '{"method":"public/auth","code":0}'],
  ]);
});

test('signed request validation receives headers and leaves rejected writes unused', () => {
  const checkedVenue = {
    rust: 'weex',
    matchRequestBody: true,
    validateRequest: ({ method, url: sentUrl, body, headers }) => {
      assert.equal(method, 'POST');
      assert.equal(sentUrl, url);
      assert.deepEqual(body, { quantity: '1' });
      if (headers?.signature !== 'valid') throw new Error('bad request headers');
    },
  };
  const tape = createTape([response({ quantity: '1' })], checkedVenue, true);
  assert.throws(() => tape.respond('POST', url, { quantity: '1' }, {}), /bad request headers/);
  assert.equal(tape.unusedWrites, 1);
  assert.equal(tape.respond('POST', url, { quantity: '1' }, { signature: 'valid' }).status, 200);
  assert.equal(tape.unusedWrites, 0);
});

test('frame times select reusable read templates within the active command stage', async () => {
  const read = 'https://api-contract.weex.com/capi/v3/account/income';
  const readVenue = { rust: 'weex', isWrite: () => false };
  const seen = [];
  const tape = createTape(
    [
      response(undefined, { method: 'POST', url: read, body: 'before', effectiveAt: 0 }),
      response(undefined, { method: 'POST', url: read, body: 'after', effectiveAt: 10 }),
      response(undefined, { method: 'POST', url: read, body: 'next stage', effectiveAt: 0, afterCommand: 3 }),
      { type: 'command', op: 'checkpoint', t: 0 },
      { type: 'command', op: 'checkpoint', t: 10 },
      { type: 'command', op: 'checkpoint', t: 20 },
    ],
    readVenue,
    false,
  );
  await tape.play(undefined, undefined, async () => {
    seen.push(tape.respond('POST', read).body);
    seen.push(tape.respond('POST', read).body);
    return [];
  });
  assert.deepEqual(seen, ['before', 'before', 'after', 'after', 'next stage', 'next stage']);
  assert.equal(tape.unusedWrites, 0);
  assert.equal(tape.unserved, 0);
});

test('response placeholders use sent generated IDs and mutations remain single use', () => {
  const write = 'https://api-contract.weex.com/capi/v3/algoOrder';
  const generatedVenue = { rust: 'weex', matchRequestBody: true, volatileBodyFields: true };
  const body = { symbol: 'BTCUSDT', clientAlgoId: 'fresh-algo', newClientOrderId: 'fresh-order' };
  const tape = createTape(
    [
      response(
        { symbol: 'BTCUSDT' },
        {
          url: write,
          body: '{"algoId":"$clientAlgoId","clientId":"$newClientOrderId"}',
        },
      ),
    ],
    generatedVenue,
    true,
  );
  assert.deepEqual(JSON.parse(tape.respond('POST', write, JSON.stringify(body)).body), {
    algoId: 'fresh-algo',
    clientId: 'fresh-order',
  });
  assert.equal(tape.respond('POST', write, body).status, 599);
});

test('URL and nested JSON query matching keep caller IDs exact', () => {
  const queryVenue = { rust: 'weex', volatileBodyFields: true };
  const endpoint = 'https://api-contract.weex.com/capi/v3/batchOrders';
  const make = (caller, minted, reverse = false) => {
    const row = reverse
      ? { nested: { clientAlgoId: minted }, newClientOrderId: caller, symbol: 'BTCUSDT' }
      : { symbol: 'BTCUSDT', newClientOrderId: caller, nested: { clientAlgoId: minted } };
    return `${endpoint}?batchOrders=${encodeURIComponent(JSON.stringify([row]))}`;
  };
  const tape = createTape(
    [
      { type: 'command', orders: [{ clientOrderId: 'caller' }] },
      response(undefined, { url: `${endpoint}?newClientOrderId=caller` }),
      response(undefined, { url: make('caller', 'recorded-generated') }),
    ],
    queryVenue,
    true,
  );
  assert.equal(tape.respond('POST', `${endpoint}?newClientOrderId=wrong`).status, 599);
  assert.equal(tape.respond('POST', `${endpoint}?newClientOrderId=caller`).status, 200);
  assert.equal(tape.respond('POST', make('wrong', 'actual-generated', true)).status, 599);
  assert.equal(tape.respond('POST', make('caller', 'actual-generated', true)).status, 200);
  assert.equal(tape.unusedWrites, 0);
});

test('missing socket deadlines run even while virtual time is stopped and remove pending waiters', async () => {
  const tape = createTape([], venue, false, { socketTimeoutMs: 5 });
  const clock = new ReplayClock(1700000000000);
  clock.install();
  clock.start();
  try {
    await assert.rejects(tape.socketFor('wss://ws.bitget.com/missing'), /socket did not open/);
    assert.equal(tape.waiters.length, 0);
  } finally {
    clock.restore();
  }
});

test('BingX matches typed bodies and caller IDs while listen-key lifecycle responses remain reusable', () => {
  const order = 'https://open-api.bingx.com/openApi/swap/v2/trade/order';
  const listen = 'https://open-api.bingx.com/openApi/user/auth/userDataStream';
  const tape = createTape(
    [
      { type: 'command', orders: [{ clientOrderId: 'caller' }] },
      response({ quantity: 1, clientOrderId: 'caller', timestamp: 1 }, { url: order }),
      ...['POST', 'PUT', 'DELETE'].map((method) => response(undefined, { method, url: listen })),
    ],
    { rust: 'bingx', matchRequestBody: true },
    true,
  );
  assert.equal(tape.unusedWrites, 1);
  assert.equal(tape.respond('POST', order, { quantity: '1', clientOrderId: 'caller' }).status, 599);
  assert.equal(tape.respond('POST', order, { quantity: 1, clientOrderId: 'wrong' }).status, 599);
  assert.equal(tape.respond('POST', order, { quantity: 1, clientOrderId: 'caller', timestamp: 2 }).status, 200);
  assert.equal(tape.respond('POST', order, { quantity: 1, clientOrderId: 'caller' }).status, 599);
  for (const method of ['POST', 'PUT', 'DELETE']) {
    assert.equal(tape.respond(method, listen).status, 200);
    assert.equal(tape.respond(method, listen).status, 200);
  }
  assert.equal(tape.unusedWrites, 0);
});

test('both time spellings and command stages prevent premature snapshots', () => {
  const read = 'https://api-contract.weex.com/capi/v3/account/income';
  const tape = createTape(
    [
      response(undefined, { method: 'POST', url: read, body: 'first', effectiveAt: 10 }),
      response(undefined, { method: 'POST', url: read, body: 'second', availableAfter: 20, afterCommand: 2 }),
    ],
    { rust: 'weex', isWrite: null, matchRequestBody: true },
    false,
  );
  // With no venue override POST is a write; it still cannot precede its time.
  assert.equal(tape.respond('POST', read).status, 599);
  tape.time = 10;
  assert.equal(tape.respond('POST', read).body, 'first');
  tape.time = 20;
  assert.equal(tape.respond('POST', read).status, 599);
  tape.commandIndex = 2;
  assert.equal(tape.respond('POST', read).body, 'second');
  assert.equal(tape.respond('POST', read).status, 599);
});

test('Bitget callback validation fails before consuming a matched mutation', () => {
  const tape = createTape([response({ orderId: '1', newClientOid: 'recorded' })], venue, true);
  assert.throws(() => tape.respond('POST', url, { orderId: '1', newClientOid: 'invalid id' }), /invalid generated/);
  assert.equal(tape.unusedWrites, 1);
  assert.equal(tape.respond('POST', url, { orderId: '1', newClientOid: 'fresh' }).status, 200);
  assert.equal(tape.unusedWrites, 0);
});

test('close frames invoke callbacks and permit sockets first appearing after reconnection', async () => {
  const first = 'wss://open-api-swap.bingx.com/swap-market?listenKey=first';
  const next = 'wss://open-api-swap.bingx.com/swap-market?listenKey=next';
  const seen = [];
  const tape = createTape(
    [
      { type: 'ws', url: first, t: 0, text: 'before' },
      { type: 'close', url: first, t: 1, code: 1012 },
      { type: 'ws', url: next, t: 2, text: 'after' },
    ],
    { rust: 'bingx' },
    false,
  );
  const socket = tape.connect(first, () => {}, (text) => seen.push(text), (code) => {
    seen.push(code);
    tape.connect(next, () => {}, (text) => seen.push(text));
  });
  assert.equal(await tape.play(), 3);
  assert.equal(socket.closed, true);
  assert.deepEqual(seen, ['before', 1012, 'after']);
});

test('startup waits for seed subscriptions to reach send and retains per-generation subscriptions', async () => {
  const ws = 'wss://stream.bybit.com/v5/public/linear';
  const topic = `${ws} subscribe tickers.BTCUSDT`;
  const seen = [];
  const tape = createTape(
    [{ type: 'ws', url: ws, t: 0, text: '{}' }],
    { rust: 'bybit', startupSubscriptions: () => [topic] },
    false,
  );
  const socket = tape.connect(`${ws}?requestId=one`, () => {}, () => seen.push('frame'));
  await tape.play(undefined, async () => {
    setImmediate(() => {
      tape.send(socket, '{"op":"subscribe","args":["tickers.BTCUSDT"]}');
      seen.push('sent');
    });
  });
  assert.deepEqual(seen, ['sent', 'frame']);
  assert.equal(tape.pendingStartupSubscriptions, 0);
  assert.equal(tape.startupSubscriptions.get(topic), true);
  const replacement = tape.connect(`${ws}?requestId=two`, () => {}, () => {});
  tape.send(replacement, '{"op":"subscribe","args":["tickers.BTCUSDT"]}');
  assert.deepEqual([...tape.subscriptions], [`1 ${topic}`, `2 ${topic}`]);
});

test('BloFin factory compares bodies and defaults while retaining nested caller IDs and cache-buster scope', async () => {
  const { venueOf } = await import('./venues.mjs');
  const blofin = venueOf('blofin');
  const place = 'https://openapi.blofin.com/api/v1/trade/order';
  const instruments = 'https://openapi.blofin.com/api/v1/market/instruments';
  const caller = 'b3cdedc0a20880baCaller123';
  const tape = createTape([
    { type: 'command', nested: { legs: [{ clientOrderId: caller }] } },
    response({ clientOrderId: caller, size: '1' }, { url: place }),
    response({ clientOrderId: 'b3cdedc0a20880baMinted1', size: '2', triggerPriceType: 'last' }, { url: place }),
    response(undefined, { method: 'GET', url: `${instruments}?recorded` }),
  ], blofin, true);
  assert.equal(tape.respond('POST', place, { clientOrderId: 'b3cdedc0a20880baWrong', size: '1' }).status, 599);
  assert.equal(tape.respond('POST', place, { clientOrderId: caller, size: 1 }).status, 599);
  assert.equal(tape.respond('POST', place, { clientOrderId: caller, size: '1' }).status, 200);
  assert.equal(tape.respond('POST', place, { clientOrderId: 'b3cdedc0a20880baMinted2', size: '2' }).status, 200);
  assert.equal(tape.respond('POST', place, { clientOrderId: 'b3cdedc0a20880baMinted3', size: '2' }).status, 599);
  assert.equal(tape.respond('GET', `${instruments}?runtime`).status, 200);
  assert.equal(tape.respond('GET', `${instruments}?instType=SWAP`).status, 599);
  assert.equal(tape.unusedWrites, 0);
});

test('opt-in WebSocket requests compare login credentials and native topics across batching and key order', () => {
  const ws = 'wss://openapi.blofin.com/ws/private';
  const make = () => {
    const tape = createTape([], { rust: 'blofin', requestBodies: true }, true);
    return [tape, tape.connect(ws, () => {}, () => {})];
  };
  const [first, a] = make();
  const [second, b] = make();
  first.send(a, '{"op":"login","args":[{"apiKey":"key","passphrase":"pass","timestamp":"1","nonce":"one","sign":"a"}]}');
  second.sent(b, '{"args":[{"sign":"b","nonce":"two","timestamp":"2","passphrase":"pass","apiKey":"key"}],"op":"login"}');
  first.send(a, '{"op":"subscribe","args":[{"channel":"orders","instType":"ANY"},{"channel":"positions"}]}');
  second.send(b, '{"args":[{"instType":"ANY","channel":"orders"}],"op":"subscribe"}');
  second.send(b, '{"args":[{"channel":"positions"}],"op":"subscribe"}');
  first.send(a, 'ping');
  second.send(b, 'pong');
  assert.deepEqual([...first.sentRequests].sort(), [...second.sentRequests].sort());
  assert.equal(first.sentRequests.size, 3);
  second.send(b, '{"op":"unsubscribe","args":[{"channel":"orders","instType":"ANY"}]}');
  assert.notDeepEqual([...first.sentRequests].sort(), [...second.sentRequests].sort());
  const [third, c] = make();
  third.send(c, '{"op":"login","args":[{"apiKey":"different","passphrase":"pass","timestamp":"1","nonce":"one","sign":"a"}]}');
  assert.notEqual([...first.sentRequests][0], [...third.sentRequests][0]);
  const ordinary = createTape([], { rust: 'cryptocom' }, false);
  ordinary.send(ordinary.connect(ws, () => {}, () => {}), '{"op":"subscribe","args":[{"channel":"orders"}]}');
  assert.equal(ordinary.sentRequests.size, 0);
});

test('nested Bitget caller IDs retain exact identity through path-scoped minting', () => {
  const place = 'https://api.bitget.com/api/v3/trade/place-order';
  const tape = createTape([
    { type: 'command', payload: { order: { clientOrderId: 'nested-caller' } } },
    response({ clientOid: 'nested-caller' }, { url: place }),
  ], venue, true);
  assert.equal(tape.respond('POST', place, { clientOid: 'fresh-generated' }).status, 599);
  assert.equal(tape.respond('POST', place, { clientOid: 'nested-caller' }).status, 200);
});

// Incoming shared transport cases exercise the same venue-aware factory.
{
const mexcTape = (records, { isPrivate = false } = {}) => createTape(records, { rust: 'mexc', matchRequestBody: true }, isPrivate);

const url = 'https://api.mexc.com/api/v1/private/order/cancel';
test('POST matching preserves snowflakes and consumes every accepted write once', () => {
  const tape = mexcTape([
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
  assert.notEqual(canonicalBody('[739113577038255617]'), canonicalBody('[739113577038255618]'));
});

test('body matching ignores only generated externalOid and keeps nested business fields', () => {
  assert.equal(
    canonicalBody('[{"externalOid":"random1","price":"59000","vol":60}]'),
    canonicalBody('[{"vol":60,"price":"59000","externalOid":"random2"}]'),
  );
  assert.notEqual(canonicalBody('{"vol":60}'), canonicalBody('{"vol":61}'));
  assert.notEqual(canonicalBody('{"symbol":"BTC_USDT"}'), canonicalBody('{"symbol":"ETH_USDT"}'));
});

test('only GET may reuse its last recorded snapshot', () => {
  const tape = mexcTape([
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
    canonicalUrl(`${active}?start_time=10&end_time=100&states=1`),
    canonicalUrl(`${active}?states=1&end_time=200&start_time=110`),
  );
  assert.notEqual(
    canonicalUrl(`${active}?start_time=10&end_time=100`),
    canonicalUrl(`${active}?start_time=11&end_time=100`),
  );
  const history = 'https://api.mexc.com/api/v1/private/order/list/history_orders';
  assert.notEqual(
    canonicalUrl(`${history}?start_time=10&end_time=100`),
    canonicalUrl(`${history}?start_time=110&end_time=200`),
  );
  assert.equal(
    canonicalUrl('https://fapi.binance.com/fapi/v1/order?symbol=BTCUSDT&timestamp=1&signature=a'),
    canonicalUrl('https://fapi.binance.com/fapi/v1/order?signature=b&timestamp=2&symbol=BTCUSDT'),
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
  const tape = mexcTape(records);
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
  const tape = mexcTape([{ type: 'command', t: 0, op: 'example' }], { isPrivate: true });
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
    canonicalUrl('wss://stream.bybit.com/v5/private?requestId=recorded'),
    canonicalUrl('wss://stream.bybit.com/v5/private?requestId=runtime'),
  );
  assert.equal(
    canonicalBody('{"requestId":"recorded","qty":"1","orderLinkId":"caller"}'),
    canonicalBody('{"qty":"1","orderLinkId":"caller","requestId":"runtime"}'),
  );
  assert.notEqual(canonicalBody('{"orderLinkId":"caller"}'), canonicalBody('{"orderLinkId":"different"}'));
  assert.notEqual(canonicalBody('{"qty":"1"}'), canonicalBody('{"qty":"2"}'));
});

test('legacy Bybit sockets match generated URLs and retain latest-connection routing', { timeout: 1000 }, async () => {
  const url = 'wss://stream.bybit.com/v5/private';
  const tape = mexcTape([{ type: 'ws', t: 0, url, text: 'private' }]);
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
});

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

test('MEXC factory normalizes only rolling list windows and preserves caller external IDs', () => {
  const active = 'https://api.mexc.com/api/v1/private/planorder/list/orders';
  const history = 'https://api.mexc.com/api/v1/private/order/list/history_orders';
  const submit = 'https://api.mexc.com/api/v1/private/order/submit';
  const tape = mexcTape([
    { type: 'command', orders: [{ clientOrderId: 'caller' }] },
    response(undefined, { method: 'GET', url: `${active}?start_time=10&end_time=100&states=1` }),
    response(undefined, { method: 'GET', url: `${history}?start_time=10&end_time=100` }),
    response({ externalOid: 'caller', vol: 2 }, { url: submit }),
  ]);
  assert.equal(tape.respond('GET', `${active}?states=1&end_time=200&start_time=111`).status, 599);
  assert.equal(tape.respond('GET', `${active}?states=1&end_time=200&start_time=110`).status, 200);
  assert.equal(tape.respond('GET', `${history}?end_time=200&start_time=110`).status, 599);
  assert.equal(tape.respond('GET', `${history}?end_time=100&start_time=10`).status, 200);
  assert.equal(tape.respond('POST', submit, { externalOid: 'wrong', vol: 2 }).status, 599);
  assert.equal(tape.unusedWrites, 1);
  assert.equal(tape.respond('POST', submit, { externalOid: 'caller', vol: 2 }).status, 200);
});

test('timer-driven reads use the current virtual time while advancing toward a later frame', async () => {
  const endpoint = 'https://api.mexc.com/api/v1/private/account/assets';
  const tape = mexcTape([
    response(undefined, { method: 'GET', url: endpoint, body: 'old' }),
    response(undefined, { method: 'GET', url: endpoint, body: 'new', availableAfter: 180 }),
    { type: 'command', t: 400, op: 'checkpoint' },
  ]);
  const clock = new ReplayClock(1700000000000);
  const seen = [];
  clock.install();
  try {
    await tape.play(undefined, undefined, async () => {
      seen.push(tape.respond('GET', endpoint).body);
      return [];
    }, clock, () => {
      setTimeout(() => seen.push(tape.respond('GET', endpoint).body), 150);
      setTimeout(() => seen.push(tape.respond('GET', endpoint).body), 250);
    });
    assert.deepEqual(seen, ['old', 'new', 'new']);
  } finally {
    clock.restore();
  }
});

}
