import assert from 'node:assert/strict';
import test from 'node:test';
import { createHmac } from 'node:crypto';
import { gzipSync, gunzipSync, deflateSync, inflateSync } from 'node:zlib';
import { createPrivateRecorder, createSmokeRedactor } from './live-smoke-tape.mjs';
import { Tape, bodyBytes } from './tape.mjs';
import { venueOf } from './venues.mjs';

const secrets = { key: 'real-account-api-key', secret: 'real-account-api-secret', passphrase: 'real-account-passphrase' };
const decoder = new TextDecoder();
const bytes = (text) => new TextEncoder().encode(text);
const makeLive = (request) => ({ now: () => 1700000000000, request, setTimeout, clearTimeout });

test('config replay credentials and response session aliases never persist originals', async () => {
  const output = [];
  const live = makeLive(async () => ({ status: 200, headers: 'set-cookie: account-session=secret-cookie\nx-request-id: keep', body: bytes(JSON.stringify({ listenKey: 'returned-live-token', note: 'returned-live-token', orderId: 'native-order-42', balance: '900' })) }));
  const recorder = createPrivateRecorder(live, { venue: venueOf('binance'), options: { ...secrets, testnet: true }, secrets, write: (record) => output.push(record) });
  await recorder.transport.request({ method: 'POST', url: 'https://demo-fapi.binance.com/fapi/v1/listenKey', headers: `X-MBX-APIKEY: ${secrets.key}` });
  const serialized = JSON.stringify(output);
  for (const secret of [...Object.values(secrets), 'returned-live-token', 'secret-cookie']) assert.ok(!serialized.includes(secret));
  assert.deepEqual(output[1].options, { key: 'replay-key', secret: 'replay-secret', passphrase: 'replay-passphrase', testnet: true });
  const response = JSON.parse(output[2].body);
  assert.equal(response.listenKey, response.note);
  assert.equal(response.orderId, 'native-order-42');
  assert.equal(response.balance, '900');
  assert.match(output[2].headers, /x-request-id: keep/);
});

test('redaction discovers fields before scrubbing arbitrary echoes, JSON and URL escapes', () => {
  const scrub = createSmokeRedactor({ key: 'secret-"quote"/line\nvalue' });
  const session = 'session/that needs+encoding';
  const value = { echo: `Bearer ${session}`, url: `wss://server.example/ws?token=${encodeURIComponent(session)}`, token: session,
    msg: JSON.stringify({ echo: 'secret-"quote"/line\nvalue' }), base64: Buffer.from(session).toString('base64') };
  const safe = scrub.redact(value);
  const wire = JSON.stringify(safe);
  assert.ok(!wire.includes('secret-'));
  assert.ok(!wire.includes('session/that'));
  assert.ok(!wire.includes(encodeURIComponent(session)));
  assert.ok(!wire.includes(Buffer.from(session).toString('base64')));
  assert.match(safe.echo, /live-smoke-session/);
  assert.match(safe.msg, /replay-key/);
});

test('large native order IDs remain exact through sanitized response JSON', () => {
  const scrub = createSmokeRedactor(secrets);
  assert.equal(scrub.redact('{"orderId":8389765519993909012,"price":"1.25","token":"auth-session"}'),
    '{"orderId":8389765519993909012,"price":"1.25","token":"live-smoke-session-1"}');
});

test('canonical URLs remove clocks/signatures, preserve economics and session identity', () => {
  const scrub = createSmokeRedactor(secrets);
  const canonical = scrub.url(`https://username:password@example.com/orders?symbol=BTCUSDT&price=23&timestamp=42&signature=real-sign&apiKey=${secrets.key}&listenKey=session-abc`);
  assert.equal(canonical, 'https://example.com/orders?apiKey=replay-key&listenKey=live-smoke-session-1&price=23&symbol=BTCUSDT');
  assert.equal(scrub.redact({ listenKey: 'session-abc' }).listenKey, 'live-smoke-session-1');
});

test('raw, gzip and deflate binary frames redact before base64 encoding', () => {
  const scrub = createSmokeRedactor(secrets);
  const text = `{"apiKey":"${secrets.key}","orderId":"own-order","price":"9"}`;
  for (const [encode, decode] of [[Buffer.from, (value) => value], [gzipSync, gunzipSync], [deflateSync, inflateSync]]) {
    const original = encode(text);
    const recorded = Buffer.from(scrub.binary(original), 'base64');
    const parsed = JSON.parse(decode(recorded).toString());
    assert.deepEqual(parsed, { apiKey: 'replay-key', orderId: 'own-order', price: '9' });
    assert.ok(!recorded.includes(Buffer.from(secrets.key)));
  }
  assert.throws(() => scrub.binary(Uint8Array.from([0xff, 0x00, 0xfe, 0x01])), /cannot safely redact/);
});

test('websocket recording captures opens, sends, binary receives and closes without changing live traffic', () => {
  let callbacks;
  const sent = [];
  const originalUrl = 'wss://stream.example/private?token=live-session-123';
  const live = { ...makeLive(), connect(url, handlers) {
    assert.equal(url, originalUrl); callbacks = handlers;
    return { send: (text) => sent.push(text), close: () => handlers.onClose(1000) };
  } };
  const recorder = createPrivateRecorder(live, { secrets });
  const incoming = [];
  const connection = recorder.transport.connect(originalUrl, { onOpen() {}, onMessage: (text) => incoming.push(text), onBinary: (data) => incoming.push(data), onClose() {} });
  callbacks.onOpen();
  const auth = JSON.stringify({ op: 'login', apiKey: secrets.key, signature: 'live-generated-signature' });
  connection.send(auth);
  callbacks.onMessage('{"orderId":"native-44","token":"live-session-123"}');
  const binary = gzipSync('{"balance":"29","token":"live-session-123"}');
  callbacks.onBinary(binary);
  connection.close();
  assert.deepEqual(sent, [auth]);
  assert.equal(incoming[1], binary);
  const frames = recorder.records.slice(2);
  assert.deepEqual(frames.map((frame) => frame.type), ['open', 'ws', 'ws', 'close']);
  assert.ok(frames.every((frame) => frame.url === 'wss://stream.example/private?token=live-smoke-session-1'));
  assert.ok(!frames.some((frame) => frame.type === 'ws-send'));
  assert.ok(!JSON.stringify(frames).includes('live-session-123'));
  assert.equal(JSON.parse(gunzipSync(Buffer.from(frames[2].binary, 'base64'))).balance, '29');
});

