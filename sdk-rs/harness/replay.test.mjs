import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { test } from 'node:test';
import { Tape, stable } from './tape.mjs';
import { subscriptionKeys } from './requests.mjs';
import { requestKey } from './requests.mjs';
import { venueOf } from './venues.mjs';
import { Responses } from './responses.mjs';
import { parseRequestJson } from './request-json.mjs';

test('production BloFin send accepts taped literal heartbeats and still rejects malformed messages', async () => {
  // Exercise the production override: SharedTape alone does not run this path.
  const source = readFileSync(new URL('./replay.mjs', import.meta.url), 'utf8');
  const start = source.indexOf('  send(socket, text) {');
  const end = source.indexOf('\n  verifySignature(', start);
  assert(start >= 0 && end > start);
  const venue = venueOf('blofin');
  const ProductionTape = new Function('Tape', 'values', 'venue', 'subscriptionKeys', 'canonicalObject',
    `return class extends Tape { ${source.slice(start, end)} }`)(Tape, { venue: 'blofin' }, venue, subscriptionKeys, stable);
  const tape = new ProductionTape([], venue);
  tape.subscriptions = [];
  const delivered = [];
  const socket = tape.connect('wss://openapi.blofin.com/ws/public', () => {}, (text) => delivered.push(text));
  tape.send(socket, 'ping');
  tape.send(socket, 'pong');
  await new Promise(setImmediate);
  // This venue uses recorded pong frames; do not invent an extra reply.
  assert.deepEqual(delivered, []);
  assert.deepEqual(tape.subscriptions, []);
  assert.equal(tape.sentRequests.size, 0);
  socket.onMessage('pong');
  assert.deepEqual(delivered, ['pong']);
  tape.send(socket, '{"op":"subscribe","args":[{"channel":"books","instId":"BTC-USDT"}]}');
  assert.equal(tape.subscriptions.length, 1);
  assert.equal(tape.sentRequests.size, 1);
  for (const text of ['{broken', 'unexpected', 'ping ']) assert.throws(() => tape.send(socket, text), SyntaxError);
});

test('production BingX matcher normalizes signing fields while retaining typed order fields and caller IDs', () => {
  const source = readFileSync(new URL('./replay.mjs', import.meta.url), 'utf8');
  const start = source.indexOf('const VOLATILE =');
  const end = source.indexOf('\nconst isWriteRequest =', start);
  assert(start >= 0 && end > start);
  const callerIds = new Set(['caller-lowercase']);
  const productionKey = new Function('values', 'venue', 'keyOf', 'callerIds', 'parseRequestJson',
    `${source.slice(start, end)}\nreturn requestKey;`)(
    { venue: 'bingx' }, venueOf('bingx'), requestKey, callerIds, parseRequestJson,
  );
  const url = 'https://open-api.bingx.com/openApi/swap/v2/trade/order';
  const order = { symbol: 'BTC-USDT', positionSide: 'BOTH', side: 'BUY', type: 'LIMIT',
    quantity: 0.0021, price: 90500, timeInForce: 'GTC', clientOrderId: 'caller-lowercase' };
  const responses = new Responses([{ type: 'http', method: 'POST', url,
    requestBody: JSON.stringify(order), status: 200, body: '{}' }], productionKey);
  const signed = { ...order, timestamp: 1792454400000, signature: 'actual-signed-body' };
  for (const changed of [{ ...signed, clientOrderId: 'other' }, { ...signed, quantity: '0.0021' },
    { ...signed, price: 90501 }, { ...signed, reduceOnly: true }]) {
    assert.equal(responses.respond('POST', url, JSON.stringify(changed)).status, 599);
    assert.equal(responses.unusedWrites(), 1);
  }
  assert.equal(responses.respond('POST', url, JSON.stringify(signed)).status, 200);
  assert.equal(responses.unusedWrites(), 0);
  assert.equal(responses.respond('POST', url, JSON.stringify(signed)).status, 599);
});

