import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { createHash, createHmac, timingSafeEqual } from 'node:crypto';
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

test('production KuCoin constructor preserves signed REST query fields independently of socket IDs', () => {
  const source = readFileSync(new URL('./replay.mjs', import.meta.url), 'utf8');
  const keyStart = source.indexOf('const VOLATILE =');
  const classStart = source.indexOf('class Tape extends SharedTape {');
  const classEnd = source.indexOf('\nconst settle =', classStart);
  const venue = venueOf('kucoin');
  const url = 'https://api-futures.kucoin.com/api/v1/orders';
  const body = '{"symbol":"XBTUSDTM","size":"2","clientOid":"generated-valid"}';
  const records = [{ type: 'http', method: 'POST', url, requestBody: body, status: 200, body: '{}' }];
  const ProductionTape = new Function('SharedTape', 'values', 'venue', 'records', 'bookSymbols', 'callerIds',
    'keyOf', 'Responses', 'sharedUrl', 'parseRequestJson', 'isRead', 'isPrivate',
    `${source.slice(keyStart, classStart)}\nreturn (${source.slice(classStart, classEnd)});`)(
    Tape, { venue: 'kucoin' }, venue, records, [], new Set(), requestKey,
    Responses, undefined, parseRequestJson, () => false, true,
  );
  const tape = new ProductionTape('ts');
  const signed = (route) => {
    const timestamp = String(Date.now());
    const hmac = (text) => createHmac('sha256', 'replay-secret').update(text).digest('base64');
    return { 'KC-API-KEY': 'replay-key', 'KC-API-KEY-VERSION': '2', 'KC-API-TIMESTAMP': timestamp,
      'KC-API-PASSPHRASE': hmac('replay-passphrase'), 'KC-API-SIGN': hmac(timestamp + 'POST' + route + body) };
  };
  const extra = '/api/v1/orders?connectId=unknown-economic-parameter';
  assert.equal(tape.respond('POST', new URL(extra, url).href, body, signed(extra)).status, 599);
  assert.equal(tape.missingWrites, 1);
  assert.equal(tape.respond('POST', url, body, signed('/api/v1/orders')).status, 200);
  assert.equal(tape.missingWrites, 0);
  assert.equal(tape.respond('POST', url, body, signed('/api/v1/orders')).status, 599);
});

test('production KuCoin startup requires every requested REST book and accepts initialized empty snapshots', async () => {
  const source = readFileSync(new URL('./replay.mjs', import.meta.url), 'utf8');
  const start = source.indexOf('    async (accountReady = true) => {', source.indexOf('async function runTs()'));
  const end = source.indexOf('\n    (record) => runTsCommand', start);
  const venue = venueOf('kucoin');
  assert.equal(venue.initialBookSnapshots, true);
  const books = { BTCUSDT: { bids: [], asks: [] }, WRONG: { bids: [], asks: [] } };
  const exchange = { publicWebsocket: { messageHandlers: { orderBook_BTCUSDT: [], orderBook_ETHUSDT: [] } } };
  const ready = new Function('prepared', 'subscribed', 'venue', 'exchange', 'bookSymbols', 'realTimeout',
    'replayBooksReady', 'finalBooks', 'store', 'isPrivate',
    `return (${source.slice(start, end).trim().replace(/,$/, '')});`)(
    Promise.resolve(), Promise.resolve(), venue, exchange, ['BTCUSDT', 'ETHUSDT'], setTimeout,
    (required, available) => required.every((symbol) => available.includes(symbol)), books, { loaded: {} }, false,
  );
  let done = false;
  const pending = ready(true).then(() => { done = true; });
  await new Promise(setImmediate);
  assert.equal(done, false, 'a queued second snapshot must finish before public playback');
  books.ETHUSDT = { bids: [], asks: [] };
  await pending;
  assert.equal(done, true);
});

test('production Aster constructor binds configured V1/V3 credentials before matching and consuming writes', () => {
  const source = readFileSync(new URL('./replay.mjs', import.meta.url), 'utf8');
  const keyStart = source.indexOf('const VOLATILE =');
  const classStart = source.indexOf('class Tape extends SharedTape {');
  const classEnd = source.indexOf('\nconst settle =', classStart);
  const vectors = JSON.parse(readFileSync(new URL('../venues/asterdex/tests/fixtures/signing.json', import.meta.url))).cases;
  const vector = vectors.find((row) => row.name === 'documented-order-mainnet');
  const now = Date.now;
  Date.now = () => vector.nowMs;
  try {
    for (const v3 of [false, true]) {
      const venue = { ...venueOf('asterdex'), replayOptions: v3
        ? { key: vector.signer, secret: '0x4fd0a42218f3eae43a6ce26d22544e986139a01e5b34a62db53757ffca81bae1', applicationId: vector.user }
        : { key: 'configured-key', secret: 'configured-secret' } };
      const base = 'https://fapi.asterdex.com/fapi/' + (v3 ? 'v3' : 'v1') + '/order';
      const fields = 'symbol=BTCUSDT&quantity=0.003&recvWindow=20000&timestamp=' + vector.nowMs;
      const url = v3 ? base + '?' + vector.codeQuery + '&signature=' + vector.codeSignature
        : base + '?' + fields + '&signature=' + createHmac('sha256', 'configured-secret').update(fields).digest('hex');
      const records = [{ type: 'http', method: 'POST', url, status: 200, body: '{}' }];
      const ProductionTape = new Function('SharedTape', 'values', 'venue', 'records', 'bookSymbols', 'callerIds',
        'keyOf', 'Responses', 'sharedUrl', 'parseRequestJson', 'isRead', 'isPrivate',
        `${source.slice(keyStart, classStart)}\nreturn (${source.slice(classStart, classEnd)});`)(
        Tape, { venue: 'asterdex' }, venue, records, [], new Set(), requestKey,
        Responses, undefined, parseRequestJson, () => false, true,
      );
      const tape = new ProductionTape('ts');
      assert(tape.responseStore, 'production Aster uses the shared typed store');
      assert.throws(() => tape.respond('POST', url, undefined, { 'X-MBX-APIKEY': 'wrong-key' }), /wrong API wallet/);
      assert.equal(tape.missingWrites, 1, 'invalid auth must not consume a response');
      if (!v3) for (const [field, value] of [['newClientOrderId', ''], ['newClientOrderId', 'bad\n'],
        ['clientAlgoId', 'x'.repeat(99)], ['quantity', '0.004'], ['unknown', 'economic']]) {
        const changed = new URL(url);
        changed.searchParams.delete('signature');
        changed.searchParams.set(field, value);
        const payload = [...changed.searchParams].map(([key, value]) => `${key}=${value}`).join('&');
        changed.searchParams.set('signature', createHmac('sha256', 'configured-secret').update(payload).digest('hex'));
        assert.equal(tape.respond('POST', changed.href, undefined, { 'X-MBX-APIKEY': 'configured-key' }).status, 599);
        assert.equal(tape.missingWrites, 1, `${field} must not alias the expected order`);
      }

      assert.equal(tape.respond('POST', url, undefined, { 'X-MBX-APIKEY': venue.replayOptions.key }).status, 200);
      assert.equal(tape.missingWrites, 0);
      if (v3) assert.throws(() => tape.respond('POST', url, undefined, { 'X-MBX-APIKEY': venue.replayOptions.key }), /nonce was reused/);
      else assert.equal(tape.respond('POST', url, undefined, { 'X-MBX-APIKEY': venue.replayOptions.key }).status, 599);
    }
  } finally { Date.now = now; }
});

const productionTapeFor = (name, records, venue = venueOf(name)) => {
  const source = readFileSync(new URL('./replay.mjs', import.meta.url), 'utf8');
  const start = source.indexOf('const VOLATILE =');
  const classStart = source.indexOf('class Tape extends SharedTape {');
  const end = source.indexOf('\nconst settle =', classStart);
  const ProductionTape = new Function('SharedTape', 'values', 'venue', 'records', 'bookSymbols', 'callerIds',
    'keyOf', 'Responses', 'sharedUrl', 'parseRequestJson', 'isRead', 'isPrivate',
    'subscriptionKeys', 'createHash', 'createHmac', 'timingSafeEqual', 'KEY', 'PASSPHRASE', 'SECRET',
    `${source.slice(start, classStart)}\nreturn (${source.slice(classStart, end)});`)(
    Tape, { venue: name }, venue, records, [], new Set(), requestKey, Responses, undefined,
    parseRequestJson, (method) => method === 'GET', true,
    subscriptionKeys, createHash, createHmac, timingSafeEqual, 'replay-key', 'replay-passphrase', 'replay-secret',
  );
  return new ProductionTape('ts');
};

