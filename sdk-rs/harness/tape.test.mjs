import assert from 'node:assert/strict';
import test from 'node:test';
import { createTape, bodyBytes, canonicalBody as mexcBody, canonicalUrl as mexcUrl, replayStoreReady, ReplayClock } from './tape.mjs';
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

test('factory keeps command stage, response time, headers and generated-ID validation independent', () => {
  const stagedVenue = {
    rust: 'bitget',
    checkRequest: (_method, _url, _body, headers) => headers?.auth === 'fixture',
  };
  const tape = createTape([
    response({ orderId: '1', newClientOid: 'expected' }, { afterCommand: 2, availableAfter: 100 }),
  ], stagedVenue, true);
  const body = { orderId: '1', newClientOid: 'fresh' };
  const headers = { auth: 'fixture' };
  tape.commandIndex = 1;
  tape.time = 100;
  assert.equal(tape.respond('POST', url, body, headers).status, 599);
  tape.commandIndex = 2;
  tape.time = 99;
  assert.equal(tape.respond('POST', url, body, headers).status, 599);
  tape.time = 100;
  assert.equal(tape.respond('POST', url, body, {}).status, 599);
  assert.throws(() => tape.respond('POST', url, { ...body, newClientOid: 'bad id' }, headers), /invalid generated/);
  assert.equal(tape.unusedWrites, 1);
  assert.equal(tape.respond('POST', url, body, headers).status, 200);
  assert.equal(tape.unusedWrites, 0);
  assert.equal(tape.respond('POST', url, body, headers).status, 599);
});

