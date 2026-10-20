import assert from 'node:assert/strict';
import { createPrivateKey, sign } from 'node:crypto';
import { test } from 'node:test';
import { ORDERLY_BROKERS, ORDERLY_REPLAY, venueOf } from './venues.mjs';
import { subscriptionKeys, requestKey } from './requests.mjs';
import { Tape } from './tape.mjs';

const venue = venueOf('orderly');
const publicUrl = `wss://ws-evm.orderly.org/ws/stream/${ORDERLY_REPLAY.publicApplicationId}`;
const privateUrl = `wss://ws-private-evm.orderly.org/v2/ws/private/stream/${ORDERLY_REPLAY.applicationId}`;
const privateKey = createPrivateKey({
  key: Buffer.concat([Buffer.from('302e020100300506032b657004220420', 'hex'), Buffer.alloc(32, 11)]),
  format: 'der',
  type: 'pkcs8',
});
const signature = (text) => sign(null, Buffer.from(text), privateKey).toString('base64url');
const auth = () => {
  const timestamp = Date.now();
  return { event: 'auth', params: { orderly_key: ORDERLY_REPLAY.key, timestamp, sign: signature(String(timestamp)) } };
};
const request = (id, symbol = 'PERP_BTC_USDC') => ({ id, event: 'request', params: { type: 'orderbook', symbol } });
const response = (id, symbol = 'PERP_BTC_USDC') => ({
  id,
  event: 'request',
  success: true,
  data: { symbol, ts: 1000, bids: [[100, 2]], asks: [[101, 3]] },
});
const recordedRequest = (id, symbol, socket = 0) => ({
  type: 'ws-send',
  url: publicUrl,
  socket,
  text: JSON.stringify(request(id, symbol)),
});
const connect = (tape, url = publicUrl, received = []) =>
  tape.connect(
    url,
    () => {},
    (text) => received.push(JSON.parse(text)),
    () => {},
  );

test('every Orderly broker selects one frozen comparator and keeps its own Rust name', () => {
  assert.equal(Object.keys(ORDERLY_BROKERS).length, 6);
  for (const name of Object.keys(ORDERLY_BROKERS)) {
    const broker = venueOf(name);
    assert.equal(broker.rust, name);
    assert.equal(broker.ts.name, name);
    assert(broker.ts.module.includes(`/__tests__/reference/orderly/exchanges/${name}/`));
    assert.equal(broker.options.applicationId, ORDERLY_REPLAY.applicationId);
    assert.equal(broker.publicOptions.applicationId, ORDERLY_REPLAY.publicApplicationId);
    assert.equal(broker.requestBodies, true);
    assert.equal(broker.compareWriteOrder, true);
    assert.equal(broker.compareClientIds, true);
  }
  assert.equal(venue.ts.name, 'tealstreet-dex');
  const exchange = { publicWebsocket: {} };
  venue.configureTs(exchange);
  assert.equal(exchange.publicWebsocket.replaySnapshots, true);
  assert.notEqual(venueOf('binance').family, 'orderly');
});

test('event/topic subscriptions retain operation, native symbol and socket path', () => {
  const key = (message) => subscriptionKeys(publicUrl, JSON.stringify(message));
  assert.deepEqual(key({ event: 'subscribe', topic: 'PERP_BTC_USDC@orderbookupdate', id: 'volatile' }), [
    `WS ${publicUrl}? {"event":"subscribe","topic":"PERP_BTC_USDC@orderbookupdate"}`,
  ]);
  assert.notDeepEqual(key({ event: 'subscribe', topic: 'tickers' }), key({ event: 'unsubscribe', topic: 'tickers' }));
  assert.notDeepEqual(key({ event: 'subscribe', topic: 'tickers' }), key({ event: 'subscribe', topic: 'bbos' }));
  assert.equal(key(auth()).length, 1);
  assert.equal(key(request('id')).length, 1);
});

test('Orderly REST verification binds exact bytes, query order, credentials and method', () => {
  const timestamp = String(Date.now());
  const url = 'https://api-evm.orderly.org/v1/order?first=a%20b&second=%2F';
  const body = '{"client_order_id":"caller-1","order_quantity":1,"side":"BUY"}';
  const payload = `${timestamp}POST/v1/order?first=a%20b&second=%2F${body}`;
  const signed = {
    method: 'POST',
    url,
    body,
    headers: {
      'orderly-account-id': ORDERLY_REPLAY.applicationId,
      'orderly-key': ORDERLY_REPLAY.key,
      'orderly-timestamp': timestamp,
      'orderly-signature': signature(payload),
      'Content-Type': 'application/json',
    },
  };
  assert.equal(venue.verifyRequest(signed), true);
  for (const changed of [
    { ...signed, body: body.replace('quantity":1', 'quantity":2') },
    { ...signed, body: body.replace('"order_quantity":1', '"order_quantity":"1"') },
    { ...signed, method: 'PUT' },
    { ...signed, url: url.replace('first=a%20b&second=%2F', 'second=%2F&first=a%20b') },
    { ...signed, url: url.replace('%20', '+') },
    { ...signed, headers: { ...signed.headers, 'orderly-account-id': 'other' } },
    { ...signed, headers: { ...signed.headers, 'orderly-key': ORDERLY_REPLAY.key.slice(8) } },
    { ...signed, headers: { ...signed.headers, 'orderly-timestamp': '1' } },
    { ...signed, headers: { ...signed.headers, 'Content-Type': 'application/x-www-form-urlencoded' } },
  ])
    assert.equal(venue.verifyRequest(changed), false);
});