for (const name of ['okx', 'okxdemo', 'okxeu', 'okxeudemo']) {
  const host = name.startsWith('okxeu') ? 'eea.okx.com' : name.endsWith('demo') ? 'openapi.okx.com' : 'www.okx.com';
  const headers = (method, url, body = '') => {
    const timestamp = new Date().toISOString(), parsed = new URL(url);
    return { 'OK-ACCESS-KEY': 'replay-key', 'OK-ACCESS-PASSPHRASE': 'replay-passphrase',
      'OK-ACCESS-TIMESTAMP': timestamp,
      'OK-ACCESS-SIGN': createHmac('sha256', 'replay-secret').update(timestamp + method + parsed.pathname + parsed.search + body).digest('base64'),
      ...(name.endsWith('demo') ? { 'x-simulated-trading': '1' } : {}) };
  };
  test(`production ${name} constructor authenticates raw bytes before typed caller, economic and unknown-field matching`, () => {
    const url = `https://${host}/api/v5/trade/order`, caller = 'e847386590ce4dBCcallerChosen';
    const body = { instId: 'BTC-USD-UM', tdMode: 'cross', side: 'buy', ordType: 'limit', sz: '1', px: '90000', clOrdId: caller };
    const raw = JSON.stringify(body), records = [{ type: 'command', op: 'placeOrders', orders: [{ clientOrderId: caller }] },
      { type: 'http', method: 'POST', url, requestBody: raw, status: 200, body: '{}' }];
    const tape = productionTapeFor(name, records);
    assert(tape.responseStore, 'the actual constructor must use SharedTape typed responses');
    for (const invalidUrl of [url.replace('https://', 'https://user:pass@'), `${url}#unknown`, `${url}#`, url.replace(host, `${host}:8443`)]) {
      assert.throws(() => tape.respond('POST', invalidUrl, raw, headers('POST', invalidUrl, raw)), /HTTP (host|origin)/);
      assert.equal(tape.missingWrites, 1, 'raw URL rejection precedes response matching');
    }
    assert.throws(() => tape.respond('POST', url, raw, { ...headers('POST', url, raw), 'OK-ACCESS-SIGN': 'bad' }), /signature/);
    assert.throws(() => tape.respond('POST', url, raw, [...Object.entries(headers('POST', url, raw)), ['ok-access-key', 'replay-key']]), /Duplicate/);
    for (const changed of [{ ...body, sz: 1 }, { ...body, px: '90001' }, { ...body, clOrdId: 'e847386590ce4dBCotherChosen' },
      ...['timestamp', 'recvWindow', 'signature', 'unknownNativeField'].map((field) => ({ ...body, [field]: 'extra' }))]) {
      const value = JSON.stringify(changed);
      assert.equal(tape.respond('POST', url, value, headers('POST', url, value)).status, 599);
      assert.equal(tape.missingWrites, 1, 'rejected requests cannot consume the recorded write');
    }
    assert.equal(tape.respond('POST', url, raw, headers('POST', url, raw)).status, 200);
    assert.equal(tape.missingWrites, 0);
    assert.equal(tape.respond('POST', url, raw, headers('POST', url, raw)).status, 599);

    const generated = { ...body, clOrdId: 'e847386590ce4dBCgeneratedA' };
    const generatedTape = productionTapeFor(name, [{ type: 'http', method: 'POST', url, requestBody: generated, status: 200, body: '{}' }]);
    for (const id of ['e847386590ce4dBCbad\n', 'e847386590ce4dBC' + 'a'.repeat(17)]) {
      const value = JSON.stringify({ ...generated, clOrdId: id });
      assert.equal(generatedTape.respond('POST', url, value, headers('POST', url, value)).status, 599);
    }
    const fresh = JSON.stringify({ ...generated, clOrdId: 'e847386590ce4dBCgeneratedB' });
    assert.equal(generatedTape.respond('POST', url, fresh, headers('POST', url, fresh)).status, 200);

    for (const [method, path, field] of [['POST', 'cancel-order', 'clOrdId'], ['POST', 'amend-order', 'clOrdId'], ['GET', 'order', 'clOrdId'], ['POST', 'cancel-algos', 'algoClOrdId']]) {
      const endpoint = `https://${host}/api/v5/trade/${path}`, target = { instId: body.instId, [field]: generated.clOrdId, ordId: '5' };
      const expectedUrl = method === 'GET' ? `${endpoint}?${new URLSearchParams(target)}` : endpoint;
      const expectedBody = method === 'GET' ? undefined : JSON.stringify(target);
      const targetTape = productionTapeFor(name, [{ type: 'http', method, url: expectedUrl, requestBody: expectedBody, status: 200, body: '{}' }]);
      for (const change of [{ [field]: 'e847386590ce4dBCgeneratedB' }, { ordId: '6' },
        ...(method === 'GET' ? ['timestamp', 'recvWindow', 'signature'].map((key) => ({ [key]: 'extra' })) : [])]) {
        const changed = { ...target, ...change }, changedUrl = method === 'GET' ? `${endpoint}?${new URLSearchParams(changed)}` : endpoint;
        const changedBody = method === 'GET' ? '' : JSON.stringify(changed);
        assert.equal(targetTape.respond(method, changedUrl, changedBody, headers(method, changedUrl, changedBody)).status, 599, `${path} target identity stays exact`);
      }
      assert.equal(targetTape.respond(method, expectedUrl, expectedBody, headers(method, expectedUrl, expectedBody)).status, 200);
    }
    for (const path of ['/api/v5/public/instruments?instType=SWAP', '/api/v5/market/tickers?instType=SWAP']) {
      const publicUrl = `https://${host}${path}`, unsignedHeaders = name.endsWith('demo') ? { 'x-simulated-trading': '1' } : {};
      const publicTape = productionTapeFor(name, ['first', 'second'].map((body) => ({ type: 'http', method: 'GET', url: publicUrl, status: 200, body })));
      for (const contamination of [{ 'OK-ACCESS-KEY': 'leaked-key' }, { 'OK-ACCESS-KEY': 'replay-key' },
        { 'OK-ACCESS-PASSPHRASE': 'replay-passphrase' }, { 'OK-ACCESS-SIGN': '' },
        { Authorization: 'Bearer leaked-token' }, { 'OK-ACCESS-UNKNOWN': 'extra' }]) {
        assert.throws(() => publicTape.respond('GET', publicUrl, undefined, { ...unsignedHeaders, ...contamination }), /credentials|timestamp|authentication/);
        assert.equal(publicTape.requests.size, 0, 'credential rejection never reaches the response matcher');
        assert.equal([...publicTape.responseStore.queues.values()][0].length, 2);
      }
      assert.equal(publicTape.respond('GET', publicUrl, undefined, unsignedHeaders).body, 'first');
      assert.equal(publicTape.respond('GET', publicUrl, undefined, headers('GET', publicUrl)).body, 'second', 'honest private clients may sign public market reads');
    }
  });

  test(`production ${name} socket path records full subscriptions and isolates product intents per actual socket`, async () => {
    const tape = productionTapeFor(name, []), regional = name.startsWith('okxeu') ? name.endsWith('demo') ? 'wseeapap' : 'wseea' : name.endsWith('demo') ? 'wspap' : 'ws';
    const url = `wss://${regional}.okx.com:8443/ws/v5/business`, socket = tape.connect(url, () => {}, () => {});
    const timestamp = String(Math.floor(Date.now() / 1000));
    const arg = { apiKey: 'replay-key', passphrase: 'replay-passphrase', timestamp,
      sign: createHmac('sha256', 'replay-secret').update(`${timestamp}GET/users/self/verify`).digest('base64') };
    assert.throws(() => tape.send(socket, JSON.stringify({ op: 'login', args: [{ ...arg, sign: 'bad' }] })), /signature/);
    const login = JSON.stringify({ op: 'login', args: [arg] });
    tape.send(socket, login);
    assert.equal(socket.loginVerified, true);
    const subscribe = JSON.stringify({ op: 'subscribe', args: [{ channel: 'orders-algo', instType: 'FUTURES' }] });
    tape.send(socket, subscribe);
    assert.deepEqual([...tape.fullSubscriptions], [login, subscribe].flatMap((text) =>
      subscriptionKeys(url, text, new Set(), { ...venueOf(name), fullSocketIdentity: true })));
    const message = { arg: { channel: 'orders-algo', instType: 'FUTURES' }, data: [{ instType: 'FUTURES', instId: 'BTC-USD-UM' }] };
    assert.doesNotThrow(() => tape.assertSubscribed(socket, message));
    assert.throws(() => tape.assertSubscribed(socket, { ...message, data: [{ instType: 'SWAP', instId: 'BTC-USDT-SWAP' }] }), /Unsubscribed/);
    const second = tape.connect(url, () => {}, () => {});
    assert.throws(() => tape.assertSubscribed(second, message), /Unsubscribed/);
    tape.send(socket, JSON.stringify({ op: 'unsubscribe', args: [{ channel: 'orders-algo', instType: 'FUTURES' }] }));
    assert.throws(() => tape.assertSubscribed(socket, message), /Unsubscribed/);
    await new Promise(setImmediate);
    tape.close(socket); tape.close(second);
  });
}

test('production Kraken constructor validates raw form HMAC before typed generated create matching', async () => {
  const venue = venueOf('kraken');
  venue.initializeReplay('ts');
  const url = 'https://futures.kraken.com/derivatives/api/v3/sendorder';
  const records = [{ type:'http', method:'POST', url, requestBody:'cliOrdId=ts_recorded&size=1&symbol=PF_XBTUSD', status:200, body:'{}' }];
  const tape = productionTapeFor('kraken', records);
  assert(tape.responseStore);
  const {krakenSignature}=await import('./kraken-protocol.mjs');
  let nonce=BigInt(Date.now())*1000n;
  const send=(size,id,auth=true)=>{
    const body=new URLSearchParams({cliOrdId:id,size,symbol:'PF_XBTUSD'}).toString();
    const n=String(++nonce);
    return tape.respond('POST',url,body,{APIKey:'replay-key',Nonce:n,Authent:auth?krakenSignature(body+n+'/api/v3/sendorder'):'bad'});
  };
  assert.throws(()=>send('1','ts_bad',false),/signature/);
  assert.equal(tape.missingWrites,1);
  assert.equal(send('2','ts_changed').status,599);assert.equal(tape.missingWrites,1);
  assert.throws(()=>send('1','ts_invalid\n'),/invalid/);assert.equal(tape.missingWrites,1);
  assert.equal(send('1','ts_valid').status,200);assert.equal(tape.missingWrites,0);
});