test('factory supplies socket audit context and audits frames before venue delivery', async () => {
  const seen = [];
  const books = ['BTCUSD-PERP'];
  const auditVenue = {
    rust: 'cryptocom',
    publicTickers: () => ['BTCUSD-PERP', 'ETHUSD-PERP'],
    createSocketAudit: (impl, context) => {
      assert.equal(impl, 'rust');
      assert.deepEqual(context, { isPrivate: true, books, tickers: ['BTCUSD-PERP', 'ETHUSD-PERP'], records: [] });
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

test('Backpack signature rejection preserves responses and existing cancellation client identities stay exact', () => {
  const endpoint = 'https://api.backpack.exchange/api/v1/order';
  const tape = createTape([
    response({ clientId: 42 }, { method: 'DELETE', url: endpoint }),
  ], {
    rust: 'backpack', isWrite: (method) => method !== 'GET',
    verifyRequest: ({ headers }) => headers?.signature === 'valid',
  }, true);
  assert.throws(() => tape.respond('DELETE', endpoint, { clientId: 42 }, { signature: 'invalid' }), /signature/);
  assert.equal(tape.invalidSignatures, 1);
  assert.equal(tape.missingWrites, 1);
  assert.equal(tape.respond('DELETE', endpoint, { clientId: 43 }, { signature: 'valid' }).status, 599);
  assert.equal(tape.missingWrites, 1);
  assert.equal(tape.respond('DELETE', endpoint, { clientId: 42 }, { signature: 'valid' }).status, 200);
  assert.equal(tape.missingWrites, 0);
});

test('Backpack subscription verification precedes all topic registration and normalized comparisons remain separate', async () => {
  const tape = createTape([], {
    rust: 'backpack', compareSubscriptions: true,
    marketFilter: ({ symbol }) => symbol.endsWith('_PERP'),
    verifySubscribe: (message) => message.signature === 'valid',
  }, true);
  const socket = tape.connect('wss://ws.backpack.exchange', () => {}, () => {});
  const message = { method: 'SUBSCRIBE', params: ['account.orderUpdate', 'depth.SOL_USDC_PERP', 'depth.SOL_USDC'] };
  assert.throws(() => tape.send(socket, JSON.stringify(message)), /signature/);
  assert.equal(tape.invalidSignatures, 1);
  assert.equal(socket.subscriptions.size, 0);
  assert.equal(tape.subscriptions.size, 0);
  assert.equal(tape.normalizedSubscriptions.size, 0);
  tape.send(socket, JSON.stringify({ ...message, signature: 'valid' }));
  assert.equal(tape.subscriptions.size, 3);
  assert.deepEqual([...tape.normalizedSubscriptions], ['SUBSCRIBE account.orderUpdate', 'SUBSCRIBE depth.SOL_USDC_PERP']);
  assert.equal(tape.protocolSubscriptions, tape.subscriptions);
  await new Promise(setImmediate);
});

test('Backpack decimal-string normalization does not weaken Bitget wire matching', () => {
  const place = 'https://api.bitget.com/api/v3/trade/place-order';
  const tape = createTape([response({ price: '1.00' }, { url: place })], venue, true);
  assert.equal(tape.respond('POST', place, { price: '1' }).status, 599);
  assert.equal(tape.respond('POST', place, { price: 1 }).status, 599);
  assert.equal(tape.respond('POST', place, { price: '1.00' }).status, 200);
});

// Incoming shared transport cases exercise the same venue-aware factory.
{
const mexcOptions = { canonicalIntegerStrings: true, ignoredBodyFields: ['externalOid', 'requestId'], rollingListWindows: true };
const canonicalBody = (body) => mexcBody(body, new Set(), mexcOptions);
const canonicalUrl = (url) => mexcUrl(url, new Set(), mexcOptions);
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

import { createHmac, createPrivateKey, sign } from 'node:crypto';
import {
  canonicalBody,
  canonicalUrl,
  sameCommandResults,
  Tape,
  withTimeout,
} from './tape.mjs';
import { venueOf } from './venues.mjs';

{
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


test('venue socket keys preserve bullet tokens while normalizing generated connection IDs', async () => {
  const venue = { volatileQueryFields: ['connectId'] };
  const urlKey = (value) => canonicalUrl(value, new Set(), venue);
  const tape = new Tape([], venue);
  let received;
  tape.connect('wss://bullet.test/socket?token=public&connectId=runtime', () => {}, (text) => { received = text; });
  await new Promise(setImmediate);
  const socket = await tape.socketFor('wss://bullet.test/socket?token=public&connectId=recorded', undefined, 0);
  socket.onMessage('depth');
  assert.equal(received, 'depth');
  assert.notEqual(urlKey(socket.url), urlKey('wss://bullet.test/socket?token=private&connectId=runtime'));
});

test('recorded welcomes establish readiness once without consuming business frames', async () => {
  const url = 'wss://bullet.test/endpoint?token=recorded';
  const records = [
    { type: 'ws', t: 1, url, text: '{"type":"welcome"}' },
    { type: 'ws', t: 2, url, text: '{"type":"message"}' },
  ];
  const tape = new Tape(records, { startupMessage: (r) => JSON.parse(r.text).type === 'welcome' });
  const observed = [];
  tape.connect(url, () => observed.push('open'), (text) => observed.push(JSON.parse(text).type));
  const frames = await tape.play(undefined, async () => {
    await new Promise(setImmediate);
    assert.deepEqual(observed, ['open', 'welcome']);
    observed.push('ready');
  });
  assert.equal(frames, 2);
  assert.deepEqual(observed, ['open', 'welcome', 'ready', 'message']);
});

test('startup replay never invents a welcome or pulls one past business traffic', async () => {
  const url = 'wss://bullet.test/endpoint?token=recorded';
  for (const rows of [[], [
    { type: 'ws', t: 1, url, text: '{"type":"message"}' },
    { type: 'ws', t: 2, url, text: '{"type":"welcome"}' },
  ]]) {
    const tape = new Tape(rows, { startupMessage: (r) => JSON.parse(r.text).type === 'welcome' });
    const observed = [];
    tape.connect(url, () => {}, (text) => observed.push(text));
    await new Promise(setImmediate);
    assert.deepEqual(observed, []);
  }
});


test('KuCoin shared Tape binds signed headers, caller IDs, body types and response stages', () => {
  const venue = venueOf('kucoin');
  const url = 'https://api-futures.kucoin.com/api/v1/orders';
  const body = JSON.stringify({ symbol: 'XBTUSDTM', clientOid: 'caller-exact', size: '2' });
  const tape = new Tape([
    { type: 'command', op: 'placeOrders', orders: [{ clientOrderId: 'caller-exact' }] },
    { type: 'http', method: 'POST', url, requestBody: body, status: 200, body: '{}', availableAfter: 10 },
  ], venue, true);
  const headers = (payload) => {
    const timestamp = String(Date.now());
    const hmac = (value) => createHmac('sha256', 'replay-secret').update(value).digest('base64');
    return { 'KC-API-KEY': 'replay-key', 'KC-API-KEY-VERSION': '2', 'KC-API-TIMESTAMP': timestamp,
      'KC-API-PASSPHRASE': hmac('replay-passphrase'), 'KC-API-SIGN': hmac(timestamp + 'POST/api/v1/orders' + (payload ?? '')) };
  };
  tape.time = 9;
  assert.equal(tape.respond('POST', url, body, headers(body)).status, 599);
  tape.time = 10;
  assert.equal(tape.respond('POST', url, body, {}).status, 599);
  for (const wrong of [undefined, body.replace('"2"', '2'), body.replace('caller-exact', 'generated')]) {
    assert.equal(tape.respond('POST', url, wrong, headers(wrong)).status, 599);
  }
  assert.equal(tape.missingWrites, 1);
  assert.equal(tape.respond('POST', url, body, headers(body)).status, 200);
  assert.equal(tape.respond('POST', url, body, headers(body)).status, 599);
  assert.equal(tape.missingWrites, 0);
});

test('KuCoin shared Tape audits the recorded welcome, topics and JSON pong across connection IDs', async () => {
  const venue = venueOf('kucoin');
  const endpoint = 'wss://bullet.test/socket';
  const url = endpoint + '?token=public-token&connectId=recorded';
  const topic = '/contractMarket/snapshot:XBTUSDTM';
  const records = [
    { type: 'http', method: 'POST', url: 'https://api-futures.kucoin.com/api/v1/bullet-public', status: 200,
      body: JSON.stringify({ code: '200000', data: { token: 'public-token', instanceServers: [{ endpoint }] } }) },
    { type: 'ws', t: 1, url, socket: 0, text: '{"type":"welcome"}' },
    { type: 'ws', t: 2, url, socket: 0, text: JSON.stringify({ type: 'message', topic }) },
  ];
  const tape = new Tape(records, venue, false, 500, 'rust');
  const observed = [];
  const socket = tape.connect(endpoint + '?token=public-token&connectId=runtime', () => {}, (text) => {
    const message = JSON.parse(text);
    observed.push(message);
    if (message.type === 'welcome') {
      tape.send(socket, JSON.stringify({ type: 'subscribe', topic, privateChannel: false }));
      tape.send(socket, '{"type":"ping","id":"heartbeat"}');
    }
  });
  const ticks = [];
  const frames = await tape.play((t) => ticks.push(t));
  await new Promise(setImmediate);
  assert.equal(frames, 2);
  assert.deepEqual(ticks, [1, 2]);
  assert.equal(observed.filter((m) => m.type === 'welcome').length, 1);
  assert(observed.some((m) => m.type === 'pong' && m.id === 'heartbeat'));
  assert([...tape.protocolSubscriptions].some((key) => key.endsWith(' subscribe ' + topic)));
  assert.deepEqual(tape.audit.errors(), []);
});

test('startup welcomes match socket ordinals and cannot bypass recorded closes', async () => {
  const url = 'wss://bullet.test/socket?token=shared';
  const venue = { startupMessage: (r) => JSON.parse(r.text).type === 'welcome' };
  const tape = new Tape([{ type: 'ws', t: 1, url, socket: 1, text: '{"type":"welcome"}' }], venue);
  const seen = [];
  tape.connect(url, () => {}, () => seen.push(0));
  await new Promise(setImmediate);
  assert.deepEqual(seen, []);
  tape.connect(url, () => {}, () => seen.push(1));
  await new Promise(setImmediate);
  assert.deepEqual(seen, [1]);
  const closed = new Tape([
    { type: 'close', t: 0, url, socket: 0 },
    { type: 'ws', t: 1, url, socket: 0, text: '{"type":"welcome"}' },
  ], venue);
  closed.connect(url, () => {}, () => assert.fail('welcome bypassed close'));
  await new Promise(setImmediate);
});
}
