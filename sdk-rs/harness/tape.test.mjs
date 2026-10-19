import assert from 'node:assert/strict';
import { createHmac, createPrivateKey, sign } from 'node:crypto';
import { test } from 'node:test';
import { canonicalBody, sameCommandResults, Tape, withTimeout } from './tape.mjs';
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