test('production ApeX constructor authenticates exact form bytes and binds typed caller/economic fields', async () => {
  const {prepareApexReplay,apexSignatureForTest,APEX_ACCOUNT}=await import('./apexomni-protocol.mjs');
  await prepareApexReplay({isPrivate:true});
  const url='https://omni.apex.exchange/api/v3/order',now=Date.now();
  const id=`apexomni-${APEX_ACCOUNT}-${now}-123456`;
  const fields={brokerId:'6587',clientId:id,clientOrderId:id,expiration:String((Math.ceil((now+30*86400000)/3600000)+168)*3600000),
    limitFee:'0.030000',price:'60000',side:'BUY',size:'0.001',symbol:'BTC-USDT',timeInForce:'GOOD_TIL_CANCEL',type:'LIMIT',
    signature:apexSignatureForTest(id,'0.001','60000',true)};
  const expected={...fields,clientId:'recorded-parent',clientOrderId:'recorded-parent',expiration:'recorded-expiration',signature:'recorded-signature'};
  const tape=productionTapeFor('apexomni',[{type:'http',method:'POST',url,requestBody:expected,status:200,body:'{}'}]);
  assert(tape.responseStore);
  const send=(values,auth=true)=>{
    const body=new URLSearchParams(Object.entries(values).sort(([a],[b])=>a.localeCompare(b))).toString();
    const signature=createHmac('sha256',Buffer.from('replay-secret').toString('base64')).update(`${now}POST/api/v3/order${body}`).digest('base64');
    return tape.respond('POST',url,body,{'APEX-API-KEY':'replay-key','APEX-PASSPHRASE':'replay-passphrase','APEX-TIMESTAMP':String(now),
      'APEX-SIGNATURE':auth?signature:'bad','Content-Type':'application/x-www-form-urlencoded'});
  };
  assert.throws(()=>send(fields,false),/HMAC/);assert.equal(tape.missingWrites,1);
  assert.equal(send({...fields,unknownEconomicField:'extra'}).status,599);assert.equal(tape.missingWrites,1);
  assert.throws(()=>send({...fields,clientId:id+'\n',clientOrderId:id+'\n'}),/client order ID/);assert.equal(tape.missingWrites,1);
  assert.equal(send(fields).status,200);assert.equal(tape.missingWrites,0);
});

test('production ApeX socket audit keeps parallel and replacement auth, topics and pings isolated', () => {
  const tape=productionTapeFor('apexomni',[]);
  const url='wss://quote.omni.apex.exchange/realtime_private?v=2';
  const connect=()=>tape.connect(url,()=>{},()=>{});
  const login=(socket)=>{
    const now=Date.now();
    const signature=createHmac('sha256',Buffer.from('replay-secret').toString('base64')).update(`${now}GET/ws/accounts`).digest('base64');
    tape.send(socket,JSON.stringify({op:'login',args:[JSON.stringify({type:'login',httpMethod:'GET',requestPath:'/ws/accounts',apiKey:'replay-key',passphrase:'replay-passphrase',timestamp:now,signature})]}));
    tape.send(socket,'{"op":"subscribe","args":["ws_zk_accounts_v3","ws_user_v1","ws_notify_v1"]}');
    tape.send(socket,'{"op":"ping"}');
  };
  const first=connect();login(first);
  const sibling=connect();
  sibling.onMessage('{"topic":"ws_user_v1","data":{}}');
  assert(tape.audit.errors().some(error=>error.includes('without subscription')));
  assert(tape.audit.errors().some(error=>error.includes('preceded login')));
  login(sibling);
  first.onMessage('{"op":"ping","args":["only-first"]}');
  tape.send(sibling,'{"op":"pong","args":["only-first"]}');
  assert(tape.audit.errors().some(error=>error.includes('unexpected server ping response')));
  tape.send(first,'{"op":"pong","args":["only-first"]}');
  tape.send(first,'{"op":"unsubscribe","args":["ws_user_v1"]}');
  sibling.onMessage('{"topic":"ws_user_v1","data":{}}');
  tape.close(first,1000,true);tape.close(sibling,1000,true);
  const replacement=connect();
  replacement.onMessage('{"topic":"ws_user_v1","data":{}}');
  assert.equal(tape.audit.ready(),false,'closed authenticated sockets cannot make replacement ready');
  tape.send(replacement,'{"op":"ping"}');
  assert(tape.audit.errors().some(error=>error.includes('heartbeat preceded login')));
  assert([...tape.fullSubscriptions].some(key=>key.includes('"op":"login"')),'actual wrapper delegates the shared send path');
});

test('both production exchange constructors receive the complete initialized replay options', () => {
  const source=readFileSync(new URL('./replay.mjs',import.meta.url),'utf8');
  const options={ key:'key', secret:'secret', passphrase:'pass', omniKeySeed:'documented-seed', applicationId:'owner', asterdex:{apiVersion:'v3'} };
  for(const functionName of ['runRust','runTs']) {
    const start=source.indexOf(`async function ${functionName}()`);
    const declaration=source.slice(start).match(/  const options = [^\n]+;/)?.[0];
    assert(declaration,`${functionName} production options declaration exists`);
    const actual=new Function('venue',`${declaration}\nreturn options;`)({replayOptions:options});
    assert.deepEqual(actual,options,`${functionName} must preserve per-venue credentials beyond the common three fields`);
    assert.notEqual(actual,options);
  }
});

test('production ApeX rejects authenticated JSON under a form header without consuming its form response', () => {
  const url='https://omni.apex.exchange/api/v3/delete-order';
  const tape=productionTapeFor('apexomni',[{type:'http',method:'POST',url,requestBody:{id:'order'},status:200,body:'{}'}]);
  const send=(body,contentType='application/x-www-form-urlencoded')=>{
    const timestamp=String(Date.now());
    const signature=createHmac('sha256',Buffer.from('replay-secret').toString('base64')).update(`${timestamp}POST/api/v3/delete-order${body}`).digest('base64');
    return tape.respond('POST',url,body,{'APEX-API-KEY':'replay-key','APEX-PASSPHRASE':'replay-passphrase','APEX-TIMESTAMP':timestamp,
      'APEX-SIGNATURE':signature,'Content-Type':contentType});
  };
  for(const body of ['{"id":"order"}',' ["id=order"]','id','']) {
    assert.throws(()=>send(body),/encoded form bytes/);
    assert.equal(tape.missingWrites,1,'invalid actual encoding cannot consume a fixture acknowledgement');
  }
  for(const header of ['application/json','application/x-www-form-urlencoded-invalid'])
    assert.throws(()=>send('id=order',header),/not form encoded/);
  assert.equal(tape.missingWrites,1);
  assert.equal(send('id=order').status,200);
  assert.equal(tape.missingWrites,0);
});

test('production Kraken batch matching retains signed outer economic fields around JSON', async () => {
  const venue=venueOf('kraken');venue.initializeReplay('ts');
  const url='https://futures.kraken.com/derivatives/api/v3/batchorder';
  const json=JSON.stringify({batchOrder:[{order:'cancel',order_id:'001'}]});
  const body=new URLSearchParams({json}).toString();
  const tape=productionTapeFor('kraken',[{type:'http',method:'POST',url,requestBody:body,status:200,body:'{}'}]);
  const {krakenSignature}=await import('./kraken-protocol.mjs');
  let nonce=BigInt(Date.now())*1000n;
  const send=(actual)=>{
    const n=String(++nonce);
    return tape.respond('POST',url,actual,{APIKey:'replay-key',Nonce:n,Authent:krakenSignature(actual+n+'/api/v3/batchorder')});
  };
  for(const actual of [body+'&unknownEconomicField=extra',body+'&size=100',body+'&order_id=other']) {
    assert.equal(send(actual).status,599);
    assert.equal(tape.missingWrites,1,'authenticated outer fields cannot disappear while matching the batch');
  }
  assert.equal(send(body).status,200);assert.equal(tape.missingWrites,0);
  const options={requestBody:(body,context)=>venue.requestBody(body,context)};
  const large=(value)=>new URLSearchParams({json:`{"batchOrder":[{"order":"cancel","order_id":${value}}]}`}).toString();
  assert.notEqual(requestKey('POST',url,large('9007199254740992'),new Set(),options),
    requestKey('POST',url,large('9007199254740993'),new Set(),options),'JSON envelope decoding must retain distinct large numeric values');
});

test('production Paradex authenticates exact order economics before route-scoped typed generated/caller identity matching', () => {
  const vectors=JSON.parse(readFileSync(new URL('../venues/paradex/tests/sign-vectors.json',import.meta.url)));
  const c=vectors.cases.find(row=>row.environment==='prod'),order=c.orders.find(row=>!row.id);
  const now=Date.now;Date.now=()=>c.now;
  try {
    const url='https://api.prod.paradex.trade/v1/orders/batch';
    const row={market:order.market,side:order.side,type:order.type,size:order.size,...(order.price===undefined?{}:{price:order.price}),
      signature_timestamp:c.now,signature:order.signature,client_id:'ts_runtime'};
    const expected={...row,client_id:'generated'};
    const tape=productionTapeFor('paradex',[{type:'http',method:'POST',url,requestBody:JSON.stringify([expected]),status:200,body:'{"orders":[{"id":"1","client_id":"$request:0.client_id"}]}'}]);
    assert(tape.responseStore,'actual production constructor uses the shared typed store');
    const send=(value,authorization='Bearer replay-paradex-jwt')=>tape.respond('POST',url,JSON.stringify([value]),{Authorization:authorization,'PARADEX-PARTNER':'TEALSTREET'});
    assert.throws(()=>send(row,'wrong'),/invalid replay signature/);assert.equal(tape.missingWrites,1);
    for(const client_id of [123,null,true,'','ts_bad\n','ts_'+'a'.repeat(22),'caller-undeclared',{},[]]) {
      assert.throws(()=>send({...row,client_id}),/invalid replay signature/);assert.equal(tape.missingWrites,1);
    }
    assert.throws(()=>send({...row,size:'987654321.12345678'}),/invalid replay signature/);assert.equal(tape.missingWrites,1);
    assert.equal(send({...row,unknownEconomic:'extra'}).status,599);assert.equal(tape.missingWrites,1);
    const response=send(row);assert.equal(response.status,200);assert.deepEqual(JSON.parse(response.body),{orders:[{id:'1',client_id:'ts_runtime'}]});
    assert.equal(tape.missingWrites,0);assert.equal(send(row).status,599);
    const caller={...row,client_id:'caller-exact'};
    const callerTape=productionTapeFor('paradex',[{type:'command',t:0,op:'placeOrders',orders:[{clientOrderId:'caller-exact'}]},
      {type:'http',method:'POST',url,requestBody:JSON.stringify([caller]),status:200,body:'{}'}]);
    const headers={Authorization:'Bearer replay-paradex-jwt','PARADEX-PARTNER':'TEALSTREET'};
    assert.equal(callerTape.respond('POST',url,JSON.stringify([row]),headers).status,599);assert.equal(callerTape.missingWrites,1);
    assert.equal(callerTape.respond('POST',url,JSON.stringify([caller]),headers).status,200);
  } finally {Date.now=now;}
});