test('streamed HTTP stays streamed to host while recording complete sanitized UTF8', async () => {
  const raw = JSON.stringify({ balance: '90', description: 'café', access_token: 'response-auth-xxx' });
  const encoded = bytes(raw);
  const recorder = createPrivateRecorder(makeLive(async () => ({ status: 200, body: (async function* () {
    yield encoded.subarray(0, 40); yield encoded.subarray(40);
  })() })), { secrets });
  const response = await recorder.transport.request({ method: 'GET', url: 'https://exchange.example/account' });
  assert.equal(recorder.records.length, 2);
  assert.equal(decoder.decode(await bodyBytes(response.body)), raw);
  assert.equal(recorder.records.length, 3);
  assert.deepEqual(JSON.parse(recorder.records[2].body), { balance: '90', description: 'café', access_token: 'live-smoke-session-1' });
});

test('Binance recorded signed writes replay with fresh signatures but exact amount, price and caller ID', async () => {
  const recorder = createPrivateRecorder(makeLive(async () => ({ status: 200, body: bytes('{"orderId":8389765519993909012,"status":"NEW"}') })), { venue: venueOf('binance'), options: { ...secrets, testnet: true }, secrets });
  const frame = { op: 'placeOrders', orders: [{ symbol: 'BTCUSDT', type: 'limit', side: 'buy', amount: 0.001, price: 20000, clientOrderId: 'smoke-caller-123' }] };
  await recorder.command(frame, () => recorder.transport.request({ method: 'POST', url: 'https://demo-fapi.binance.com/fapi/v1/order?symbol=BTCUSDT&quantity=0.001&price=20000&newClientOrderId=smoke-caller-123&timestamp=42&signature=real-signature' }));
  assert.equal(recorder.records[3].afterCommand, 1);
  const tape = new Tape(recorder.records, venueOf('binance'), true);
  tape.commandIndex = 1;
  const signed = (price) => {
    const query = `symbol=BTCUSDT&quantity=0.001&price=${price}&newClientOrderId=smoke-caller-123&timestamp=${Date.now()}&recvWindow=5000`;
    return `https://demo-fapi.binance.com/fapi/v1/order?${query}&signature=${createHmac('sha256', 'replay-secret').update(query).digest('hex')}`;
  };
  const headers = { 'X-MBX-APIKEY': 'replay-key' };
  assert.equal(tape.respond('POST', signed(20001), undefined, headers).status, 599);
  assert.equal(tape.respond('POST', signed(20000), undefined, headers).status, 200);
  assert.equal(tape.respond('POST', signed(20000), undefined, headers).status, 599);
  assert.equal(tape.missingWrites, 0);
});

test('private read snapshots are fenced at their command stage', async () => {
  let balance = '10';
  const recorder = createPrivateRecorder(makeLive(async () => ({ status: 200, body: bytes(JSON.stringify({ balance })) })), { secrets });
  const request = { method: 'GET', url: 'https://exchange.example/account' };
  await recorder.transport.request(request);
  await recorder.command({ op: 'fetchBalance' }, async () => { balance = '9'; await recorder.transport.request(request); });
  const tape = new Tape(recorder.records);
  assert.equal(JSON.parse(tape.respond('GET', request.url).body).balance, '10');
  tape.commandIndex = 1;
  assert.equal(JSON.parse(tape.respond('GET', request.url).body).balance, '9');
});

test('transport and persistence failures expose no original error contents', async () => {
  const recorder = createPrivateRecorder(makeLive(async () => { throw new Error(`failed ${secrets.secret}`); }), { secrets });
  await assert.rejects(recorder.transport.request({ method: 'GET', url: 'https://exchange.example/account' }), { message: 'Live-smoke transport request failed' });
  assert.throws(() => createPrivateRecorder(makeLive(), { secrets, write() { throw new Error(secrets.secret); } }), { message: 'Live-smoke tape write failed' });
});

test('positional Bybit, Phemex and Backpack auth signatures and token aliases are unusable in tapes', () => {
  const scrub = createSmokeRedactor(secrets);
  const cases = [
    { op: 'auth', args: [secrets.key, 1700000000000, 'bybit-live-signature'] },
    { event: 'subscribe', api_key: secrets.key, signed_challenge: 'kraken-live-signature', original_challenge: 'server-public-nonce' },
    { method: 'user.auth', params: ['API', secrets.key, 'phemex-live-signature', 1700000000] },
    { method: 'SUBSCRIBE', params: ['account.orderUpdate'], signature: [secrets.key, 'backpack-live-signature', '1700000000000', '5000'] },
    { action: 'exchange', signature: { r: 'r-live-signature', s: 's-live-signature', v: 27 } },
    { auth: 'lighter-live-token', jwt_token: 'paradex-live-token', bearer: 'returned-bearer-token', authenticated: true },
  ];
  for (const value of cases) {
    const result = scrub.redact(JSON.stringify(value));
    assert.ok(!result.includes('live-signature'));
    assert.ok(!result.includes('live-token'));
    assert.ok(!result.includes('returned-bearer-token'));
    assert.ok(!result.includes(secrets.key));
  }
  assert.equal(JSON.parse(scrub.redact(JSON.stringify(cases[5]))).authenticated, true);
});

