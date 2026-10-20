import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { createHmac } from 'node:crypto';
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
    `${source.slice(start, classStart)}\nreturn (${source.slice(classStart, end)});`)(
    Tape, { venue: name }, venue, records, [], new Set(), requestKey, Responses, undefined,
    parseRequestJson, (method) => method === 'GET', true,
  );
  return new ProductionTape('ts');
};

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