test('production Paradex preserves business signature fields outside independently signed order rows', () => {
  const url='https://api.prod.paradex.trade/v1/account/margin/BTC-USD-PERP';
  const tape=productionTapeFor('paradex',[{type:'http',method:'POST',url,requestBody:'{"leverage":10,"margin_type":"CROSS"}',status:200,body:'{}'}]);
  const headers={Authorization:'Bearer replay-paradex-jwt'};
  for(const field of ['signature','signature_timestamp']) {
    assert.equal(tape.respond('POST',url,JSON.stringify({leverage:10,margin_type:'CROSS',[field]:'unknown-business-field'}),headers).status,599);
    assert.equal(tape.missingWrites,1);
  }
  assert.equal(tape.respond('POST',url,'{"leverage":10,"margin_type":"CROSS"}',headers).status,200);
});

test('production Paradex socket state isolates parallel connections and closed generations', async () => {
  const tape=productionTapeFor('paradex',[]);
  const url='wss://ws.api.prod.paradex.trade/v1?cancel-on-disconnect=false&sbeSchemaId=1&sbeSchemaVersion=2';
  const connect=()=>tape.connect(url,()=>{},()=>{});
  let id=1;
  const send=(socket,method,params)=>tape.send(socket,JSON.stringify({jsonrpc:'2.0',id:id++,method,params}));
  const subscribe=(socket,channel)=>send(socket,'subscribe',{channel});
  const authenticate=async(socket)=>{send(socket,'auth',{bearer:'replay-paradex-jwt'});await new Promise(setImmediate);for(const channel of ['orders.ALL','fills.ALL','positions','balance_events','account'])subscribe(socket,channel);};
  const publicSocket=connect(),first=connect();subscribe(publicSocket,'markets_summary');await authenticate(first);
  assert.equal(tape.audit.ready(),true);
  const sibling=connect();
  sibling.onMessage('{"jsonrpc":"2.0","params":{"channel":"fills.ALL","data":{}}}');
  assert(tape.audit.errors().some(error=>error.includes('Unsubscribed Paradex data')));
  tape.close(first,1000,true);assert.equal(tape.audit.ready(),false,'closed authenticated connection cannot satisfy private readiness');
  await authenticate(sibling);assert.equal(tape.audit.ready(),true);
  const publicSibling=connect();subscribe(publicSibling,'markets_summary');
  send(publicSocket,'unsubscribe',{channel:'markets_summary'});
  assert.equal(tape.audit.ready(),true,'sibling owns its independent public subscription');
  tape.close(publicSibling,1000,true);assert.equal(tape.audit.ready(),false,'closed public connection cannot lend a topic');
});

test('production Paradex rejects case-insensitive duplicate auth headers without consuming a response', () => {
  const url='https://api.prod.paradex.trade/v1/orders/42';
  const tape=productionTapeFor('paradex',[{type:'http',method:'DELETE',url,status:200,body:'{}'}]);
  for(const headers of [
    [['Authorization','Bearer invalid'],['authorization','Bearer replay-paradex-jwt']],
    {Authorization:'Bearer invalid',authorization:'Bearer replay-paradex-jwt'},
    [['Authorization','Bearer replay-paradex-jwt'],['PARADEX-PARTNER','wrong'],['paradex-partner','TEALSTREET']],
  ]) {
    assert.throws(()=>tape.respond('DELETE',url,undefined,headers),/Duplicate Paradex request header/);
    assert.equal(tape.missingWrites,1,'duplicate authentication metadata cannot consume an acknowledgement');
  }
  assert.equal(tape.respond('DELETE',url,undefined,{Authorization:'Bearer replay-paradex-jwt'}).status,200);
  assert.equal(tape.missingWrites,0);
  assert.equal(tape.respond('DELETE',url,undefined,{Authorization:'Bearer replay-paradex-jwt'}).status,599);
});

test('production Phemex validates exact decoded query HMAC before route-scoped generated IDs and single-use ACKs', () => {
  const root='https://api.phemex.com';
  const path='/g-orders/create';
  const expected=new URLSearchParams({clOrdID:'Tealstreet_recorded',symbol:'BTCUSDT',orderQtyRq:'0.002',priceRp:'69300'});
  const tape=productionTapeFor('phemex',[{type:'http',method:'PUT',url:root+path+'?'+expected,status:200,body:'{}'}]);
  assert(tape.responseStore);
  const send=(clOrdID,extra={},valid=true)=>{
    const query=new URLSearchParams({clOrdID,symbol:'BTCUSDT',orderQtyRq:'0.002',priceRp:'69300',...extra});
    const expiry=String(Math.floor(Date.now()/1000+60));
    const signature=createHmac('sha256','replay-secret').update(path+decodeURIComponent(query.toString().replace(/\+/g,'%20'))+expiry).digest('hex');
    return tape.respond('PUT',root+path+'?'+query,undefined,{'X-PHEMEX-ACCESS-TOKEN':'replay-key','X-PHEMEX-REQUEST-EXPIRY':expiry,'X-PHEMEX-REQUEST-SIGNATURE':valid?signature:'invalid'});
  };
  assert.throws(()=>send('Tealstreet_runtime',{},false),/invalid Phemex request/);assert.equal(tape.missingWrites,1);
  for(const id of ['Tealstreet_','Tealstreet_bad\n','Tealstreet_'+ 'a'.repeat(22),'caller-undeclared']) {
    assert.equal(send(id).status,599);assert.equal(tape.missingWrites,1,'invalid/generated-looking identities remain exact');
  }
  for(const extra of [{orderQtyRq:'0.003'},{priceRp:'69301'},{unknownEconomicField:'extra'}]) {
    assert.equal(send('Tealstreet_runtime',extra).status,599);assert.equal(tape.missingWrites,1);
  }
  assert.equal(send('Tealstreet_runtime').status,200);assert.equal(tape.missingWrites,0);
  assert.equal(send('Tealstreet_another').status,599);
  const venue=venueOf('phemex');
  const key=(method,path,id,ids=new Set())=>requestKey(method,root+path+'?'+new URLSearchParams({clOrdID:id}),undefined,ids,venue);
  assert.notEqual(key('DELETE','/g-orders/cancel','Tealstreet_one'),key('DELETE','/g-orders/cancel','Tealstreet_two'));
  assert.notEqual(key('PUT',path,'Tealstreet_one',new Set(['Tealstreet_one'])),key('PUT',path,'Tealstreet_two',new Set(['Tealstreet_one'])));
});

test('production Phemex per-socket topics, auth ACKs and private business frames cannot borrow sibling state', () => {
  const tape=productionTapeFor('phemex',[]),url='wss://ws.phemex.com';
  const connect=()=>tape.connect(url,()=>{},()=>{});
  const send=(socket,method,params=[],id=1)=>tape.send(socket,JSON.stringify({method,params,id}));
  const authenticate=(socket)=>{
    const expiry=Math.floor(Date.now()/1000+60);
    const signature=createHmac('sha256','replay-secret').update('replay-key'+expiry).digest('hex');
    send(socket,'user.auth',['API','replay-key',signature,expiry],999);
    socket.onMessage('{"id":999,"error":null,"result":{"status":"success"}}');
    send(socket,'aop_p.subscribe');
  };
  const pub=connect(),first=connect();send(pub,'perp_market24h_pack_p.subscribe');authenticate(first);
  assert.equal(tape.audit.ready(),true);
  const sibling=connect();sibling.onMessage('{"orders_p":[]}');
  assert(tape.audit.errors().some(error=>error.includes('private data preceded authenticated subscription')));
  send(sibling,'aop_p.subscribe');assert(tape.audit.errors().some(error=>error.includes('preceded auth acknowledgement')));
  authenticate(sibling);tape.close(first,1000,true);assert.equal(tape.audit.ready(),true);
  tape.close(pub,1000,true);assert.equal(tape.audit.ready(),false,'closed public topics are not a live subscription');
  const replacement=connect();send(replacement,'perp_market24h_pack_p.subscribe');assert.equal(tape.audit.ready(),true);
  send(replacement,'perp_market24h_pack_p.unsubscribe');assert.equal(tape.audit.ready(),false);
  replacement.onMessage('{"method":"perp_market24h_pack_p.update","data":[]}');
  assert(tape.audit.errors().some(error=>error.includes('ticker data arrived without subscription')));
});

test('production Phemex refuses a signed out-of-order mutation before consuming its ACK', () => {
  const root='https://api.phemex.com';
  const create='/g-orders/create?clOrdID=Tealstreet_recorded&symbol=BTCUSDT';
  const cancel='/g-orders/cancel?symbol=BTCUSDT&orderID=exact-order';
  const tape=productionTapeFor('phemex',[
    {type:'http',method:'PUT',url:root+create,status:200,body:'{}'},
    {type:'http',method:'DELETE',url:root+cancel,status:200,body:'{}'},
  ]);
  const send=(method,path)=>{
    const url=new URL(root+path),expiry=String(Math.floor(Date.now()/1000+60));
    const signature=createHmac('sha256','replay-secret').update(url.pathname+decodeURIComponent(url.search.slice(1).replace(/\+/g,'%20'))+expiry).digest('hex');
    return tape.respond(method,url.href,undefined,{'X-PHEMEX-ACCESS-TOKEN':'replay-key','X-PHEMEX-REQUEST-EXPIRY':expiry,'X-PHEMEX-REQUEST-SIGNATURE':signature});
  };
  assert.equal(send('DELETE',cancel).status,599);assert.equal(tape.missingWrites,2);
  assert.equal(send('PUT',create.replace('Tealstreet_recorded','Tealstreet_runtime')).status,200);assert.equal(tape.missingWrites,1);
  assert.equal(send('DELETE',cancel).status,200);assert.equal(tape.missingWrites,0);
});