test('WebSocket capture failure latches and reports safely while allowing live cleanup', () => {
  let callbacks;
  let received = false;
  let failures = 0;
  let closed = false;
  const live = { ...makeLive(), connect(_url, handlers) { callbacks = handlers; return { send() {}, close() { closed = true; } }; } };
  const recorder = createPrivateRecorder(live, { onFailure(error) { failures++; assert.equal(error.message, 'Live-smoke tape recording failed'); } });
  const socket = recorder.transport.connect('wss://example.com/ws', { onOpen() {}, onMessage() {}, onBinary() { received = true; }, onClose() {} });
  assert.doesNotThrow(() => callbacks.onBinary(Uint8Array.from([0xff, 0xfe, 0x00])));
  assert.equal(received, true);
  assert.equal(failures, 1);
  assert.ok(recorder.failure instanceof Error);
  callbacks.onBinary(Uint8Array.from([0xff, 0xfe, 0x00]));
  assert.equal(failures, 1);
  socket.close();
  assert.equal(closed, true);
  assert.equal(recorder.epoch, 1700000000000);
});

test('live smoke startup frames deliver before readiness exactly once; ordinary tapes cannot opt in by frame alone', async () => {
  const url = 'wss://example.com/private';
  const frame = { type: 'ws', url, t: 0, bootstrap: true, text: '{"auth":true}' };
  const records = [{ type: 'meta', private: true, liveSmoke: true }, { type: 'ws-send', url, text: 'PING' }, frame];
  const tape = new Tape(records, {}, true, 1000);
  let delivered = 0;
  tape.connect(url, () => {}, () => { delivered++; }, () => {});
  await tape.play(undefined, async () => { assert.equal(delivered, 1); });
  assert.equal(delivered, 1);
  const ordinary = new Tape([{ type: 'meta', private: true }, frame]);
  assert.equal(ordinary.bootstrapFrame(frame), false);
});

test('markReady fences startup auth tagging without changing later business frames', () => {
  let callbacks;
  const live = { ...makeLive(), connect(_url, handlers) { callbacks = handlers; return { send() {}, close() {} }; } };
  const recorder = createPrivateRecorder(live);
  recorder.transport.connect('wss://example.com/ws', { onOpen() {}, onMessage() {}, onClose() {} });
  callbacks.onMessage('{"authenticated":true}');
  recorder.markReady();
  callbacks.onMessage('{"orderId":"native-order"}');
  assert.equal(recorder.records[2].bootstrap, true);
  assert.equal(recorder.records[3].bootstrap, undefined);
});

test('venue replay credentials retain typed key formats and nested explicit overrides', () => {
  const scrub = createSmokeRedactor({ key: secrets.key, secret: secrets.secret, 'polymarket.apiKey': 'clob-live-key' },
    { key: 'safe-valid-public-key', secret: 'safe-valid-seed', polymarket: { apiKey: 'safe-clob-api-key' } });
  assert.deepEqual(scrub.redact({ key: secrets.key, secret: secrets.secret, polymarket: { apiKey: 'clob-live-key' } }),
    { key: 'safe-valid-public-key', secret: 'safe-valid-seed', polymarket: { apiKey: 'safe-clob-api-key' } });
});

test('live config uses each signer venue pinned valid replay credentials', () => {
  const cases = [
    ['polymarket', { key: `0x${'22'.repeat(32)}`, 'polymarket.apiKey': 'raw-clob-key', 'polymarket.apiSecret': 'raw-clob-secret', 'polymarket.apiPassphrase': 'raw-clob-pass' },
      { key: `0x${'22'.repeat(32)}`, polymarket: { apiKey: 'raw-clob-key', apiSecret: 'raw-clob-secret', apiPassphrase: 'raw-clob-pass', signerType: 0 } }],
    ['backpack', secrets, secrets],
    ['coinbase', secrets, secrets],
    ['lighter', { secret: secrets.secret, applicationId: '1', apiKeyIndex: '2' }, { secret: secrets.secret, applicationId: '1', apiKeyIndex: '2' }],
    ['orderly', { ...secrets, applicationId: 'actual-account-identity' }, { ...secrets, applicationId: 'actual-account-identity' }],
  ];
  for (const [name, supplied, options] of cases) {
    const venue = venueOf(name);
    const recorder = createPrivateRecorder(makeLive(), { venue, options, secrets: supplied });
    const configured = recorder.records[1].options;
    if (name === 'polymarket') {
      assert.equal(configured.key, `0x${'01'.repeat(32)}`);
      assert.equal(configured.polymarket.apiKey, 'poly-replay-key');
      assert.equal(configured.polymarket.signerType, 0);
    } else {
      for (const [key, expected] of Object.entries({ ...venue.options, ...venue.replayCredentials })) {
        if (key in options) assert.equal(configured[key], expected, `${name}.${key}`);
      }
    }
  }
});

test('short numeric identity config substitutes without rewriting economic string fields', () => {
  const scrub = createSmokeRedactor({ applicationId: '1' }, { applicationId: '42' });
  assert.deepEqual(scrub.redact({ applicationId: '1', orderId: '1', price: '1', amount: 1 }), { applicationId: '42', orderId: '1', price: '1', amount: 1 });
});

test('form auth fields and escaped root JSON strings scrub dynamic secrets', () => {
  const scrub = createSmokeRedactor();
  assert.equal(scrub.redact('access_token=dynamic-live-token&orderId=native-1'), 'access_token=live-smoke-session-1&orderId=native-1');
  assert.equal(scrub.redact('"dynamic-live-\\u0074oken"'), '"live-smoke-session-1"');
});