test('Orderly caller IDs and numerical body types are never normalized away', () => {
  const key = (body) => requestKey('POST', 'https://api-evm.orderly.org/v1/order', body, new Set(['caller']), venue);
  assert.notEqual(key({ client_order_id: 'caller' }), key({ client_order_id: 'changed' }));
  assert.notEqual(key({ client_order_id: 'caller' }), key({}));
  assert.notEqual(key({ order_quantity: 1 }), key({ order_quantity: '1' }));
  assert.notEqual(key({ order_tag: 'TEALSTREET' }), key({ order_tag: 'TEALKODIAK' }));
  assert.notEqual(key('{"order_id":9007199254740992}'), key('{"order_id":9007199254740993}'));
});

test('typed request matching enforces write order and consumes mutations once', () => {
  const url = 'https://api-evm.orderly.org/v1/order';
  const records = [1, 2].map((quantity) => ({
    type: 'http',
    method: 'POST',
    url,
    status: 200,
    requestBody: JSON.stringify({ order_quantity: quantity }),
    body: '{}',
  }));
  const tape = new Tape(records, venue, false);
  assert.equal(tape.respond('POST', url, '{"order_quantity":2}').status, 599);
  assert.equal(tape.missingWrites, 2);
  assert.equal(tape.respond('POST', url, '{"order_quantity":1}').status, 200);
  assert.equal(tape.respond('POST', url, '{"order_quantity":1}').status, 599);
  assert.equal(tape.respond('POST', url, '{"order_quantity":2}').status, 200);
  assert.equal(tape.missingWrites, 0);
});

test('snapshot correlation preserves real depth and binds recorded symbol, ID and generation', () => {
  const tape = new Tape([recordedRequest('taped')], venue, false);
  const received = [],
    socket = connect(tape, publicUrl, received);
  tape.send(socket, JSON.stringify(request('implementation-id')));
  assert.throws(() => socket.onMessage(JSON.stringify(response('taped', 'PERP_ETH_USDC'))), /symbol/);
  assert.throws(() => socket.onMessage(JSON.stringify(response('wrong-id'))), /no taped request/);
  socket.onMessage(JSON.stringify(response('taped')));
  assert.deepEqual(received, [{ ...response('taped'), id: 'implementation-id' }]);
  assert.throws(() => socket.onMessage(JSON.stringify(response('taped'))), /duplicate/);
  assert.throws(() => tape.send(socket, JSON.stringify(request('extra-request'))), /unrecorded/);
  const next = connect(tape);
  assert.throws(() => tape.send(next, JSON.stringify(request('new-generation'))), /unrecorded/);
});

test('superseded snapshot replies retain their old IDs rather than satisfying a new request', () => {
  const tape = new Tape([recordedRequest('old-tape'), recordedRequest('new-tape')], venue, false);
  const received = [],
    socket = connect(tape, publicUrl, received);
  tape.send(socket, JSON.stringify(request('old-runtime')));
  tape.send(socket, JSON.stringify(request('new-runtime')));
  socket.onMessage(JSON.stringify(response('new-tape')));
  socket.onMessage(JSON.stringify(response('old-tape')));
  assert.deepEqual(
    received.map((r) => r.id),
    ['new-runtime', 'old-runtime'],
  );
});

test('Orderly snapshots require recorded request metadata, including failure replies', () => {
  const tape = new Tape([], venue, false);
  const socket = connect(tape);
  assert.throws(() => tape.send(socket, JSON.stringify(request('not-recorded'))), /unrecorded/);
  assert.throws(() => socket.onMessage(JSON.stringify(response('not-recorded'))), /no taped request/);
});

test('failed snapshot responses correlate through taped request metadata without fabricating data', () => {
  const tape = new Tape([recordedRequest('taped')], venue, false);
  const received = [],
    socket = connect(tape, publicUrl, received);
  tape.send(socket, JSON.stringify(request('runtime')));
  socket.onMessage('{"event":"request","id":"taped","success":false,"message":"retry"}');
  assert.deepEqual(received, [{ event: 'request', id: 'runtime', success: false, message: 'retry' }]);
});