test('production native WebSocket local close emits one queued event and permits reconnect', async () => {
  const source = readFileSync(new URL('./replay.mjs', import.meta.url), 'utf8');
  const start = source.indexOf('  class TapeWebSocket {');
  const end = source.indexOf('  globalThis.WebSocket = TapeWebSocket;', start);
  assert(start >= 0 && end > start);
  const tape = new Tape([]);
  const Socket = new Function('tape', 'venue', 'exchange', `${source.slice(start, end)}; return TapeWebSocket;`)(tape, {}, undefined);
  const socket = new Socket('wss://example.test');
  let closes = 0;
  socket.addEventListener('close', () => { closes++; new Socket('wss://example.test'); });
  await new Promise(setImmediate);
  socket.close(); socket.close();
  assert.equal(closes, 0);
  assert.equal(socket.socket.closed, true);
  await new Promise(setImmediate);
  assert.equal(closes, 1);
  assert.equal(tape.sockets.length, 2);
});

test('production private GRVT activates a session only on HTTP delivery without a replay clock', async () => {
  const url = 'https://edge.grvt.io/auth/api_key/login';
  const body = '{"api_key":"grvt-replay-key"}';
  const tape = productionTapeFor('grvt', [{ type:'http', method:'POST', url, requestBody:body, status:200, body:'{}',
    headers:'X-Ts-Grvt-Session: fixture-session-1\nX-Grvt-Account-Id: fixture-account' }]);
  assert.equal(tape.replayClock, undefined);
  const response = tape.respond('POST', url, body, {'X-Grvt-Remember-Me':'true'});
  assert.equal(tape.requestAudit.current(), undefined);
  await new Promise(setImmediate);
  assert.equal(tape.requestAudit.current(), undefined);
  await tape.waitForResponse(response);
  assert.deepEqual(tape.requestAudit.current(), {cookie:'fixture-session-1', account:'fixture-account'});
  const other = productionTapeFor('grvt', []);
  assert.equal(other.requestAudit.current(), undefined);
});


test('production Coinbase route-scoped minted IDs preserve original types and declared callers', async () => {
  const {createPrivateKey,sign} = await import('node:crypto');
  const {COINBASE_REPLAY} = await import('./coinbase-protocol.mjs');
  const url='https://api.coinbase.com/api/v3/brokerage/orders';
  const body={client_order_id:'tealstreet-recorded',product_id:'BTC-USD',side:'BUY',order_configuration:{limit_limit_gtc:{base_size:'1',limit_price:'100'}}};
  const record={type:'http',method:'POST',url,requestBody:body,status:200,body:'{}'};
  const tape=productionTapeFor('coinbase',[record]);
  const key=createPrivateKey({key:Buffer.concat([Buffer.from('302e020100300506032b657004220420','hex'),Buffer.from(COINBASE_REPLAY.secret,'base64')]),format:'der',type:'pkcs8'});
  const part=value=>Buffer.from(JSON.stringify(value)).toString('base64url');
  const now=Math.floor(Date.now()/1000);
  const payload=part({alg:'EdDSA',typ:'JWT',kid:COINBASE_REPLAY.key,nonce:'production-nonce'})+'.'+part({sub:COINBASE_REPLAY.key,iss:'cdp',aud:['cdp_service'],nbf:now,exp:now+120,uri:'POST api.coinbase.com/api/v3/brokerage/orders'});
  const headers={Authorization:'Bearer '+payload+'.'+sign(null,Buffer.from(payload),key).toString('base64url')};
  for(const client_order_id of [123,null,true,'','tealstreet-x\n','tealstreet-'+'x'.repeat(22)]) {
    assert.equal(tape.respond('POST',url,JSON.stringify({...body,client_order_id}),headers).status,599);
    assert.equal(tape.missingWrites,1);
  }
  assert.equal(tape.respond('POST',url,JSON.stringify({...body,client_order_id:'tealstreet-generated'}),headers).status,200);
  assert.equal(tape.missingWrites,0);
  const caller='tealstreet-caller';
  const declared=productionTapeFor('coinbase',[{type:'command',op:'placeOrders',orders:[{clientOrderId:caller}]},{...record,requestBody:{...body,client_order_id:caller}}]);
  assert.equal(declared.respond('POST',url,JSON.stringify({...body,client_order_id:'tealstreet-other'}),headers).status,599);
  assert.equal(declared.missingWrites,1);
  assert.equal(declared.respond('POST',url,JSON.stringify({...body,client_order_id:caller}),headers).status,200);
});

test('production GRVT keeps unsupported root clock, signature and client fields exact after auth', async () => {
  const loginUrl='https://edge.grvt.io/auth/api_key/login',url='https://trades.grvt.io/lite/v1/cancel_order';
  const body={sa:'777',oi:'0x300'};
  const tape=productionTapeFor('grvt',[
    {type:'http',method:'POST',url:loginUrl,requestBody:{api_key:'grvt-replay-key'},status:200,body:'{}',headers:'X-Ts-Grvt-Session: fixture-session-1\nX-Grvt-Account-Id: fixture-account'},
    {type:'http',method:'POST',url,requestBody:body,status:200,body:'{}'},
  ]);
  await tape.waitForResponse(tape.respond('POST',loginUrl,'{"api_key":"grvt-replay-key"}',{'X-Grvt-Remember-Me':'true'}));
  const headers={'X-Ts-Grvt-Session':'fixture-session-1','X-Grvt-Account-Id':'fixture-account'};
  for(const key of ['timestamp','recvWindow','signature','requestId','clientOrderId','newClientOrderId','clientAlgoId']) {
    assert.equal(tape.respond('POST',url,JSON.stringify({...body,[key]:'unrecognized-business-field'}),headers).status,599,key);
    assert.equal(tape.missingWrites,1);
  }
  assert.equal(tape.respond('POST',url+'?signature=extra',JSON.stringify(body),headers).status,599);
  assert.equal(tape.missingWrites,1);
  assert.equal(tape.respond('POST',url,JSON.stringify(body),headers).status,200);
  assert.equal(tape.missingWrites,0);
});


test('production Axios adapter combines relative paths and preserves explicit nested parameter serialization', async () => {
  const source=readFileSync(new URL('./replay.mjs',import.meta.url),'utf8');
  const start=source.indexOf('  const adapter = async (config) => {');
  const end=source.indexOf("  const { ExchangeWorker }",start);
  const seen=[];
  const tape={respond(method,url,body,headers){seen.push({method,url,body,headers});return {status:200,body:'{}'};},waitForResponse:async response=>response};
  const adapter=new Function('tape','clock',`${source.slice(start,end)};return adapter;`)(tape,undefined);
  for(const [baseURL,url] of [['https://example.test/dxsca-web','marketdata'],['https://example.test/dxsca-web/','/marketdata']]) {
    await adapter({baseURL,url,method:'post',params:{token_id:['1','2']},paramsSerializer:{serialize:()=> 'token_id=1&token_id=2'},data:'{}'});
  }
  assert.deepEqual(seen.map(row=>row.url),['https://example.test/dxsca-web/marketdata?token_id=1&token_id=2','https://example.test/dxsca-web/marketdata?token_id=1&token_id=2']);
});

test('production polling private startup prepares subscriptions before playback clock activation', () => {
  const source=readFileSync(new URL('./replay.mjs',import.meta.url),'utf8');
  const start=source.indexOf('  if (clock) tape.prepareReadiness = subscribeBooks;');
  const end=source.indexOf('  // Model the worker posting each subscribed book',start);
  assert(start>=0&&end>start);
  const prepare=new Function('clock',`const tape={};let subscribedDone=false;const subscribeBooks=()=>{subscribedDone=true;};${source.slice(start,end)};return {tape,subscribed:()=>subscribedDone};`);
  const privateStartup=prepare({active:false,startup:false});
  assert.equal(privateStartup.subscribed(),true);
  const logical=prepare({active:true,startup:true});
  assert.equal(logical.subscribed(),false);
  logical.tape.prepareReadiness();
  assert.equal(logical.subscribed(),true);
});

test('production Binance modes retain raw auth and route-scoped generated identities before one-shot writes', () => {
  const paths={binance:['https://fapi.binance.com','/fapi/v1/order'],binancepm:['https://papi.binance.com','/papi/v1/um/order'],binanceinverse:['https://dapi.binance.com','/dapi/v1/order'],binancespot:['https://api.binance.com','/api/v3/order']};
  for(const [name,[origin,path]] of Object.entries(paths)) {
    const url=origin+path,base={symbol:name==='binanceinverse'?'BTCUSD_PERP':'BTCUSDT',quantity:'7'};
    const record={type:'http',method:'POST',url:url+'?'+new URLSearchParams(base),status:200,body:'{"clientOrderId":"$newClientOrderId"}'};
    const tape=productionTapeFor(name,[record]);
    assert(tape.responseStore,name+' must use shared typed Responses');
    const signed=(fields,headers={'X-MBX-APIKEY':'replay-key'},target=url,method='POST')=>{
      const params=new URLSearchParams({...base,...fields,timestamp:String(Date.now()),recvWindow:'5000'});
      const query=params.toString();
      return tape.respond(method,target+'?'+query+'&signature='+createHmac('sha256','replay-secret').update(query).digest('hex'),undefined,headers);
    };
    const valid='x-s3vFFRQpGenerated';
    assert.throws(()=>signed({newClientOrderId:valid},[['X-MBX-APIKEY','wrong'],['x-mbx-apikey','replay-key']]),/Duplicate request header/);
    assert.equal(tape.missingWrites,1);
    for(const newClientOrderId of ['arbitrary','',valid+'\n',valid+'\r',valid+'\u2028','x-s3vFFRQp'+'x'.repeat(40)]) {
      assert.equal(signed({newClientOrderId}).status,599,name+JSON.stringify(newClientOrderId));
      assert.equal(tape.missingWrites,1);
    }
    assert.equal(signed({newClientOrderId:valid,quantity:'8'}).status,599);
    const response=signed({newClientOrderId:valid});
    assert.equal(response.status,200,name);
    assert.deepEqual(JSON.parse(response.body),{clientOrderId:valid});
    assert.equal(tape.missingWrites,0);
    assert.equal(signed({newClientOrderId:valid}).status,599);
    const caller='x-s3vFFRQpCaller';
    const callerTape=productionTapeFor(name,[{type:'command',op:'placeOrders',orders:[{clientOrderId:caller}]},{...record,url:record.url+'&newClientOrderId='+caller}]);
    const sendCaller=(id)=>{
      const query=new URLSearchParams({...base,newClientOrderId:id,timestamp:String(Date.now()),recvWindow:'5000'}).toString();
      return callerTape.respond('POST',url+'?'+query+'&signature='+createHmac('sha256','replay-secret').update(query).digest('hex'),undefined,{'X-MBX-APIKEY':'replay-key'});
    };
    assert.equal(sendCaller(valid).status,599,name+' caller must remain exact');
    assert.equal(callerTape.missingWrites,1);
    assert.equal(sendCaller(caller).status,200);
  }
});