test('synchronous connection creation failures expose no credential-bearing message', () => {
  const recorder = createPrivateRecorder({ ...makeLive(), connect() { throw new Error(`unavailable ${secrets.key}`); } }, { secrets });
  assert.throws(() => recorder.transport.connect('wss://example.com/ws', {}), { message: 'Live-smoke transport connection failed' });
});

test('Bearer and cookie credentials discovered in headers also redact later token-only echoes', () => {
  const scrub = createSmokeRedactor();
  const headers = scrub.headers('authorization: Bearer private-jwt-value\nset-cookie: session=private-cookie-value; Path=/; HttpOnly');
  assert.ok(!headers.includes('private-'));
  assert.ok(!scrub.redact('server echoed private-jwt-value and private-cookie-value').includes('private-'));
});

test('Paradex derived JWT maps to the existing independent HTTP and socket auth oracle', async () => {
  const venue = venueOf('paradex');
  const recorder = createPrivateRecorder(makeLive(async (request) => ({ status: 200, body: bytes(request.url.includes('/auth/') ? '{"jwt_token":"actual-derived-paradex-jwt"}' : '{"results":[]}' ) })), {
    venue, options: { key: `0x${'22'.repeat(32)}`, paradexAddress: '0xabcdef', subkeyAddress: '0xabcdee' },
    secrets: { key: `0x${'22'.repeat(32)}`, paradexAddress: '0xabcdef', subkeyAddress: '0xabcdee' },
  });
  await recorder.transport.request({ method: 'POST', url: 'https://api.testnet.paradex.trade/v1/auth/0xabcdee' });
  await recorder.transport.request({ method: 'GET', url: 'https://api.testnet.paradex.trade/v1/balance', headers: { authorization: 'Bearer actual-derived-paradex-jwt' } });
  assert.equal(JSON.parse(recorder.records[2].body).jwt_token, 'replay-paradex-jwt');
  const tape = new Tape(recorder.records, venue, true);
  assert.equal(tape.respond('GET', 'https://api.testnet.paradex.trade/v1/balance', undefined, { authorization: 'Bearer replay-paradex-jwt' }).status, 200);
  const auth = recorder.redact({ method: 'auth', id: 1, params: { bearer: 'actual-derived-paradex-jwt' } });
  assert.deepEqual(venue.socketReplies('wss://ws.api.testnet.paradex.trade/v1', auth), ['{"jsonrpc":"2.0","id":1,"result":{}}']);
});

test('Polymarket derived L2 credentials preserve the protocol private subscription identity', async () => {
  const venue = venueOf('polymarket');
  const wallet = `0x${'22'.repeat(32)}`;
  const original = { apiKey: 'derived-real-poly-key', secret: 'derived-real-poly-secret', passphrase: 'derived-real-poly-passphrase' };
  const recorder = createPrivateRecorder(makeLive(async () => ({ status: 200, body: bytes(JSON.stringify(original)) })), { venue, options: { key: wallet }, secrets: { key: wallet } });
  await recorder.transport.request({ method: 'GET', url: 'https://clob.polymarket.com/auth/derive-api-key' });
  assert.deepEqual(JSON.parse(recorder.records[2].body), { apiKey: 'poly-replay-key', secret: Buffer.from('poly-replay-secret').toString('base64url'), passphrase: 'poly-replay-pass' });
  const expected = venue.startupSubscriptions(recorder.records, []);
  const { subscriptionKeys } = await import('./requests.mjs');
  const keys = subscriptionKeys('wss://ws-subscriptions-clob.polymarket.com/ws/user', recorder.redact({ type: 'user', auth: original }), new Set(), venue);
  assert.deepEqual(keys, expected);
});

test('Lighter semantic account indices map in URLs, channel identities and account JSON without changing economics', async () => {
  const venue = venueOf('lighter');
  const supplied = { secret: 'actual-lighter-private-key', applicationId: '1', apiKeyIndex: '2' };
  const recorder = createPrivateRecorder(makeLive(async () => ({ status: 200, body: bytes('{"accounts":[{"index":1,"account_index":1,"price":"1","order_id":"1","balance":"99"}],"AccountIndex":1,"ApiKeyIndex":2}') })), { venue, secrets: supplied, options: supplied });
  await recorder.transport.request({ method: 'GET', url: 'https://testnet-api.lighter.xyz/api/v1/account?by=index&value=1' });
  await recorder.transport.request({ method: 'GET', url: 'https://testnet-api.lighter.xyz/api/v1/nextNonce?account_index=1&api_key_index=2' });
  assert.equal(recorder.records[2].url, 'https://testnet-api.lighter.xyz/api/v1/account?by=index&value=42');
  assert.equal(recorder.records[3].url, 'https://testnet-api.lighter.xyz/api/v1/nextNonce?account_index=42&api_key_index=3');
  assert.deepEqual(JSON.parse(recorder.records[2].body), { accounts: [{ index: 42, account_index: 42, price: '1', order_id: '1', balance: '99' }], AccountIndex: 42, ApiKeyIndex: 3 });
  const original = { type: 'subscribe', channel: 'account_all_orders/1', auth: 'actual-lighter-auth-token' };
  const { subscriptionKeys } = await import('./requests.mjs');
  const url = 'wss://testnet.zklighter.elliot.ai/stream';
  assert.deepEqual(subscriptionKeys(url, recorder.redact(original), new Set(), venue), subscriptionKeys(url, { ...original, channel: 'account_all_orders/42', auth: 'fresh-verified-replay-auth' }, new Set(), venue));
  assert.deepEqual(recorder.redact({ channel: 'account_all:1', orderId: '1', price: 1 }), { channel: 'account_all:42', orderId: '1', price: 1 });
});