test('event/topic routing waits for the actual subscription on the requested socket generation', async () => {
  const tape = new Tape([], venue, false, 20);
  const first = connect(tape),
    second = connect(tape);
  await new Promise((resolve) => setImmediate(resolve));
  tape.send(second, '{"event":"subscribe","topic":"tickers"}');
  const text = '{"topic":"tickers","data":[]}';
  assert.equal(await tape.socketFor(publicUrl, text), second);
  await assert.rejects(tape.socketFor(publicUrl, text, 0), /timeout/);
  tape.send(first, '{"event":"subscribe","topic":"tickers"}');
  assert.equal(await tape.socketFor(publicUrl, text, 0), first);
  tape.send(first, '{"event":"unsubscribe","topic":"tickers"}');
  await assert.rejects(tape.socketFor(publicUrl, text, 0), /timeout/);
});

test('Orderly audit checks auth acknowledgement ordering, topics and server heartbeat responses', () => {
  const audit = venue.createSocketAudit('rust', { isPrivate: true, books: ['BTCUSDC'] });
  for (const topic of ['tickers', 'bbos', 'markprices', 'estfundingrate', 'PERP_BTC_USDC@orderbookupdate'])
    audit.sent(publicUrl, JSON.stringify({ event: 'subscribe', topic }));
  assert.equal(audit.ready(), false);
  audit.sent(privateUrl, JSON.stringify(auth()));
  assert.equal(audit.ready(), true, 'taped auth acknowledgement must not block playback readiness');
  assert(audit.errors().some((e) => e.includes('authenticated private')));
  audit.received(privateUrl, '{"event":"auth","success":true}');
  for (const topic of ['executionreport', 'algoexecutionreportv2', 'position'])
    audit.sent(privateUrl, JSON.stringify({ event: 'subscribe', topic }));
  audit.received(publicUrl, '{"event":"ping"}');
  assert(audit.errors().some((e) => e.includes('heartbeat')));
  audit.sent(publicUrl, '{"event":"pong"}');
  audit.received(privateUrl, '{"topic":"position","data":{"positions":[]}}');
  assert.deepEqual(audit.errors(), []);
});

test('Orderly audit rejects bad signatures, early subscriptions and unsubscribed data', () => {
  const audit = venue.createSocketAudit('ts', { isPrivate: true, books: [] });
  const bad = auth();
  bad.params.sign = 'invalid';
  audit.sent(privateUrl, JSON.stringify(bad));
  audit.sent(privateUrl, '{"event":"subscribe","topic":"executionreport"}');
  audit.received(privateUrl, '{"event":"auth","success":true}');
  audit.received(publicUrl, '{"topic":"bbos","data":[]}');
  const errors = audit.errors();
  for (const word of ['signature', 'preceded', 'unsubscribed', 'subscriptions'])
    assert(
      errors.some((e) => e.includes(word)),
      word,
    );
  const tape = new Tape([], venue, true);
  const socket = connect(tape, privateUrl);
  assert.throws(() => tape.send(socket, JSON.stringify(bad)), /signature/);
  assert.equal(tape.invalidSignatures, 1);
});

test('Orderly negative subscription acknowledgements invalidate sent-topic coverage and remain audit failures', () => {
  const audit = venue.createSocketAudit('rust', { isPrivate: false, books: ['BTCUSDC'] });
  for (const topic of ['tickers', 'bbos', 'markprices', 'PERP_BTC_USDC@orderbookupdate'])
    audit.sent(publicUrl, JSON.stringify({ event: 'subscribe', topic }));
  assert.equal(audit.ready(), true);
  audit.received(publicUrl, '{"event":"subscribe","topic":"PERP_BTC_USDC@orderbookupdate","success":false,"errorMsg":"rejected"}');
  audit.received(publicUrl, '{"topic":"tickers","data":[{"symbol":"PERP_BTC_USDC","close":100,"volume":1,"amount":100}]}');
  assert.equal(audit.ready(), false, 'sent subscriptions cannot prove rejected feed coverage');
  assert(audit.errors().some((error) => error.includes('rejected Orderly subscribe')));
  audit.closed(publicUrl, 1);
  audit.connected(publicUrl, 2);
  for (const topic of ['tickers', 'bbos', 'markprices', 'PERP_BTC_USDC@orderbookupdate'])
    audit.sent(publicUrl, JSON.stringify({ event: 'subscribe', topic }), 2);
  assert.equal(audit.ready(), true);
  assert(audit.errors().some((error) => error.includes('rejected Orderly subscribe')), 'recovery must not erase a rejection');
  audit.received(publicUrl, '{"event":"subscribe","success":false}', 2);
  assert.equal(audit.ready(), false, 'a rejection without a topic cannot prove any sent feed was accepted');
});