test('production Bitrue send and receive share subscription audit identity', () => {
  const source = readFileSync(new URL('./replay.mjs', import.meta.url), 'utf8');
  const start = source.indexOf('  send(socket, text) {');
  const end = source.indexOf('\n  verifySignature(', start);
  assert(start >= 0 && end > start);
  const venue = venueOf('bitrue');
  const ProductionTape = new Function('Tape', 'values', 'venue', 'subscriptionKeys', 'canonicalObject',
    `return class extends Tape { ${source.slice(start, end)} }`)(Tape, { venue: 'bitrue' }, venue, subscriptionKeys, stable);
  const tape = new ProductionTape([], venue);
  const channel = 'market_e_btcusdt_ticker';
  const rawUrl = 'wss://fapi.bitrue.com/kline-api/ws';
  const socket = tape.connect(rawUrl, () => {}, () => {});
  assert.notEqual(socket.url, socket.rawUrl, 'production canonical URLs include an empty query delimiter');
  const subscribe = JSON.stringify({ event: 'sub', params: { channel, cb_id: 'minted-callback' } });
  tape.send(socket, subscribe);
  assert.deepEqual([...tape.fullSubscriptions], subscriptionKeys(rawUrl, subscribe, new Set(), { ...venue, fullSocketIdentity: true }));
  socket.onMessage(JSON.stringify({ channel, tick: { close: 59000 } }));
  assert.deepEqual(tape.audit.errors(), []);
  tape.send(socket, JSON.stringify({ event: 'unsub', params: { channel, cb_id: 'other-callback' } }));
  socket.onMessage(JSON.stringify({ channel, tick: { close: 59001 } }));
  assert.deepEqual(tape.audit.errors(), ['data arrived without subscription: ' + channel]);
});

test('production Bitrue audit isolates replacement and parallel socket subscriptions', () => {
  const source = readFileSync(new URL('./replay.mjs', import.meta.url), 'utf8');
  const start = source.indexOf('  send(socket, text) {');
  const end = source.indexOf('\n  verifySignature(', start);
  const venue = venueOf('bitrue');
  const ProductionTape = new Function('Tape', 'values', 'venue', 'subscriptionKeys', 'canonicalObject',
    `return class extends Tape { ${source.slice(start, end)} }`)(Tape, { venue: 'bitrue' }, venue, subscriptionKeys, stable);
  const rawUrl = 'wss://fapi.bitrue.com/kline-api/ws';
  const channel = 'market_e_btcusdt_ticker';
  const otherChannel = 'market_e_ethusdt_ticker';
  const send = (tape, socket, event, channel) => tape.send(socket, JSON.stringify({ event, params: { channel } }));
  const data = (socket, channel) => socket.onMessage(JSON.stringify({ channel, tick: { close: 59000 } }));
  for (const parallel of [false, true]) {
    const tape = new ProductionTape([], venue);
    const first = tape.connect(rawUrl, () => {}, () => {});
    send(tape, first, 'sub', channel);
    if (!parallel) tape.close(first);
    const second = tape.connect(rawUrl, () => {}, () => {});
    if (parallel) send(tape, second, 'sub', otherChannel);
    data(second, channel);
    assert.deepEqual(tape.audit.errors(), ['data arrived without subscription: ' + channel],
      parallel ? 'parallel sockets cannot inherit sibling subscriptions' : 'a replacement must resubscribe');
    send(tape, second, 'sub', channel);
    data(second, channel);
    if (parallel) {
      data(first, channel);
      send(tape, second, 'unsub', channel);
      data(first, channel);
      data(second, otherChannel);
    }
    assert.equal(tape.audit.errors().length, 1, 'local subscription changes leave the other socket intact');
  }
  const depthTape = new ProductionTape([], venue, false, 30000, undefined, ['E-BTC-USDT', 'E-ETH-USDT']);
  for (const symbol of ['btcusdt', 'ethusdt']) {
    const socket = depthTape.connect('wss://fmarket-ws.bitrue.com/kline-api/ws', () => {}, () => {});
    send(depthTape, socket, 'sub', `market_e_${symbol}_simple_depth_step0`);
    depthTape.close(socket, 1000, true);
  }
  assert.deepEqual(depthTape.audit.errors(), [], 'split subscriptions remain visible after final local teardown');
});