test('nested signed form transactions redact signatures and remap only scoped Lighter identity fields', () => {
  const scrub = createSmokeRedactor({ applicationId: '1', apiKeyIndex: '2' }, { applicationId: '42', apiKeyIndex: '3' }, { venue: 'lighter' });
  const form = new URLSearchParams({ tx_type: '14', tx_info: '{"AccountIndex":1,"ApiKeyIndex":2,"Signature":"signed-live-transaction","BaseAmount":1,"Price":1}' });
  const safe = new URLSearchParams(scrub.redact(form.toString()));
  assert.deepEqual(JSON.parse(safe.get('tx_info')), { AccountIndex: 42, ApiKeyIndex: 3, Signature: '<redacted-signature>', BaseAmount: 1, Price: 1 });
});

test('live smoke bootstrap replays an interrupted startup generation before readiness', async () => {
  const url = 'wss://example.com/private';
  const tape = new Tape([
    { type: 'meta', liveSmoke: true, private: true },
    { type: 'open', t: 0, url, socket: 0, bootstrap: true },
    { type: 'close', t: 1, url, socket: 0, code: 1006, bootstrap: true },
    { type: 'open', t: 2, url, socket: 1, bootstrap: true },
    { type: 'ws', t: 3, url, socket: 1, text: 'authenticated', bootstrap: true },
  ], {}, true, 1000);
  let received = 0;
  let closes = 0;
  const connect = () => tape.connect(url, () => {}, () => received++, () => { closes++; connect(); });
  connect();
  await tape.play(undefined, async () => { assert.equal(received, 1); assert.equal(closes, 1); });
  assert.equal(received, 1);
  assert.equal(closes, 1);
});

test('seal drops delayed socket callbacks and in-flight HTTP capture after final tape disposition', async () => {
  let handlers;
  let finish;
  const written = [];
  const live = { ...makeLive(() => new Promise((resolve) => { finish = resolve; })), connect(_url, callbacks) {
    handlers = callbacks; return { send() {}, close() {} };
  } };
  const recorder = createPrivateRecorder(live, { write: (value) => written.push(value) });
  recorder.transport.connect('wss://example.com/ws', { onOpen() {}, onMessage() {}, onClose() {} });
  const pending = recorder.transport.request({ method: 'GET', url: 'https://example.com/account' });
  recorder.seal();
  handlers.onOpen(); handlers.onMessage('{"account":"late"}'); handlers.onClose(1000);
  finish({ status: 200, body: bytes('{"balance":"late"}') });
  await pending;
  assert.equal(written.length, 2);
  assert.equal(recorder.records.length, 2);
});

test('recorder persists only explicit Orderly request correlation, omitting auth, login and heartbeat sends', () => {
  let sent = 0;
  const live = { ...makeLive(), connect() { return { send() { sent++; }, close() {} }; } };
  const recorder = createPrivateRecorder(live, { venue: venueOf('orderly'), secrets });
  const socket = recorder.transport.connect('wss://testnet-ws-evm.orderly.org/ws/stream/replay', { onOpen() {}, onMessage() {}, onClose() {} });
  socket.send(JSON.stringify({ event: 'auth', params: { orderly_key: secrets.key, sign: 'unknown-live-signature' } }));
  socket.send(JSON.stringify({ op: 'login', args: [secrets.key, 'other-live-signature'] }));
  socket.send('PING');
  const request = { id: 'request-1', event: 'request', params: { type: 'orderbook', symbol: 'PERP_BTC_USDC' } };
  socket.send(JSON.stringify(request));
  assert.equal(sent, 4);
  assert.deepEqual(recorder.records.slice(2).map((record) => [record.type, JSON.parse(record.text)]), [['ws-send', request]]);
  assert.ok(!JSON.stringify(recorder.records).includes('live-signature'));
});

test('all registered venue auth header signatures are discovered before arbitrary response echoes', async () => {
  const names = ['KC-API-SIGN', 'KC-API-PARTNER-SIGN', 'ACCESS-SIGN', 'OK-ACCESS-SIGN', 'X-BAPI-SIGN', 'API-Sign', 'X-CH-SIGN', 'X-Signature', 'Authent', 'X-PHEMEX-REQUEST-SIGNATURE', 'orderly-signature', 'APEX-SIGNATURE', 'POLY_SIGNATURE', 'x-api-signature'];
  for (const [index, name] of names.entries()) {
    const signature = `generated-live-auth-signature-${index}`;
    const recorder = createPrivateRecorder(makeLive(async () => ({ status: 200, headers: `${name}: ${signature}`,
      body: bytes(JSON.stringify({ echo: signature, encoded: encodeURIComponent(signature), orderId: 'native-order-42', price: '10.5' })) })), { secrets });
    await recorder.transport.request({ method: 'POST', url: 'https://exchange.example/private', headers: { [name]: signature } });
    assert.ok(!JSON.stringify(recorder.records).includes(signature), `persisted ${name}`);
    const body = JSON.parse(recorder.records[2].body);
    assert.equal(body.orderId, 'native-order-42');
    assert.equal(body.price, '10.5');
  }
});

test('newly returned venue credential headers and bearer aliases are scrubbed before echoes', () => {
  const scrub = createSmokeRedactor();
  for (const [index, name] of ['ACCESS-KEY', 'OK-ACCESS-KEY', 'orderly-key', 'X-PHEMEX-ACCESS-TOKEN', 'Coinbase-Authorization'].entries()) {
    const token = `returned-dynamic-credential-${index}`;
    const original = name === 'Coinbase-Authorization' ? `Bearer ${token}` : token;
    assert.ok(!scrub.headers(`${name}: ${original}`).includes(token), name);
    assert.ok(!scrub.redact(`server echo ${token}`).includes(token), `${name} echo`);
  }
  assert.equal(scrub.redact('{"design":"keep-economic-field","triggerSignal":"last"}'), '{"design":"keep-economic-field","triggerSignal":"last"}');
});