test('production Binance cancellation targets, duplicate queries and batch JSON value types stay exact', () => {
  const name='binanceinverse',url='https://dapi.binance.com/dapi/v1/order';
  const base={symbol:'BTCUSD_PERP',orderId:'123'};
  const tape=productionTapeFor(name,[{type:'http',method:'DELETE',url:url+'?'+new URLSearchParams(base),status:200,body:'{}'}]);
  const send=(extra='')=>{
    const query=new URLSearchParams({...base,timestamp:String(Date.now()),recvWindow:'5000'}).toString()+extra;
    return tape.respond('DELETE',url+'?'+query+'&signature='+createHmac('sha256','replay-secret').update(query).digest('hex'),undefined,{'X-MBX-APIKEY':'replay-key'});
  };
  assert.equal(send('&newClientOrderId=x-s3vFFRQpExtra').status,599);
  assert.throws(()=>send('&orderId=123'),/Duplicate Binance query/);
  assert.equal(tape.missingWrites,1);
  assert.equal(send().status,200);
  const batchUrl='https://dapi.binance.com/dapi/v1/batchOrders';
  const order={symbol:'BTCUSD_PERP',quantity:'7'};
  const batch=productionTapeFor(name,[{type:'http',method:'POST',url:batchUrl+'?batchOrders='+encodeURIComponent(JSON.stringify([order])),status:200,body:'[]'}]);
  const post=(value)=>{
    const query=new URLSearchParams({batchOrders:JSON.stringify([{...order,newClientOrderId:value}]),timestamp:String(Date.now()),recvWindow:'5000'}).toString();
    return batch.respond('POST',batchUrl+'?'+query+'&signature='+createHmac('sha256','replay-secret').update(query).digest('hex'),undefined,{'X-MBX-APIKEY':'replay-key'});
  };
  for(const value of [123,null,true,[],['x-s3vFFRQpGenerated'],{},'x-s3vFFRQpBad\n']) {
    assert.equal(post(value).status,599);assert.equal(batch.missingWrites,1);
  }
  assert.equal(post('x-s3vFFRQpGenerated').status,200);
});

test('production TxFlow typed writes retain economics, caller IDs and unknown fields after raw authentication', async () => {
  const { createRequire } = await import('node:module');
  const require=createRequire(new URL('../../packages/safe-cex/package.json',import.meta.url));
  const ethers=require('ethers'),{encode}=require('@msgpack/msgpack');
  const venue=venueOf('txflow');await venue.initializeReplay('ts',venue.replayCredentials);
  const url='https://rpc.txflow-testnet.xyz/exchange',headers={'Content-Type':'application/json','x-api-version':'2026-08-16'};
  const caller='ts_declaredcaller',order={a:1,b:true,p:'90',s:'0.25',r:false,t:{limit:{tif:'Gtc'}},c:caller};
  const sign=(row,nonce=Date.now(),extra={})=>{
    const body={grouping:'na',orders:[row]},suffix=Buffer.alloc(9);suffix.writeBigUInt64BE(BigInt(nonce));
    const digest=ethers.TypedDataEncoder.hash({name:'TxFlow-Testnet',version:'1',chainId:2511,verifyingContract:ethers.ZeroAddress},
      {Agent:[{name:'txflowNetwork',type:'string'},{name:'chainId',type:'uint32'},{name:'apiVersion',type:'uint32'},{name:'connectionId',type:'bytes32'}]},
      {txflowNetwork:'TxFlow-Testnet',chainId:2511,apiVersion:1,connectionId:ethers.keccak256(Buffer.concat([encode(body),suffix]))});
    const signature=new ethers.SigningKey(venue.replayCredentials.secret).sign(digest);
    return {action:{type:'order',...body},nonce,signature:{r:signature.r,s:signature.s,v:signature.v},confirm:'executed',...extra};
  };
  const records=[{type:'command',op:'placeOrders',orders:[{clientOrderId:caller}]},{type:'http',method:'POST',url,requestBody:sign(order),status:200,body:'{}'}];
  for(const changed of [{...order,c:'other-caller'},{...order,s:'0.3'}]){
    const tape=productionTapeFor('txflow',records);const nonce=Date.now();
    assert.equal(tape.respond('POST',url,sign(changed,nonce),headers).status,599);assert.equal(tape.missingWrites,1);
    assert.equal(tape.respond('POST',url,sign(order,nonce+1),headers).status,200);assert.equal(tape.missingWrites,0);
  }
  for(const field of ['requestId','timestamp','recvWindow']){
    const tape=productionTapeFor('txflow',records),nonce=Date.now();
    assert.equal(tape.respond('POST',url,sign(order,nonce,{[field]:'unknown-business-field'}),headers).status,599);
    assert.equal(tape.missingWrites,1);assert.equal(tape.respond('POST',url,sign(order,nonce+1),headers).status,200);
  }
  const tape=productionTapeFor('txflow',records),signed=sign(order);
  assert.throws(()=>tape.respond('POST',url,signed,[...Object.entries(headers),['X-API-VERSION','wrong']]));
  assert.equal(tape.missingWrites,1);assert.equal(tape.respond('POST',url,signed,headers).status,200);
});

test('production Binance COIN-M account metadata derives only from native rows and their own multiplier', () => {
  const source = readFileSync(new URL('./replay.mjs', import.meta.url), 'utf8');
  const start = source.indexOf('function rustAccount('), end = source.indexOf('\nasync function runTs()', start);
  const orderColumns = ['amount','filled','remaining'], positionColumns = ['notional','unrealizedPnl'];
  const derive = new Function('venue','ORDER_COLUMNS','POSITION_COLUMNS','orderEnums','positionEnums', `${source.slice(start,end)};return rustAccount;`)(
    {coinNativeAmounts:true},orderColumns,positionColumns,()=>({}),()=>({}));
  const input = () => ({markets:{symbol:()=> 'BTCUSD_PERP',contractSize:[100]},
    orders:{count:1,id:()=> 'order',clientOrderId:()=>undefined,parentId:()=>undefined,market:[0],enums:[0],amount:[10],filled:[3],remaining:[7]},
    positions:{count:1,market:[0],enums:[0],notional:[700],unrealizedPnl:[2]},balance:{},fills:[]});
  const projection = (row) => {const account=derive(row);return [account.orders[0].extra.remainingNative,account.positions[0].extra.contractsNative];};
  assert.deepEqual(projection(input()),[7,7]);
  for (const mutate of [(row)=>row.orders.remaining[0]=6,(row)=>row.positions.notional[0]=600,(row)=>row.markets.contractSize[0]=200]) {
    const row=input();mutate(row);assert.notDeepEqual(projection(row),[7,7]);
  }
});

test('production Kraken Spot preserves authenticated typed economics and scopes generated IDs to placement', async () => {
  const { krakenSpotSignature } = await import('./krakenspot.mjs');
  const url='https://api.kraken.com/0/private/AddOrder',id='a'.repeat(32);
  const expected={pair:'XXBTZUSD',type:'buy',ordertype:'limit',volume:'1',price:'90',cl_ord_id:id,nonce:'0'};
  const record={type:'http',method:'POST',url,requestBody:new URLSearchParams(expected).toString(),status:200,body:'{}'};
  const signed=(body)=>({'API-Key':'replay-key','API-Sign':krakenSpotSignature('/0/private/AddOrder',body)});
  let nonce=BigInt(Date.now())*1000n;
  const request=(patch={})=>new URLSearchParams({...expected,...patch,nonce:String(++nonce)}).toString();
  for (const patch of [{volume:'2'},{cl_ord_id:id+'\n'},{cl_ord_id:''},{requestId:'unknown'},{signature:'unknown'},{clientOrderId:'unknown'}]) {
    const tape=productionTapeFor('krakenspot',[record]),body=request(patch);
    try {assert.equal(tape.respond('POST',url,body,signed(body)).status,599);} catch(error) {assert.match(error.message,/client ID/);}
    assert.equal(tape.unusedWrites(),1);
    const valid=request({cl_ord_id:'b'.repeat(32)});
    assert.equal(tape.respond('POST',url,valid,signed(valid)).status,200);
    assert.equal(tape.unusedWrites(),0);
  }
  const tape=productionTapeFor('krakenspot',[record]),body=request();
  assert.throws(()=>tape.respond('POST',url,body,{'API-Key':'replay-key','API-Sign':'invalid'}),/signature/);
  assert.equal(tape.respond('POST',url,body,signed(body)).status,200,'invalid HMAC must not commit nonce');
});