test('production WOO audit keeps parallel subscriptions and heartbeat replies on their socket', () => {
  const source = readFileSync(new URL('./replay.mjs', import.meta.url), 'utf8');
  const start = source.indexOf('  send(socket, text) {');
  const end = source.indexOf('\n  verifySignature(', start);
  const venue = venueOf('woo');
  const ProductionTape = new Function('Tape', 'values', 'venue', 'subscriptionKeys', 'canonicalObject',
    `return class extends Tape { ${source.slice(start, end)} }`)(Tape, { venue: 'woo' }, venue, subscriptionKeys, stable);
  const tape = new ProductionTape([], venue);
  const url = 'wss://wss.woox.io/ws/stream/undefined';
  const first = tape.connect(url, () => {}, () => {});
  tape.send(first, '{"event":"ping"}');
  const subscribe = '{"event":"subscribe","topic":"tickers"}';
  tape.send(first, subscribe);
  const second = tape.connect(url, () => {}, () => {});
  tape.send(second, '{"event":"ping"}');
  first.onMessage('{"topic":"tickers","data":[]}');
  assert.deepEqual(tape.audit.errors(), [], 'opening a sibling must not reset the first socket');
  second.onMessage('{"topic":"tickers","data":[]}');
  assert.deepEqual(tape.audit.errors(), ['WOO frame arrived without subscription tickers']);
  tape.send(second, subscribe);
  const canonical = subscriptionKeys(url, subscribe, new Set(), { ...venue, fullSocketIdentity: true })[0];
  assert(tape.fullSubscriptions.has(`1 ${canonical}`));
  assert(tape.fullSubscriptions.has(`2 ${canonical}`));
  first.onMessage('{"event":"ping"}');
  tape.send(second, '{"event":"pong"}');
  assert(tape.audit.errors().some((error) => error.includes('server ping was not answered')));
});


test('production Orderly send correlates a taped snapshot before its sole consumer delivery', () => {
  const source = readFileSync(new URL('./replay.mjs', import.meta.url), 'utf8');
  const start = source.indexOf('  send(socket, text) {');
  const end = source.indexOf('\n  verifySignature(', start);
  const venue = venueOf('orderly');
  const ProductionTape = new Function('Tape', 'values', 'venue', 'subscriptionKeys', 'canonicalObject',
    `return class extends Tape { ${source.slice(start, end)} }`)(Tape, { venue: 'orderly' }, venue, subscriptionKeys, stable);
  const url = 'wss://ws-evm.orderly.org/ws/stream/public';
  const params = { type: 'orderbook', symbol: 'PERP_BTC_USDC' };
  const recorded = { type: 'ws-send', url, socket: 0, text: JSON.stringify({ event: 'request', id: 'taped-id', params }) };
  const tape = new ProductionTape([recorded], venue);
  const received = [];
  const socket = tape.connect(url, () => {}, (text) => received.push(JSON.parse(text)));
  tape.send(socket, JSON.stringify({ event: 'request', id: 'runtime-id', params }));
  const payload = { event: 'request', id: 'taped-id', success: true, data: { symbol: params.symbol, bids: [[100, 2]], asks: [[101, 3]] } };
  socket.onMessage(JSON.stringify(payload));
  assert.deepEqual(received, [{ ...payload, id: 'runtime-id' }]);
  assert.equal(tape.sentRequests.size, 1);
  assert(tape.fullSubscriptions.size > 0, 'the production wrapper retains canonical outgoing identity');
  assert.throws(() => socket.onMessage(JSON.stringify(payload)), /duplicate/);
  const next = tape.connect(url, () => {}, () => {});
  assert.throws(() => tape.send(next, JSON.stringify({ event: 'request', id: 'replacement-id', params })), /unrecorded/);
});