test('canonical form encoding cannot retain known credentials containing spaces, quotes or tildes', () => {
  const secret = 'private key ~ with "quotes"';
  const scrub = createSmokeRedactor({ key: secret });
  const form = new URLSearchParams({ apiKey: secret }).toString();
  const url = scrub.url(`https://exchange.example/private?${form}&symbol=BTCUSDT`);
  assert.equal(url, 'https://exchange.example/private?apiKey=replay-key&symbol=BTCUSDT');
  assert.ok(!scrub.redact(`echo ${form}`).includes(form));
});

test('short credential, session and signature material refuses instead of preserving arbitrary echoes', () => {
  for (const name of ['key', 'secret', 'passphrase', 'privateKey', 'polymarket.apiKey', 'signature', 'signed_challenge']) {
    assert.throws(() => createSmokeRedactor({ [name]: 'abc' }), { message: 'Live-smoke credential material is too short to record safely' });
  }
  const scrub = createSmokeRedactor({ applicationId: '1', apiKeyIndex: '2' }, { applicationId: '42', apiKeyIndex: '3' });
  for (const name of ['token', 'listenKey', 'auth', 'jwt_token', 'ACCESS-SIGN']) {
    assert.throws(() => scrub.redact({ [name]: 'abc' }), /too short to record safely/);
    assert.throws(() => scrub.redact(JSON.stringify({ nested: { [name]: 'abc' } })), /too short to record safely/);
  }
  // Refuse even if a public metadata alias already exists for the same bytes.
  assert.throws(() => scrub.redact({ token: '1' }), /too short to record safely/);
  assert.throws(() => scrub.url('wss://example.com/private?token=abc'), /too short to record safely/);
  assert.throws(() => scrub.headers('ACCESS-SIGN: abc'), /too short to record safely/);
  assert.deepEqual(scrub.redact({ applicationId: '1', apiKeyIndex: '2', price: '1', orderId: '2' }),
    { applicationId: '42', apiKeyIndex: '3', price: '1', orderId: '2' });
});

test('short dynamic HTTP auth response never reaches the tape and latches recording failure', async () => {
  const recorder = createPrivateRecorder(makeLive(async () => ({ status: 200, body: bytes('{"token":"abc","echo":"abc"}') })));
  await assert.rejects(recorder.transport.request({ method: 'GET', url: 'https://example.com/auth' }), { message: 'Live-smoke transport response recording failed' });
  assert.ok(recorder.failure);
  assert.equal(recorder.records.length, 2);
  assert.ok(!JSON.stringify(recorder.records).includes('abc'));
});

test('short dynamic WebSocket auth refuses capture while failure latching preserves cleanup transport', () => {
  let callbacks, fed = false, closed = false;
  const sent = [];
  const live = { ...makeLive(), connect(_url, handlers) {
    callbacks = handlers; return { send: (text) => sent.push(text), close() { closed = true; } };
  } };
  const recorder = createPrivateRecorder(live);
  const socket = recorder.transport.connect('wss://example.com/private', { onOpen() {}, onMessage() { fed = true; }, onClose() {} });
  assert.doesNotThrow(() => callbacks.onMessage('{"token":"abc"}'));
  assert.equal(fed, true);
  assert.ok(recorder.failure);
  const cancel = '{"op":"cancel","orderId":"native-order-44"}';
  socket.send(cancel);
  socket.close();
  assert.deepEqual(sent, [cancel]);
  assert.equal(closed, true);
  assert.ok(!JSON.stringify(recorder.records).includes('abc'));
});

const assertReadStages = (records, url, method = 'GET', body) => {
  const tape = new Tape(records);
  assert.equal(JSON.parse(tape.respond(method, url, body).body).balance, '10');
  tape.commandIndex = 1;
  assert.equal(JSON.parse(tape.respond(method, url, body).body).balance, '9');
};

test('a delayed HTTP read is unavailable before the command stage where it completes', async () => {
  let resolveResponse, calls = 0;
  const url = 'https://example.com/account';
  const recorder = createPrivateRecorder(makeLive(() => ++calls === 1
    ? Promise.resolve({ status: 200, body: bytes('{"balance":"10"}') })
    : new Promise((resolve) => { resolveResponse = resolve; })));
  await recorder.transport.request({ method: 'GET', url });
  const pending = recorder.transport.request({ method: 'GET', url });
  await recorder.command({ op: 'setLeverage', symbol: 'BTCUSDT', leverage: 10 }, async () => {});
  resolveResponse({ status: 200, body: bytes('{"balance":"9"}') });
  await pending;
  assert.equal(recorder.records.at(-1).afterCommand, 1);
  assertReadStages(recorder.records, url);
});

test('a streamed read is staged only when its final response body bytes arrive', async () => {
  let finishBody, beganBody, calls = 0;
  const url = 'https://example.com/account';
  const bodyStarted = new Promise((resolve) => { beganBody = resolve; });
  const bodyReady = new Promise((resolve) => { finishBody = resolve; });
  const recorder = createPrivateRecorder(makeLive(async () => ++calls === 1
    ? { status: 200, body: bytes('{"balance":"10"}') }
    : { status: 200, body: (async function* () { yield bytes('{"balance":'); beganBody(); await bodyReady; yield bytes('"9"}'); })() }));
  await recorder.transport.request({ method: 'GET', url });
  const response = await recorder.transport.request({ method: 'GET', url });
  const consumed = bodyBytes(response.body);
  await bodyStarted;
  await recorder.command({ op: 'setLeverage', symbol: 'BTCUSDT', leverage: 10 }, async () => {});
  finishBody();
  await consumed;
  assert.equal(recorder.records.at(-1).afterCommand, 1);
  assertReadStages(recorder.records, url);
});