test('production Kraken Spot token activates only after delivery and socket topics cannot cross generations', async () => {
  const { krakenSpotSignature } = await import('./krakenspot.mjs');
  const url='https://api.kraken.com/0/private/GetWebSocketsToken';
  const tape=productionTapeFor('krakenspot',[{type:'http',method:'POST',url,requestBody:'nonce=0',status:200,body:JSON.stringify({error:[],result:{token:'delivered-token'}})}]);
  const socket=tape.connect('wss://ws-auth.kraken.com/v2',()=>{},()=>{});
  const subscribe=(channel)=>JSON.stringify({method:'subscribe',params:{channel,token:'delivered-token'}});
  const body='nonce='+String(BigInt(Date.now())*1000n);
  const response=tape.respond('POST',url,body,{'API-Key':'replay-key','API-Sign':krakenSpotSignature('/0/private/GetWebSocketsToken',body)});
  assert.throws(()=>tape.send(socket,subscribe('executions')),/delivered token/);
  await tape.waitForResponse(response);
  tape.send(socket,subscribe('executions'));tape.send(socket,subscribe('balances'));
  assert.equal(tape.audit.ready(),true);
  const sibling=tape.connect(socket.rawUrl,()=>{},()=>{});
  assert.throws(()=>tape.send(sibling,subscribe('executions')),/fresh token/);
  sibling.onMessage(JSON.stringify({channel:'executions',type:'update',data:[]}));
  assert(tape.audit.errors().some((error)=>/own subscription/.test(error)));
  tape.close(socket,1000,true);assert.equal(tape.audit.ready(),false);
});

test('production spot public sends populate own full topic identities and isolate siblings and unsubscribe', async () => {
  for (const name of ['hyperliquidspot','krakenspot']) {
    const tape=productionTapeFor(name,[]);
    const url=name==='hyperliquidspot'?'wss://api.hyperliquid.xyz/ws':'wss://ws.kraken.com/v2';
    const socket=tape.connect(url,()=>{},()=>{}),sibling=tape.connect(url,()=>{},()=>{});
    const send=(method)=>JSON.stringify(name==='hyperliquidspot'?{method,subscription:{type:'l2Book',coin:'@1'}}:{method,params:{channel:'book',symbol:['BTC/USD']}});
    const frame=JSON.stringify(name==='hyperliquidspot'?{channel:'l2Book',data:{coin:'@1',levels:[[],[]]}}:{channel:'book',data:[{symbol:'BTC/USD'}]});
    tape.send(socket,send('subscribe'));assert(tape.fullSubscriptions.size>0);
    socket.onMessage(frame);assert.deepEqual(tape.audit.errors(),[]);
    sibling.onMessage(frame);assert(tape.audit.errors().length>0);
    tape.send(socket,send('unsubscribe'));const before=tape.audit.errors().length;socket.onMessage(frame);
    assert(tape.audit.errors().length>=before);
  }
});

test('production Hyperliquid Spot authenticates raw bytes before retaining caller IDs and typed extra fields', async () => {
  const { createRequire } = await import('node:module');
  const require=createRequire(new URL('../../packages/safe-cex/package.json',import.meta.url));
  const {ethers}=require('ethers'),{encode}=require('@msgpack/msgpack');
  const {HYPERLIQUID_SPOT_REPLAY}=await import('./hyperliquidspot.mjs');
  const wallet=new ethers.Wallet(HYPERLIQUID_SPOT_REPLAY.secret),url='https://api.hyperliquid.xyz/exchange';
  const caller='0x'+'01'.repeat(16);
  const action={type:'order',orders:[{a:10001,b:true,p:'90',s:'1',r:false,t:{limit:{tif:'Gtc'}},c:caller}],grouping:'na'};
  const sign=async(value,extra={})=>{
    const nonce=Date.now(),suffix=Buffer.alloc(9);suffix.writeBigUInt64BE(BigInt(nonce));
    const signature=ethers.Signature.from(await wallet.signTypedData({name:'Exchange',version:'1',chainId:1337,verifyingContract:ethers.ZeroAddress},
      {Agent:[{name:'source',type:'string'},{name:'connectionId',type:'bytes32'}]},
      {source:'a',connectionId:ethers.keccak256(Buffer.concat([encode(value),suffix]))})).toJSON();
    return JSON.stringify({action:value,nonce,signature,...extra});
  };
  const records=[{type:'command',op:'placeOrders',orders:[{clientOrderId:caller}]},
    {type:'http',method:'POST',url,requestBody:JSON.stringify({action,nonce:0,signature:{}}),status:200,body:'{}'}];
  for (const [change,extra] of [[{...action,orders:[{...action.orders[0],c:'0x'+'02'.repeat(16)}]},{}],
    [{...action,orders:[{...action.orders[0],s:1}]},{}],[action,{requestId:'unknown'}],[action,{clientOrderId:'unknown'}],[action,{timestamp:1}]]) {
    const tape=productionTapeFor('hyperliquidspot',records),raw=await sign(change,extra);
    assert.equal(tape.respond('POST',url,raw).status,599);assert.equal(tape.unusedWrites(),1);
    assert.equal(tape.respond('POST',url,await sign(action)).status,200);assert.equal(tape.unusedWrites(),0);
  }
  const tape=productionTapeFor('hyperliquidspot',records),raw=await sign(action);
  assert.throws(()=>tape.respond('POST',url+'?signature=unknown',raw),/URL fields/);
  assert.throws(()=>tape.respond('POST',url,raw,[['Content-Type','application/json'],['content-type','application/json']]),/Duplicate/);
  assert.equal(tape.unusedWrites(),1);assert.equal(tape.respond('POST',url,raw).status,200);
});

test('production Hyperliquid Spot accepts only documented book acknowledgement defaults on its own socket', () => {
  const url='wss://api.hyperliquid.xyz/ws',topic={type:'l2Book',coin:'@1'};
  for (const [change,valid] of [[{},true],[{mantissa:7},false],[{fast:true},false],[{fast:'false'},false],[{nSigFigs:3},false],[{unknown:1},false],[{coin:'@107'},false]]) {
    const tape=productionTapeFor('hyperliquidspot',[]),socket=tape.connect(url,()=>{},()=>{});
    tape.send(socket,JSON.stringify({method:'subscribe',subscription:topic}));
    socket.onMessage(JSON.stringify({channel:'subscriptionResponse',data:{method:'subscribe',subscription:{...topic,mantissa:null,fast:false,nSigFigs:null,...change}}}));
    assert.equal(tape.audit.errors().length===0,valid,JSON.stringify(change));
  }
});


// These values are re-signed, so rejection must validate the clock contract,
// rather than merely detect a digest mismatch after changing a signed field.
const malformedBitunixTimestamps = [
  'not-a-time', '', ' ', ' 1700000000000 ', '1700000000000.5', '1.7e12',
  '9007199254740992', -1, 1700000000000.5, Number.MAX_SAFE_INTEGER + 1,
  undefined, null, true, {},
];
const bitunixDigest = (timestamp, nonce, query = '', body = '') => {
  const hash = (text) => createHash('sha256').update(text).digest('hex');
  return hash(hash(`${nonce}${timestamp}replay-key${query}${body}`) + 'replay-secret');
};
const bitunixLoginArg = (timestamp) => {
  // Sign exactly the value that the JSON socket frame will carry.
  const arg = JSON.parse(JSON.stringify({ apiKey: 'replay-key', nonce: 'honest-replay-nonce', timestamp }));
  return { ...arg, sign: bitunixDigest(arg.timestamp, arg.nonce) };
};

test('production Bitunix HTTP rejects re-signed malformed clocks before consuming the honest ACK', () => {
  const url = 'https://fapi.bitunix.com/api/v1/futures/trade/place_order';
  const body = JSON.stringify({ symbol: 'BTCUSDT', qty: '0.002', price: '90500', side: 'BUY', orderType: 'LIMIT' });
  const records = [{ type: 'http', method: 'POST', url, requestBody: body, status: 200, body: '{"code":0,"data":{"orderId":"1003"}}' }];
  const headers = (timestamp) => ({ 'api-key': 'replay-key', nonce: 'honest-replay-nonce',
    ...(timestamp === undefined ? {} : { timestamp }), sign: bitunixDigest(timestamp, 'honest-replay-nonce', '', body) });
  for (const timestamp of malformedBitunixTimestamps) {
    const tape = productionTapeFor('bitunix', records);
    assert.throws(() => tape.respond('POST', url, body, headers(timestamp)), /signature|timestamp/i);
    assert.equal(tape.signatureErrors.length, 1, 'authentication rejection remains sticky');
    assert.equal(tape.missingWrites, 1, 'malformed signed time cannot consume the honest ACK');
    assert.equal(tape.requests.size, 0);
    assert.deepEqual(tape.writes, []);
    assert.deepEqual(tape.outbound, []);
    assert.equal(tape.respond('POST', url, body, headers(String(Date.now()))).status, 200);
    assert.equal(tape.missingWrites, 0);
    assert.equal(tape.signatureErrors.length, 1, 'a later valid request cannot clear the failure');
    assert.equal(tape.respond('POST', url, body, headers(String(Date.now()))).status, 599);
  }
  for (const timestamp of [Date.now(), String(Date.now())]) {
    const tape = productionTapeFor('bitunix', records);
    assert.equal(tape.respond('POST', url, body, headers(timestamp)).status, 200, 'numeric TS and decimal-string Rust clocks both remain valid');
    assert.deepEqual(tape.signatureErrors, []);
    assert.equal(tape.respond('POST', url, body, headers(timestamp)).status, 599);
  }
});