test('production Hyro partial readiness waits for both public seed flags in public and private replays', async () => {
  const source = readFileSync(new URL('./replay.mjs', import.meta.url), 'utf8');
  const start = source.indexOf('    async (accountReady = true) => {', source.indexOf('async function runTs()'));
  const end = source.indexOf('\n    (record) => runTsCommand', start);
  assert(start >= 0 && end > start);
  for (const isPrivate of [false, true]) {
    const store = { loaded: { markets: true, tickers: false, balance: false, orders: false, positions: false } };
    const ready = new Function('prepared', 'subscribed', 'venue', 'exchange', 'bookSymbols', 'realTimeout',
      'replayBooksReady', 'finalBooks', 'store', 'isPrivate',
      `return (${source.slice(start, end).trim().replace(/,$/, '')});`)(
      Promise.resolve(), Promise.resolve(), { tsBooksSubscribed: () => true }, {}, [], setTimeout,
      () => true, {}, store, isPrivate,
    );
    let done = false;
    const pending = ready(false).then(() => { done = true; });
    await new Promise(setImmediate);
    assert.equal(done, false, 'market subscription readiness must not bypass an unfinished ticker seed');
    store.loaded.tickers = true;
    await pending;
    assert.equal(done, true, 'partial readiness does not require private account state');
  }
});

test('production constructor keeps typed signed HTTP matching separate from subscription generations', () => {
  const source = readFileSync(new URL('./replay.mjs', import.meta.url), 'utf8');
  const keyStart = source.indexOf('const VOLATILE =');
  const classStart = source.indexOf('class Tape extends SharedTape {');
  const classEnd = source.indexOf('\nconst settle =', classStart);
  assert(keyStart >= 0 && classStart > keyStart && classEnd > classStart);
  const url = 'https://open-api.bingx.com/openApi/swap/v2/trade/order';
  const order = { symbol: 'BTC-USDT', positionSide: 'BOTH', side: 'BUY', type: 'LIMIT',
    quantity: 0.0021, price: 90500, timeInForce: 'GTC', clientOrderId: 'caller-lowercase' };
  const records = [{ type: 'http', method: 'POST', url, requestBody: JSON.stringify(order), status: 200, body: '{}' }];
  const venue = venueOf('bingx');
  assert.equal(venue.subscriptionGenerations, true, 'generation tracking also belongs to unrelated protocols');
  const ProductionTape = new Function('SharedTape', 'values', 'venue', 'records', 'bookSymbols', 'callerIds',
    'keyOf', 'Responses', 'sharedUrl', 'parseRequestJson', 'isRead', 'isPrivate',
    `${source.slice(keyStart, classStart)}\nreturn (${source.slice(classStart, classEnd)});`)(
    Tape, { venue: 'bingx' }, venue, records, [], new Set(['caller-lowercase']), requestKey,
    Responses, undefined, parseRequestJson, () => false, true,
  );
  const tape = new ProductionTape('ts');
  const signed = { ...order, timestamp: 1792454400000, signature: 'verified-separately' };
  for (const changed of [{ ...signed, quantity: 0.0022 }, { ...signed, quantity: '0.0021' },
    { ...signed, clientOrderId: 'other' }, { ...signed, extra: true }]) {
    assert.equal(tape.respond('POST', url, JSON.stringify(changed)).status, 599);
    assert.equal(tape.missingWrites, 1);
  }
  assert.equal(tape.respond('POST', url, JSON.stringify(signed)).status, 200);
  assert.equal(tape.missingWrites, 0);
  assert.equal(tape.respond('POST', url, JSON.stringify(signed)).status, 599);
  const socketUrl = 'wss://ws-market-swap.qq-os.com/ws';
  const message = JSON.stringify({ id: 'runtime-id', reqType: 'sub', dataType: 'BTC-USDT@depth20' });
  const first = tape.connect(socketUrl, () => {}, () => {});
  tape.send(first, message);
  const expected = subscriptionKeys(socketUrl, message, new Set(), { ...venue, fullSocketIdentity: true });
  assert.deepEqual([...tape.fullSubscriptions], expected.map((key) => `1 ${key}`));
  tape.close(first, 1000, true);
  const replacement = tape.connect(socketUrl, () => {}, () => {});
  tape.send(replacement, message);
  assert.deepEqual([...tape.fullSubscriptions], [
    ...expected.map((key) => `1 ${key}`), ...expected.map((key) => `2 ${key}`),
  ]);
});