test('POST reads use completion stage while mutation acknowledgements retain their issuing command', async () => {
  let finishPost, finishWrite;
  const url = 'https://api.hyperliquid-testnet.xyz/info';
  const body = '{"type":"clearinghouseState","user":"fake-account-address"}';
  let calls = 0;
  const recorder = createPrivateRecorder(makeLive((request) => {
    if (request.url.endsWith('/exchange')) return new Promise((resolve) => { finishWrite = resolve; });
    return ++calls === 1 ? Promise.resolve({ status: 200, body: bytes('{"balance":"10"}') })
      : new Promise((resolve) => { finishPost = resolve; });
  }));
  await recorder.transport.request({ method: 'POST', url, body });
  const post = recorder.transport.request({ method: 'POST', url, body });
  let write;
  await recorder.command({ op: 'placeOrders', orders: [] }, async () => {
    write = recorder.transport.request({ method: 'POST', url: 'https://api.hyperliquid-testnet.xyz/exchange', body: '{"action":{"type":"order"}}' });
  });
  await recorder.command({ op: 'cancelSymbolOrders', symbol: 'BTCUSDT' }, async () => {});
  finishPost({ status: 200, body: bytes('{"balance":"9"}') });
  finishWrite({ status: 200, body: bytes('{"status":"ok"}') });
  await Promise.all([post, write]);
  assert.equal(recorder.records.find((row) => row.type === 'http' && row.body === '{"balance":"9"}').afterCommand, 2);
  assert.equal(recorder.records.find((row) => row.type === 'http' && row.url.includes('/exchange')).afterCommand, 1);
  const tape = new Tape(recorder.records);
  assert.equal(JSON.parse(tape.respond('POST', url, body).body).balance, '10');
  tape.commandIndex = 1;
  assert.equal(JSON.parse(tape.respond('POST', url, body).body).balance, '10');
  tape.commandIndex = 2;
  assert.equal(JSON.parse(tape.respond('POST', url, body).body).balance, '9');
});

test('short auth headers or URLs latch failure and cannot leave an incomplete successful capture', async () => {
  let requests = 0;
  const live = makeLive(async () => { requests++; return { status: 200, headers: 'x-access-token: abc', body: bytes('{}') }; });
  const outbound = createPrivateRecorder(live);
  await assert.rejects(outbound.transport.request({ method: 'GET', url: 'https://example.com/account', headers: { 'ACCESS-SIGN': 'abc' } }), { message: 'Live-smoke tape redaction failed' });
  assert.ok(outbound.failure);
  assert.equal(requests, 0);
  const url = createPrivateRecorder(live);
  await assert.rejects(url.transport.request({ method: 'GET', url: 'https://example.com/account?token=abc' }), { message: 'Live-smoke tape redaction failed' });
  assert.ok(url.failure);
  assert.equal(requests, 0);
  const inbound = createPrivateRecorder(live);
  await assert.rejects(inbound.transport.request({ method: 'GET', url: 'https://example.com/account' }), { message: 'Live-smoke tape redaction failed' });
  assert.ok(inbound.failure);
  assert.equal(requests, 1);
  assert.equal(inbound.records.length, 2);
});

test('unrecordable UTF8 response bytes latch failure for both buffered and streamed HTTP', async () => {
  for (const streamed of [false, true]) {
    const invalid = Uint8Array.from([0xff, 0xfe, 0x80]);
    const recorder = createPrivateRecorder(makeLive(async () => ({ status: 200,
      body: streamed ? (async function* () { yield bytes('{"balance":"9"}'); yield invalid; })() : invalid })));
    const work = (async () => {
      const response = await recorder.transport.request({ method: 'GET', url: 'https://example.com/account' });
      if (streamed) await bodyBytes(response.body);
    })();
    await assert.rejects(work, /Live-smoke (?:tape redaction|transport response recording) failed/);
    assert.ok(recorder.failure, `missing failure latch for ${streamed ? 'streamed' : 'buffered'} response`);
    assert.equal(recorder.records.length, 2);
  }
});


test('unknown response extensions and header names cannot persist newly introduced credentials', () => {
  const scrub = createSmokeRedactor();
  const safe = scrub.redact({ price: '12.5', orderId: 'order-123',
    unexpectedExtension: { price: 'previously-unknown-credential' },
    data: { futureSessionMaterial: 'another-unknown-credential', amount: '2' },
    note: 'previously-unknown-credential another-unknown-credential' });
  assert.deepEqual(Object.keys(safe), ['price', 'orderId', 'data', 'note']);
  assert.deepEqual(safe.data, { amount: '2' });
  const wire = JSON.stringify(safe);
  assert.ok(!wire.includes('previously-unknown-credential'));
  assert.ok(!wire.includes('another-unknown-credential'));
  assert.equal(safe.price, '12.5');
  assert.equal(safe.orderId, 'order-123');
  const headers = scrub.headers('X-Future-Credential: unknown-header-material\nSet-Cookie: session=private-cookie-credential; HttpOnly\nx-request-id: request-1');
  assert.equal(headers, 'x-request-id: request-1');
  assert.ok(!scrub.redact('unknown-header-material private-cookie-credential').includes('credential'));
  const url = scrub.url('https://example.com/account?futurePrivateValue=unknown-query-material&symbol=BTCUSDT');
  assert.equal(url, 'https://example.com/account?symbol=BTCUSDT');
  assert.ok(!scrub.redact('unknown-query-material').includes('unknown-query-material'));
});


test('nested form strings and URL strings apply the same positive schema as objects', () => {
  const scrub = createSmokeRedactor();
  const safe = scrub.redact({ body: 'unlistedCredential=unknown-form-secret&symbol=BTCUSDT',
    message: 'https://venue.test/path?unlistedCredential=unknown-url-secret&symbol=BTCUSDT',
    note: 'unknown-form-secret unknown-url-secret' });
  assert.equal(safe.body, 'symbol=BTCUSDT');
  assert.equal(safe.message, 'https://venue.test/path?symbol=BTCUSDT');
  assert.ok(!JSON.stringify(safe).includes('unknown-form-secret'));
  assert.ok(!JSON.stringify(safe).includes('unknown-url-secret'));
  const nested = new URLSearchParams({ tx_info: JSON.stringify({ Price: 100, unlistedCredential: 'nested-json-form-secret' }) });
  assert.deepEqual(JSON.parse(new URLSearchParams(scrub.redact(nested.toString())).get('tx_info')), { Price: 100 });
});