test('production Bitunix socket authenticates only after its re-signed timestamp validates', () => {
  const url = 'wss://fapi.bitunix.com/private/';
  for (const timestamp of malformedBitunixTimestamps) {
    const tape = productionTapeFor('bitunix', []);
    const socket = tape.connect(url, () => {}, () => {});
    tape.openSocket(socket);
    assert.throws(() => tape.send(socket, JSON.stringify({ op: 'login', args: [bitunixLoginArg(timestamp)] })), /signature|timestamp/i);
    assert.equal(Boolean(socket.authenticated), false);
    assert.equal(tape.signatureErrors.length, 1);
    tape.send(socket, JSON.stringify({ op: 'login', args: [bitunixLoginArg(String(Date.now()))] }));
    assert.equal(socket.authenticated, true);
    assert.equal(tape.signatureErrors.length, 1, 'valid retry preserves the original authentication failure');
    tape.close(socket);
  }
  for (const timestamp of [Date.now(), String(Date.now())]) {
    const tape = productionTapeFor('bitunix', []);
    const socket = tape.connect(url, () => {}, () => {});
    tape.openSocket(socket);
    tape.send(socket, JSON.stringify({ op: 'login', args: [bitunixLoginArg(timestamp)] }));
    assert.equal(socket.authenticated, true);
    assert.deepEqual(tape.signatureErrors, []);
    tape.close(socket);
  }
});

test('production Bitunix login requires exactly one independently verified argument', () => {
  for (const args of [undefined, [], [bitunixLoginArg(Date.now()), bitunixLoginArg(Date.now())]]) {
    const tape = productionTapeFor('bitunix', []);
    const socket = tape.connect('wss://fapi.bitunix.com/private/', () => {}, () => {});
    tape.openSocket(socket);
    assert.throws(() => tape.send(socket, JSON.stringify({ op: 'login', ...(args === undefined ? {} : { args }) })), /signature|timestamp|login/i);
    assert.equal(Boolean(socket.authenticated), false);
    assert.equal(tape.signatureErrors.length, 1);
    tape.send(socket, JSON.stringify({ op: 'login', args: [bitunixLoginArg(Date.now())] }));
    assert.equal(socket.authenticated, true);
    assert.equal(tape.signatureErrors.length, 1);
    tape.close(socket);
  }
});


test('production native account projection retains stored identities and per-leg triggers with legacy fallback', () => {
  const source = readFileSync(new URL('./replay.mjs', import.meta.url), 'utf8');
  const start = source.indexOf('function rustAccount('), end = source.indexOf('\nasync function runTs()', start);
  assert(start >= 0 && end > start);
  const project = new Function('venue', 'ORDER_COLUMNS', 'POSITION_COLUMNS', 'orderEnums', 'positionEnums',
    `${source.slice(start, end)};return rustAccount;`)({}, [], [], () => ({}), () => ({}));
  const input = () => ({
    markets: { symbol: () => 'REPLACEMENT' }, balance: {},
    orders: { count: 1, market: [0], enums: [0], id: () => 'order', clientOrderId: () => undefined, parentId: () => undefined,
      symbol: () => 'ORPHAN-ORDER', tpTriggerType: () => 'mark', slTriggerType: () => 'last' },
    positions: { count: 1, market: [0], enums: [0], symbol: () => 'ORPHAN-POSITION' },
    settings: { count: 1, market: [0], leverage: [5], marginType: () => 'cross', symbol: () => 'ORPHAN-SETTING' },
    fills: [{ market: 0, symbol: 'DELIVERED-FILL', side: 0, id: 'order', amount: 1 }],
  });
  const modern = project(input());
  assert.deepEqual([modern.orders[0].symbol, modern.positions[0].symbol, modern.settings[0].symbol, modern.fills[0].symbol],
    ['ORPHAN-ORDER', 'ORPHAN-POSITION', 'ORPHAN-SETTING', 'DELIVERED-FILL']);
  assert.equal(modern.orders[0].tpTriggerType, 'mark');
  assert.equal(modern.orders[0].slTriggerType, 'last');
  const legacy = input();
  for (const rows of [legacy.orders, legacy.positions, legacy.settings]) delete rows.symbol;
  delete legacy.orders.tpTriggerType;
  delete legacy.orders.slTriggerType;
  delete legacy.fills[0].symbol;
  const old = project(legacy);
  assert.deepEqual([old.orders[0].symbol, old.positions[0].symbol, old.settings[0].symbol, old.fills[0].symbol],
    ['REPLACEMENT', 'REPLACEMENT', 'REPLACEMENT', 'REPLACEMENT']);
  assert.equal(Object.hasOwn(old.orders[0], 'tpTriggerType'), false);
});

test('production fill delivery snapshots stored symbols and ticker projection prefers stored identities', () => {
  const source = readFileSync(new URL('./replay.mjs', import.meta.url), 'utf8');
  const start = source.indexOf('      const executed = frame.fills();');
  const end = source.indexOf('\n    },', start);
  assert(start >= 0 && end > start);
  const deliver = new Function('frame', 'fills', 'markets', source.slice(start, end));
  let deliveredSymbol = 'ORIGINAL';
  const fills = [];
  deliver({ fills: () => ({ count: 1, market: [0], side: [0], rows: [90, 1], orderId: () => 'order', symbol: () => deliveredSymbol }) }, fills);
  deliveredSymbol = 'REPLACED';
  assert.equal(fills[0].symbol, 'ORIGINAL');
  let catalogSymbol = 'LEGACY-AT-DELIVERY';
  const deliveryMarkets = { symbol: () => catalogSymbol };
  deliver({ fills: () => ({ count: 1, market: [0], side: [1], rows: [91, 2], orderId: () => 'legacy' }) }, fills, deliveryMarkets);
  catalogSymbol = 'REINDEXED-MARKET';
  assert.equal(fills[1].symbol, 'LEGACY-AT-DELIVERY');
  deliver({ fills: () => ({ count: 1, market: [0], side: [1], rows: [92, 3], orderId: () => 'no-catalog' }) }, fills);
  assert.equal(fills[2].symbol, undefined, 'only a missing catalog defers legacy symbol resolution');
  const tickerStart = source.indexOf('  const tickerRows = [];');
  const tickerEnd = source.indexOf('\n  const bookRows = {};', tickerStart);
  assert(tickerStart >= 0 && tickerEnd > tickerStart);
  const tickers = new Function('tickers', 'markets', `${source.slice(tickerStart, tickerEnd)};return tickerRows;`);
  const markets = { symbol: () => 'CURRENT-MARKET' };
  assert.equal(tickers({ count: 1, market: [0], columns: {}, identity: () => ({ symbol: 'STORED-TICKER' }) }, markets)[0].symbol, 'STORED-TICKER');
  assert.equal(tickers({ count: 1, market: [0], columns: {} }, markets)[0].symbol, 'CURRENT-MARKET');
});

test('production TS command dispatch forwards leverage side and position-mode symbol', async () => {
  const source = readFileSync(new URL('./replay.mjs', import.meta.url), 'utf8');
  const start = source.indexOf('async function runTsCommand('), end = source.indexOf('\n/** The fields both implementations carry', start);
  assert(start >= 0 && end > start);
  const command = new Function('commandArgs', `${source.slice(start, end)};return runTsCommand;`)((record) => record);
  const seen = [];
  const exchange = { setLeverage: (...args) => seen.push(args), changePositionMode: (...args) => seen.push(args) };
  await command(exchange, {}, { op: 'setLeverage', symbol: 'BTCUSDT', leverage: 5, side: 'long' }, {});
  await command(exchange, {}, { op: 'setPositionMode', hedged: true, symbol: 'BTCUSDT' }, {});
  assert.deepEqual(seen, [['BTCUSDT', 5, 'long'], [true, 'BTCUSDT']]);
});


test('production Coinbase market projection preserves contract flags required by strict midpoint proof', async () => {
  const { compareCoinbaseSpotMidpoints } = await import('./coinbase-spot-midpoints.mjs');
  const source = readFileSync(new URL('./replay.mjs', import.meta.url), 'utf8');
  const start = source.indexOf('const marketFields = '), end = source.indexOf('\nconst assertReplayIntegrity = ', start);
  assert(start >= 0 && end > start);
  const product = {
    product_id: 'DEEP-USD', product_type: 'SPOT', future_product_details: null,
    base_currency_id: 'DEEP', quote_currency_id: 'USD', status: 'online',
    is_disabled: false, trading_disabled: false, base_increment: '1', price_increment: '0.00001',
    base_min_size: '1', base_max_size: '100000000', price: '0.01875', volume_24h: '2625844',
    approximate_quote_24h_volume: '49234.58',
  };
  const rawMarket = {
    id: 'DEEP-USD', symbol: 'DEEP-USD', base: 'DEEP', quote: 'USD', active: true,
    isSpot: true, isInverse: false, isExpiring: false, contractSize: 1,
    precision: { amount: 1, price: 0.00001 }, limits: { amount: { min: 1, max: 100000000 } },
  };
  const records = [
    { type: 'meta', venue: 'coinbase', profile: 'spot', private: false },
    { type: 'http', t: 10, requestOrdinal: 0, method: 'GET', status: 200,
      url: 'https://api.coinbase.com/api/v3/brokerage/market/products?limit=1000',
      body: JSON.stringify({ products: [product], num_products: 1,
        pagination: { has_next: false, has_prev: false, next_cursor: '', prev_cursor: '' } }) },
  ];
  for (const name of ['coinbase', 'coinbasespot']) {
    const project = new Function('venue', `${source.slice(start, end)};return marketFields;`)(venueOf(name));
    const output = (quoteVolume) => ({ markets: [project({ ...rawMarket })],
      tickers: [{ symbol: 'DEEP-USD', last: 0.01875, volume: 2625844, quoteVolume }] });
    const ts = output(49234.57), rust = output(49234.58);
    const prove = () => compareCoinbaseSpotMidpoints({ venue: name, records, ts, rust });
    const proof = prove();
    assert.equal(proof.equivalent, true, `${name}: ${proof.reason}`);
    assert.deepEqual(proof.exceptions.map((row) => row.symbol), ['DEEP-USD']);
    for (const [field, invalid] of [['isSpot', false], ['isInverse', true], ['isExpiring', true]]) {
      const expected = rust.markets[0][field];
      rust.markets[0][field] = invalid;
      assert.equal(prove().equivalent, false, `${name}: changed ${field} must invalidate the proof`);
      rust.markets[0][field] = expected;
    }
  }
});
