import assert from 'node:assert/strict';
import test from 'node:test';
import { createTape } from './tape.mjs';
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

test('missing sockets fail after thirty seconds and remove their pending waiter', async (t) => {
  t.mock.timers.enable({ apis: ['setTimeout'] });
  const tape = createTape([], venue, false);
  const pending = assert.rejects(tape.socketFor('wss://ws.bitget.com/missing'), /socket did not open/);
  t.mock.timers.tick(30_000);
  await pending;
  assert.equal(tape.waiters.length, 0);
});