test('compressed recording bombs fail before an expanded secret-bearing message can be persisted', () => {
  const scrub = createSmokeRedactor();
  const oversized = gzipSync(Buffer.alloc(16 * 1024 * 1024 + 1, 'x'));
  assert.throws(() => scrub.binary(oversized), /cannot safely redact/);
});


test('secondary credential aliases and arbitrary secret containers never borrow economic field exemptions', () => {
  const scrub = createSmokeRedactor();
  const original = { readKey: 'private-read-key', readSecret: 'private-read-secret',
    secondary_key: 'private-secondary-key', secondary_secret: 'private-secondary-secret',
    secrets: { price: 'hidden-under-economic-name' },
    note: 'private-read-key private-read-secret private-secondary-key private-secondary-secret hidden-under-economic-name',
    price: '100' };
  const safe = scrub.redact(original);
  for (const secret of [...Object.values(original).slice(0, 4), original.secrets.price])
    assert.ok(!JSON.stringify(safe).includes(secret));
  assert.ok(!Object.hasOwn(safe, 'secrets'));
  assert.equal(safe.price, '100');
});

test('managed recorder holds original command stage across parked preparation and records after dispatch', async () => {
  let finishPrepare;
  const events = [], receivers = [];
  const response = { status: 200, headers: '', body: bytes(JSON.stringify({ orderId: 'native-1', note: secrets.key })) };
  const bound = { ready() { receivers.push(this === bound); return true; }, cancel() { receivers.push(this === bound); },
    dispatch() { receivers.push(this === bound); events.push('send'); return Promise.resolve(response); } };
  const resource = { expiresAt: 200, cancel() { receivers.push(this === resource); },
    bind(packet) { receivers.push(this === resource); assert.equal(packet.headers['X-MBX-APIKEY'], secrets.key); return bound; } };
  const live = { ...makeLive(), prepareHttp() { receivers.push(this === live); return new Promise((resolve) => { finishPrepare = () => resolve(resource); }); } };
  const recorder = createPrivateRecorder(live, { venue: venueOf('binance'), secrets,
    write: (record) => events.push(record.type) });
  const request = { request: 1, method: 'POST', url: 'https://demo-fapi.binance.com/fapi/v1/order?symbol=BTCUSDT', route: 'direct' };
  const preparing = recorder.command({ op: 'placeOrders' }, () => recorder.transport.prepareHttp(request));
  recorder.command({ op: 'fetchBalance' }, () => {});
  finishPrepare(); const prepared = await preparing;
  events.length = 0;
  const packet = { ...request, headers: { 'X-MBX-APIKEY': secrets.key } };
  const ready = prepared.bind(packet);
  packet.method = 'DELETE'; packet.url = 'https://mutated.invalid';
  assert.equal(prepared.expiresAt, 200);
  assert.equal(ready.ready(), true);
  assert.deepEqual(events, []);
  assert.equal(await ready.dispatch(), response);
  const http = recorder.records.filter((row) => row.type === 'http');
  assert.equal(http.length, 1);
  assert.equal(http[0].afterCommand, 1);
  assert.equal(http[0].method, 'POST');
  assert.equal(http[0].url, request.url);
  assert.deepEqual(events, ['send', 'http']);
  assert.equal(JSON.stringify(recorder.records).includes(secrets.key), false);
  assert.equal(JSON.parse(http[0].body).orderId, 'native-1');
  ready.cancel(); prepared.cancel();
  assert.equal(receivers.every(Boolean), true);
});

test('managed recorder writer failure follows physical dispatch and cancels bind failure', async () => {
  let sent = 0, canceled = 0;
  const live = { ...makeLive(), prepareHttp: () => ({
    cancel() { canceled++; },
    bind(packet) {
      if (packet.bad) throw new Error('private bind failure');
      return { ready: () => true, dispatch() { sent++; return Promise.resolve({ status: 200, headers: '', body: bytes('{}') }); } };
    },
  }) };
  const recorder = createPrivateRecorder(live, { secrets, write: (row) => { if (row.type === 'http') throw new Error(secrets.key); } });
  const request = { request: 1, method: 'POST', url: 'https://exchange.example/order', headers: {} };
  const rejected = await recorder.transport.prepareHttp(request);
  assert.throws(() => rejected.bind({ ...request, bad: true }), /transport.*preparation|transport.*bind/i);
  assert.equal(canceled, 1);
  assert.equal(sent, 0);
  const prepared = await recorder.transport.prepareHttp(request), ready = prepared.bind(request);
  await assert.rejects(ready.dispatch(), /write failed|recording failed/);
  assert.equal(sent, 1);
  assert.ok(recorder.failure);
  assert.equal(typeof createPrivateRecorder(makeLive(), { secrets }).transport.prepareHttp, 'undefined');
});


test('managed recorder redaction refusal cancels preparation before bind or send', async () => {
  let canceled = 0, bound = 0;
  const live = { ...makeLive(), prepareHttp: () => ({
    cancel() { canceled++; }, bind() { bound++; throw new Error('must not bind'); },
  }) };
  const recorder = createPrivateRecorder(live, { secrets });
  const request = { method: 'POST', url: 'https://exchange.example/order' };
  const prepared = await recorder.transport.prepareHttp(request);
  assert.throws(() => prepared.bind({ ...request, headers: { signature: 'x' } }), /transport preparation failed/);
  assert.equal(canceled, 1);
  assert.equal(bound, 0);
  assert.ok(recorder.failure);
});
