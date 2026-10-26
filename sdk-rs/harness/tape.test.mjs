import assert from 'node:assert/strict';
import test from 'node:test';
import {
  bodyBytes,
  canonicalBody,
  canonicalUrl,
  deliverSocketFrame,
  replayStoreReady,
  ReplayClock,
  Tape,
} from './tape.mjs';
import { createTape, withTimeout, sameCommandResults } from './tape.mjs';
import { createHmac, createPrivateKey, sign } from 'node:crypto';
import { venueOf } from './venues.mjs';
import { VENUES } from './venues.mjs';

const canonicalMexcBody = (body, ids) => canonicalBody(body, ids, VENUES.mexc);
const canonicalMexcUrl = (url) => canonicalUrl(url, new Set(), VENUES.mexc);
const url = 'https://api.mexc.com/api/v1/private/order/cancel';
test('POST matching preserves snowflakes and consumes every accepted write once', () => {
  const tape = new Tape(
    [
      {
        type: 'http',
        method: 'POST',
        url,
        status: 200,
        body: 'accepted',
        requestBody: '{"id":"739113577038255617","vol":2}',
      },
    ],
    VENUES.mexc,
  );
  assert.equal(tape.respond('POST', url, '{"vol":2,"id":739113577038255617}').body, 'accepted');
  assert.equal(tape.respond('POST', url, '{"id":"739113577038255617","vol":2}').status, 599);
  assert.notEqual(canonicalMexcBody('[739113577038255617]'), canonicalMexcBody('[739113577038255618]'));
});

test('body matching ignores only generated externalOid and keeps nested business fields', () => {
  assert.equal(
    canonicalMexcBody('[{"externalOid":"random1","price":"59000","vol":60}]'),
    canonicalMexcBody('[{"vol":60,"price":"59000","externalOid":"random2"}]'),
  );
  assert.notEqual(canonicalMexcBody('{"vol":60}'), canonicalMexcBody('{"vol":61}'));
  assert.notEqual(canonicalMexcBody('{"symbol":"BTC_USDT"}'), canonicalMexcBody('{"symbol":"ETH_USDT"}'));
});

test('only GET may reuse its last recorded snapshot', () => {
  const tape = new Tape([
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
    canonicalMexcUrl(`${active}?start_time=10&end_time=100&states=1`),
    canonicalMexcUrl(`${active}?states=1&end_time=200&start_time=110`),
  );
  assert.notEqual(
    canonicalMexcUrl(`${active}?start_time=10&end_time=100`),
    canonicalMexcUrl(`${active}?start_time=11&end_time=100`),
  );
  const history = 'https://api.mexc.com/api/v1/private/order/list/history_orders';
  assert.notEqual(
    canonicalMexcUrl(`${history}?start_time=10&end_time=100`),
    canonicalMexcUrl(`${history}?start_time=110&end_time=200`),
  );
  assert.equal(
    canonicalMexcUrl('https://fapi.binance.com/fapi/v1/order?symbol=BTCUSDT&timestamp=1&signature=a'),
    canonicalMexcUrl('https://fapi.binance.com/fapi/v1/order?signature=b&timestamp=2&symbol=BTCUSDT'),
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
  const tape = new Tape(records);
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

test('private startup advances by timer deadlines and rebases pending timers at tape readiness', async () => {
  const epoch = 1700000000000;
  const clock = new ReplayClock(epoch, true);
  clock.install();
  try {
    assert.equal(Date.now(), epoch);
    await new Promise((resolve) => setTimeout(resolve, 1));
    assert.equal(Date.now(), epoch + 1);
    let observed;
    setTimeout(() => { observed = Date.now(); }, 100);
    clock.start();
    assert.equal(Date.now(), epoch);
    await clock.advance(99);
    assert.equal(observed, undefined);
    await clock.advance(100);
    assert.equal(observed, epoch + 100);
  } finally { clock.restore(); }
});

test('a continuous private startup never moves Date.now backwards at tape readiness', async () => {
  const epoch = 1700000000000;
  const clock = new ReplayClock(epoch, true, true);
  clock.install();
  try {
    // A pacer that stored an absolute next-send time during startup.
    let next;
    await new Promise((resolve) => setTimeout(resolve, 6000));
    next = Date.now() + 334;
    let observed;
    setTimeout(() => { observed = Date.now(); }, 100);
    clock.start();
    assert.equal(Date.now(), epoch + 6000);
    await clock.advance(100);
    assert.equal(observed, epoch + 6100);
    await clock.advance(334);
    assert(Date.now() >= next, 'the stored deadline is reached at its tape-relative time');
  } finally { clock.restore(); }
});

test('virtual timers run on whole milliseconds, so Date.now() stays integral', async () => {
  const epoch = 1700000000000;
  const clock = new ReplayClock(epoch);
  clock.install();
  try {
    clock.start();
    let observed;
    setTimeout(() => { observed = Date.now(); }, 2125.905914739277);
    await clock.advance(3000);
    assert.equal(observed, epoch + 2125);
  } finally { clock.restore(); }
});

test('the replay epoch begins after account startup and before its first command', async () => {
  const tape = new Tape([{ type: 'command', t: 0, op: 'example' }], { isPrivate: true });
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
  assert.deepEqual(seen, ['ready', 'epoch', 'ready', 'command']);
});

test('Bybit request IDs are volatile while native client order IDs remain exact', () => {
  assert.equal(
    canonicalMexcUrl('wss://stream.bybit.com/v5/private?requestId=recorded'),
    canonicalMexcUrl('wss://stream.bybit.com/v5/private?requestId=runtime'),
  );
  assert.equal(
    canonicalMexcBody('{"requestId":"recorded","qty":"1","orderLinkId":"caller"}'),
    canonicalMexcBody('{"qty":"1","orderLinkId":"caller","requestId":"runtime"}'),
  );
  assert.notEqual(canonicalMexcBody('{"orderLinkId":"caller"}'), canonicalMexcBody('{"orderLinkId":"different"}'));
  assert.notEqual(canonicalMexcBody('{"qty":"1"}'), canonicalMexcBody('{"qty":"2"}'));
});

test('legacy Bybit sockets match generated URLs and retain latest-connection routing', { timeout: 1000 }, async () => {
  const url = 'wss://stream.bybit.com/v5/private';
  const tape = new Tape([{ type: 'ws', t: 0, url, text: 'private' }]);
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

test('MEXC normalization preserves caller IDs and economic JSON types', () => {
  const caller = new Set(['mine']);
  assert.notEqual(
    canonicalMexcBody('{"externalOid":"mine","vol":1}', caller),
    canonicalMexcBody('{"externalOid":"other","vol":1}', caller),
  );
  assert.notEqual(canonicalMexcBody('{"externalOid":"generated","vol":1}'), canonicalMexcBody('{"vol":1}'));
  assert.notEqual(canonicalMexcBody('{"vol":739113577038255617}'), canonicalMexcBody('{"vol":"739113577038255617"}'));
  assert.notEqual(canonicalMexcBody('{"price":1}'), canonicalMexcBody('{"price":"1"}'));
  assert.notEqual(canonicalMexcBody('{"id":739113577038255617}'), canonicalMexcBody('{"id":739113577038255618}'));
});

test('MEXC historical trigger windows and unrelated hosts keep absolute bounds', () => {
  for (const base of [
    'https://api.mexc.com/api/v1/private/planorder/list/orders?states=2',
    'https://api.mexc.com/api/v1/private/stoporder/list/orders?is_finished=1',
    'https://other.com/api/v1/private/planorder/list/orders?states=1',
  ])
    assert.notEqual(
      canonicalMexcUrl(`${base}&start_time=10&end_time=100`),
      canonicalMexcUrl(`${base}&start_time=110&end_time=200`),
    );
});

test('standalone transport cannot wildcard a missing recorded write body', () => {
  const tape = new Tape([{ type: 'http', method: 'POST', url, status: 200, body: 'accepted' }]);
  assert.equal(tape.respond('POST', url, '{"vol":1}').status, 599);
});

test('actual socket delivery audits authentication acknowledgement and heartbeat before callback', async () => {
  const { VENUES } = await import('./venues.mjs');
  const { createHmac } = await import('node:crypto');
  const audit = VENUES.cryptocom.createSocketAudit('rust', { isPrivate: true, books: [] });
  const url = 'wss://stream.crypto.com/exchange/v1/user';
  const nonce = Date.now();
  const auth = { method: 'public/auth', id: 1, api_key: 'replay-key', nonce };
  auth.sig = createHmac('sha256', 'replay-secret')
    .update(auth.method + auth.id + auth.api_key + nonce)
    .digest('hex');
  audit.sent(url, JSON.stringify(auth));
  const socket = {
    url,
    onMessage(text) {
      const frame = JSON.parse(text);
      if (frame.method === 'public/auth')
        audit.sent(
          url,
          JSON.stringify({
            method: 'subscribe',
            params: { channels: ['user.order', 'user.advance.order', 'user.balance', 'user.positions', 'user.trade'] },
          }),
        );
      else audit.sent(url, JSON.stringify({ method: 'public/respond-heartbeat', id: frame.id }));
    },
  };
  deliverSocketFrame(socket, '{"method":"public/auth","code":0}', audit);
  deliverSocketFrame(socket, '{"method":"public/heartbeat","id":7}', audit);
  assert.deepEqual(audit.errors(), []);
});

test('MEXC login timestamps and signatures are checked before identity normalization', async () => {
  const { VENUES } = await import('./venues.mjs');
  const { createHmac } = await import('node:crypto');
  const now = Date.now();
  const login = (reqTime, signature) =>
    JSON.stringify({
      method: 'login',
      param: {
        apiKey: 'replay-key',
        reqTime,
        signature: signature ?? createHmac('sha256', 'replay-secret').update(`replay-key${reqTime}`).digest('hex'),
      },
    });
  const socket = {};
  assert.throws(() => VENUES.mexc.validateSocketFrame(socket, '{"channel":"rs.login","data":"success"}'));
  for (const time of [undefined, 'bad', String(now - 30001), String(now + 30001)])
    assert.throws(() => VENUES.mexc.validateSocketMessage(socket, login(time), now));
  assert.throws(() => VENUES.mexc.validateSocketMessage(socket, login(String(now), 'wrong'), now));
  VENUES.mexc.validateSocketMessage(socket, login(String(now)), now);
  assert.equal(socket.loginVerified, true);
  VENUES.mexc.validateSocketFrame(socket, '{"channel":"rs.login","data":"success"}');
});

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
  assert.equal(tape.respond('POST', URL, JSON.stringify({ quantity: '1.00' })).status, 200);
  assert.equal(tape.missingWrites, 0);
  assert.equal(tape.respond('POST', URL, JSON.stringify({ quantity: '1.00' })).status, 599);
  assert.equal(tape.unserved, 1);
  assert.equal(tape.respond('GET', URL).status, 200);
  assert.equal(tape.respond('GET', URL).status, 200);
});
test('body matching preserves wire types, exact decimals and caller IDs', () => {
  assert.notEqual(canonicalBody({ quantity: '1.2300' }), canonicalBody({ quantity: '1.23' }));
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
  const now = Date.now();
  const seed = Buffer.from(venue.replayCredentials.secret, 'base64');
  const privateKey = createPrivateKey({
    key: Buffer.concat([Buffer.from('302e020100300506032b657004220420', 'hex'), seed]),
    format: 'der',
    type: 'pkcs8',
  });
  const signature = sign(
    null,
    Buffer.from(
      `instruction=orderExecute&orderType=Market&quantity=1.00&side=Bid&symbol=SOL_USDC_PERP&timestamp=${now}&window=60000`,
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
      'X-Timestamp': String(now),
      'X-Window': '60000',
    },
    body: JSON.stringify({ orderType: 'Market', quantity: '1.00', side: 'Bid', symbol: 'SOL_USDC_PERP' }),
  };
  assert.equal(venue.verifyRequest(request), true);
  assert.equal(venue.verifyRequest({ ...request, now: now + 60001 }), false);
  assert.equal(venue.verifyRequest({ ...request, now: now - 60001 }), false);
  assert.equal(venue.verifyRequest({ ...request, body: request.body.replace('1.00', '2.00') }), false);
  const auth = {
    method: 'SUBSCRIBE',
    params: ['account.orderUpdate'],
    signature: [
      venue.replayCredentials.key,
      sign(null, Buffer.from(`instruction=subscribe&timestamp=${now}&window=5000`), privateKey).toString('base64'),
      String(now),
      '5000',
    ],
  };
  assert.equal(venue.verifySubscribe(auth), true);
  assert.equal(venue.verifySubscribe(auth, now + 5001), false);
  assert.equal(venue.verifySubscribe(auth, now - 5001), false);
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

test('default websocket URL parsing is reused while socket selection stays live', async () => {
  const OriginalURL = globalThis.URL;
  let parses = 0;
  globalThis.URL = class extends OriginalURL {
    constructor(...args) { super(...args); parses++; }
  };
  try {
    for (const impl of ['ts', 'rust']) {
      parses = 0;
      const endpoint = 'wss://example.test/public?token=one';
      const received = [];
      const tape = new Tape([], {}, false, 200, impl);
      const first = tape.connect(endpoint, () => {}, (text) => received.push(`first:${text}`));
      assert.equal(await tape.socketFor(endpoint), first);
      assert.deepEqual(tape.sockets(endpoint), [first]);
      await tape.deliverFrame({ type: 'ws', url: endpoint, text: 'one' });
      tape.close(first);
      const replacement = tape.connect(endpoint, () => {}, (text) => received.push(`next:${text}`));
      assert.equal(await tape.socketFor(endpoint), replacement);
      assert.equal(replacement.generation, 2);
      await tape.deliverFrame({ type: 'ws', url: endpoint, text: 'two' });
      assert.deepEqual(received, ['first:one', 'next:two']);
      assert.equal(parses, 1);
      assert.deepEqual(tape.sockets(`${endpoint}&requestId=generated`), [replacement]);
      assert.deepEqual(tape.sockets('wss://example.test/public?token=two'), []);
      assert.deepEqual(tape.sockets('wss://example.test/private?token=one'), []);
      const independent = new Tape([], {}, false, 200, impl);
      assert.deepEqual(independent.sockets(endpoint), []);
      assert.equal(parses, 5);
    }
  } finally {
    globalThis.URL = OriginalURL;
  }
});

test('websocket URL reuse respects changing callers and custom normalization rules', async () => {
  const venue = {};
  const tape = new Tape([], venue, false, 200);
  const a = 'wss://example.test/a';
  const b = 'wss://example.test/b';
  const withField = `${a}?field=one`;
  const sockets = [a, b, withField].map((endpoint) => tape.connect(endpoint, () => {}, () => {}));
  await new Promise(setImmediate);
  assert.deepEqual(tape.sockets(a), [sockets[0]]);
  assert.deepEqual(tape.sockets(withField), [sockets[2]]);

  const callerUrl = `${a}?clientOrderId=caller`;
  assert.deepEqual(tape.sockets(callerUrl), [sockets[0]]);
  tape.callerIds.add('caller');
  assert.deepEqual(tape.sockets(callerUrl), []);
  const callerSocket = tape.connect(callerUrl, () => {}, () => {});
  await new Promise(setImmediate);
  assert.deepEqual(tape.sockets(callerUrl), [callerSocket]);
  tape.callerIds.clear();
  assert.deepEqual(tape.sockets(callerUrl), [sockets[0]]);

  let path = '/a';
  venue.requestUrl = () => `wss://example.test${path}`;
  assert.deepEqual(tape.sockets(a), [sockets[0]]);
  path = '/b';
  assert.deepEqual(tape.sockets(a), [sockets[1]]);
  delete venue.requestUrl;
  assert.deepEqual(tape.sockets(a), [sockets[0]]);

  let generated = false;
  venue.generatedQueryId = (key) => generated && key === 'field';
  assert.deepEqual(tape.sockets(withField), [sockets[2]]);
  generated = true;
  assert.deepEqual(tape.sockets(withField), [sockets[0]]);
  delete venue.generatedQueryId;
  assert.deepEqual(tape.sockets(withField), [sockets[2]]);

  venue.volatileQueryFields = [];
  assert.deepEqual(tape.sockets(withField), [sockets[2]]);
  venue.volatileQueryFields.push('field');
  assert.deepEqual(tape.sockets(withField), [sockets[0]]);
  venue.volatileQueryFields.splice(0);
  assert.deepEqual(tape.sockets(withField), [sockets[2]]);
  delete venue.volatileQueryFields;
  assert.deepEqual(tape.sockets(withField), [sockets[2]]);
});

test('websocket normalization handles mutable URL objects and repeated invalid URLs', async () => {
  const tape = new Tape([], {}, false, 200);
  const first = tape.connect('wss://example.test/a', () => {}, () => {});
  const second = tape.connect('wss://example.test/b', () => {}, () => {});
  await new Promise(setImmediate);
  const endpoint = new globalThis.URL('wss://example.test/a');
  assert.deepEqual(tape.sockets(endpoint), [first]);
  endpoint.pathname = '/b';
  assert.deepEqual(tape.sockets(endpoint), [second]);
  for (let i = 0; i < 2; i++) assert.throws(() => tape.sockets('invalid URL'), /Invalid URL/);
});

test('staged reads advance only after their command write is acknowledged and preserve response headers', () => {
  const early = { ...record('GET'), body: 'early' };
  const later = { ...record('GET'), body: 'later', afterCommand: 1, headers: 'x-stage: later' };
  const write = { ...record('POST', { quantity: '2' }), afterCommand: 1 };
  const tape = new Tape([early, later, write], {}, false);
  assert.equal(tape.respond('GET', URL).body, 'early');
  assert.equal(tape.respond('POST', URL, write.requestBody).status, 599);
  assert.equal(tape.missingWrites, 1);
  tape.commandIndex = 1;
  // A poll between command start and its acknowledgement still sees the prior state.
  assert.equal(tape.respond('GET', URL).body, 'early');
  assert.equal(tape.respond('POST', URL, write.requestBody).status, 200);
  assert.deepEqual(tape.respond('GET', URL), { status: 200, headers: 'x-stage: later', body: 'later' });
  tape.assertWritesConsumed();
});

test('an order event marked beforeAck reaches its socket once, before the client reads the acknowledgement', { timeout: 1000 }, async () => {
  const socketUrl = 'wss://example.test/private';
  const write = { ...record('POST', { quantity: '3' }), afterCommand: 1 };
  const records = [
    write,
    { type: 'command', t: 1, op: 'placeOrders' },
    { type: 'ws', t: 2, url: socketUrl, text: 'event', beforeAck: true, afterCommand: 1 },
  ];
  const tape = new Tape(records, {}, false);
  const seen = [];
  tape.connect(socketUrl, () => {}, (text) => seen.push(text));
  await tape.play(undefined, async () => {}, async () => {
    const response = tape.respond('POST', URL, write.requestBody);
    await Promise.resolve();
    seen.push(`ack:${response.status}`);
    return [];
  });
  assert.deepEqual(seen, ['event', 'ack:200']);
});

test('a staged command without an acknowledged write opens its reads once it settles', () => {
  const early = { ...record('GET'), body: 'early' };
  const later = { ...record('GET'), body: 'later', afterCommand: 1 };
  const write = { ...record('POST', { quantity: '2' }), afterCommand: 1 };
  const tape = new Tape([early, later, write], {}, false);
  tape.commandIndex = 1;
  assert.equal(tape.respond('GET', URL).body, 'early');
  tape.settledCommands = 1;
  assert.equal(tape.respond('GET', URL).body, 'later');
});

test('reads of a stage without staged writes open at command start, as before', () => {
  const early = { ...record('GET'), body: 'early' };
  const later = { ...record('GET'), body: 'later', afterCommand: 1 };
  const tape = new Tape([early, later], {}, false);
  tape.commandIndex = 1;
  assert.equal(tape.respond('GET', URL).body, 'later');
});

test('OKX generated IDs normalize without weakening caller IDs or wire types', () => {
  const venue = {
    matchRequestBody: true,
    generatedBodyId: (key, value) => key === 'clOrdId' && value.startsWith('broker'),
  };
  assert.equal(
    canonicalBody({ clOrdId: 'broker-one', sz: '1.00' }, new Set(), venue),
    canonicalBody({ clOrdId: 'broker-two', sz: '1.00' }, new Set(), venue),
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
    requestBody: (body) => (body?.method && body?.params ? body.params : body),
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
  tape.deliver(socket, 'auth-ack');
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
    {
      subscriptionGenerations: true,
      startupSubscriptions: () => ['WS wss://example.test/public? {"args":["tickers.BTC"],"op":"subscribe"}'],
    },
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
    '1 WS wss://example.test/public? {"args":["tickers.BTC"],"op":"subscribe"}',
    '2 WS wss://example.test/public? {"args":["tickers.BTC"],"op":"subscribe"}',
  ]);
  assert.equal(tape.pendingStartupSubscriptions, 0);
});

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

test('BloFin login clock must be integer milliseconds bound to its signature', () => {
  const venue = venueOf('blofin');
  const now = 1700000000000;
  const socket = { url: 'wss://openapi.blofin.com/ws/private' };
  const login = (timestamp) => JSON.stringify({ op: 'login', args: [{
    apiKey: 'replay-key', passphrase: 'replay-passphrase', timestamp, nonce: 'replay-nonce',
    sign: Buffer.from(createHmac('sha256', 'replay-secret')
      .update(`/users/self/verifyGET${timestamp}replay-nonce`).digest('hex')).toString('base64'),
  }] });
  assert.doesNotThrow(() => venue.validateSocketMessage(socket, login(now), now));
  assert.throws(() => venue.validateSocketMessage(socket, login(now + 0.5), now), /invalid BloFin socket login/);
  assert.throws(() => venue.validateSocketMessage(socket, login('1700000000000.0'), now), /invalid BloFin socket login/);
  assert.throws(() => venue.validateSocketMessage(socket, login(now).replace('replay-nonce', 'changed'), now), /signature/);
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

test('Hyperliquid legacy shared-URL frames reach both public and private listeners once', async () => {
  const url = 'wss://api.hyperliquid.xyz/ws';
  const tape = new Tape(
    [{ type: 'ws', t: 0, url, text: '{"channel":"webData2","data":{}}' }],
    venueOf('hyperliquid'),
    true,
  );
  const seen = [];
  tape.connect(
    url,
    () => {},
    () => seen.push('public'),
  );
  tape.connect(
    url,
    () => {},
    () => seen.push('private'),
  );
  await new Promise(setImmediate);
  assert.equal(await tape.play(), 1);
  assert.deepEqual(seen, ['public', 'private']);
});

test('Bybit startup waits for every actual topic and retains its expected map after replay', async () => {
  const url = 'wss://stream.bybit.com/v5/public/linear';
  const expected = [
    'WS wss://stream.bybit.com/v5/public/linear? {"args":["tickers.BTCUSDT"],"op":"subscribe"}',
    'WS wss://stream.bybit.com/v5/public/linear? {"args":["orderbook.1000.BTCUSDT"],"op":"subscribe"}',
  ];
  const tape = new Tape(
    [{ type: 'ws', t: 0, url, text: 'book' }],
    { startupSubscriptions: () => expected },
    false,
    200,
  );
  const map = tape.startupSubscriptions;
  const seen = [];
  const socket = tape.connect(
    url,
    () => {},
    () => seen.push('delivered'),
  );
  tape.send(socket, '{"op":"subscribe","args":["tickers.BTCUSDT"]}');
  const playback = tape.play();
  await new Promise(setImmediate);
  assert.deepEqual(seen, []);
  assert.equal(tape.pendingStartupSubscriptions, 1);
  tape.send(socket, '{"op":"subscribe","args":["orderbook.1000.BTCUSDT"]}');
  await playback;
  assert.deepEqual(seen, ['delivered']);
  assert.equal(tape.startupSubscriptions, map);
  assert.equal(map.size, 2);
  assert.deepEqual([...map.values()], [true, true]);
});

test('fixture TS body exceptions preserve common write sequence without relaxing native or TS bodies', () => {
  const row = {
    ...record('POST', { quantity: '2', stopLossTriggerPrice: '120' }),
    tsRequestBody: '{"quantity":"2","stopLossTriggerPrice":"120","stopLossLimitPrice":"120"}',
  };
  const rust = new Tape([row], {}, false, 200, 'rust');
  const ts = new Tape([row], {}, false, 200, 'ts');
  assert.equal(rust.respond('POST', URL, row.requestBody).status, 200);
  assert.equal(ts.respond('POST', URL, row.tsRequestBody).status, 200);
  assert.deepEqual(ts.writes, rust.writes);
  assert.deepEqual([...ts.requests], [...rust.requests]);
  assert.equal(ts.respond('POST', URL, row.requestBody).status, 599);
  assert.equal(rust.respond('POST', URL, row.tsRequestBody).status, 599);
});

test('caught Backpack verifier exceptions still fail the final replay check', async () => {
  const venue = venueOf('backpack');
  for (const transport of ['REST', 'WS']) {
    const tape = new Tape([], venue, true);
    assert.equal(tape.missingWrites, 0);
    if (transport === 'REST') {
      assert.throws(
        () => tape.respond('GET', 'https://api.backpack.exchange/api/v1/account', undefined, {}),
        /signature/,
      );
    } else {
      const socket = tape.connect(
        'wss://ws.backpack.exchange',
        () => {},
        () => {},
      );
      assert.throws(
        () => tape.send(socket, '{"method":"SUBSCRIBE","params":["account.orderUpdate"],"signature":[]}'),
        /signature/,
      );
      await new Promise(setImmediate);
    }
    assert.equal(tape.invalidSignatures, 1);
    // Even when the application swallows the failed read/send, loaded state
    // and zero outstanding mutations cannot turn that replay into a pass.
    assert.throws(() => tape.assertWritesConsumed(), /1 invalid replay signature/);
  }
});

test('caught throwing signature verifiers persist exactly one final audit failure', async () => {
  const broken = () => {
    throw new SyntaxError('malformed signed payload');
  };
  for (const kind of ['REST', 'WS']) {
    const tape = new Tape([], { verifyRequest: broken, verifySubscribe: broken }, true);
    if (kind === 'REST') {
      assert.throws(() => tape.respond('POST', URL, '{bad signed json', {}), /malformed signed payload/);
    } else {
      const socket = tape.connect(
        'wss://ws.backpack.exchange',
        () => {},
        () => {},
      );
      assert.throws(() => tape.send(socket, '{"method":"SUBSCRIBE","params":[{}]}'), /malformed signed payload/);
      await new Promise(setImmediate);
    }
    assert.equal(tape.invalidSignatures, 1);
    assert.equal(tape.missingWrites, 0);
    assert.throws(() => tape.assertWritesConsumed(), /1 invalid replay signature/);
  }
});

const bitgetVenue = { rust: 'bitget' };
const bitgetUrl = 'https://api.bitget.com/api/v2/mix/order/modify-order';
const bitgetResponse = (body, overrides = {}) => ({
  type: 'http',
  method: 'POST',
  url: bitgetUrl,
  requestBody: body,
  status: 200,
  body: '{"code":"00000","data":{"orderId":"2"}}',
  ...overrides,
});

test('mutation replies are consumed once; omitted writes remain visible', () => {
  const tape = createTape([bitgetResponse({ orderId: '1', newClientOid: 'expected' })], bitgetVenue, true);
  assert.equal(tape.unusedWrites(), 1);
  assert.equal(tape.respond('POST', bitgetUrl, { orderId: '1', newClientOid: 'fresh' }).status, 200);
  assert.equal(tape.unusedWrites(), 0);
  assert.equal(tape.respond('POST', bitgetUrl, { orderId: '1', newClientOid: 'fresh' }).status, 599);
  assert.equal(tape.unserved, 1);
});

test('generated IDs preserve field presence, type, and freshness', () => {
  for (const body of [{ orderId: '1' }, { orderId: '1', newClientOid: null }, { orderId: '1', newClientOid: '' }]) {
    const tape = createTape([bitgetResponse({ orderId: '1', newClientOid: 'expected' })], bitgetVenue, true);
    assert.equal(tape.respond('POST', bitgetUrl, body).status, 599);
  }
  const tape = createTape(
    [bitgetResponse({ orderId: '1', newClientOid: 'a' }), bitgetResponse({ orderId: '2', newClientOid: 'b' })],
    bitgetVenue,
    true,
  );
  tape.respond('POST', bitgetUrl, { orderId: '1', newClientOid: 'fresh' });
  assert.throws(() => tape.respond('POST', bitgetUrl, { orderId: '2', newClientOid: 'fresh' }), /reused/);
});

test('caller IDs compare verbatim and Classic margin spelling exception is TS-only', () => {
  const place = 'https://api.bitget.com/api/v3/trade/place-order';
  const records = [
    { type: 'command', orders: [{ clientOrderId: 'caller' }] },
    bitgetResponse({ clientOid: 'caller' }, { url: place }),
  ];
  assert.equal(createTape(records, bitgetVenue, true).respond('POST', place, { clientOid: 'wrong' }).status, 599);
  assert.equal(createTape(records, bitgetVenue, true).respond('POST', place, { clientOid: 'caller' }).status, 200);
  assert.equal(
    createTape([bitgetResponse({ marginMode: 'crossed' }, { url: place })], bitgetVenue, true).respond('POST', place, {
      marginMode: 'cross',
    }).status,
    599,
  );
  const classic = 'https://api.bitget.com/api/v2/mix/order/batch-place-order';
  assert.equal(
    createTape([bitgetResponse({ marginMode: 'crossed' }, { url: classic })], bitgetVenue, true).respond(
      'POST',
      classic,
      { marginMode: 'cross' },
    ).status,
    599,
  );
  assert.equal(
    createTape(
      [{ ...bitgetResponse({ marginMode: 'crossed' }, { url: classic }), tsRequestBody: { marginMode: 'cross' } }],
      bitgetVenue,
      true,
      { impl: 'ts' },
    ).respond('POST', classic, {
      marginMode: 'cross',
    }).status,
    200,
  );
});

test('signature rejection permits exactly the taped same-ID retry; reads can poll', () => {
  const body = { orderId: '1', newClientOid: 'fresh' };
  const tape = createTape(
    [bitgetResponse(body, { body: '{"code":"40009"}' }), bitgetResponse(body)],
    bitgetVenue,
    true,
  );
  assert.equal(tape.respond('POST', bitgetUrl, body).status, 200);
  assert.equal(tape.respond('POST', bitgetUrl, body).status, 200);
  const reads = createTape([bitgetResponse(undefined, { method: 'GET' })], bitgetVenue, false);
  assert.equal(reads.respond('GET', bitgetUrl).status, 200);
  assert.equal(reads.respond('GET', bitgetUrl).status, 200);
});

test('same-URL sockets receive only exact product/topic subscriptions', async () => {
  const tape = createTape([], bitgetVenue, false);
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
  const tape = createTape([bitgetResponse({ clientOid: 'resting-client' }, { url: cancel })], bitgetVenue, true);
  assert.equal(tape.respond('POST', cancel, { clientOid: 'wrong-order' }).status, 599);
  assert.equal(tape.unusedWrites(), 1);
  assert.equal(tape.respond('POST', cancel, { clientOid: 'resting-client' }).status, 200);
  const place = 'https://api.bitget.com/api/v3/trade/place-order';
  const linked = createTape(
    [
      bitgetResponse({ clientOid: 'taped-minted' }, { url: place }),
      bitgetResponse({ clientOid: 'taped-minted' }, { url: cancel }),
    ],
    bitgetVenue,
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
      bitgetResponse({ clientOid: 'taped-old' }, { url: place }),
      bitgetResponse({ clientOid: 'taped-old', newClientOid: 'taped-new' }),
    ],
    bitgetVenue,
    true,
  );
  assert.equal(tape.respond('POST', place, { clientOid: 'actual-old' }).status, 200);
  assert.equal(tape.respond('POST', bitgetUrl, { clientOid: 'actual-old', newClientOid: 'actual-new' }).status, 200);
});

test('Bitget callback validation fails before consuming a matched mutation', () => {
  const tape = createTape([bitgetResponse({ orderId: '1', newClientOid: 'recorded' })], bitgetVenue, true);
  assert.throws(
    () => tape.respond('POST', bitgetUrl, { orderId: '1', newClientOid: 'invalid id' }),
    /invalid generated/,
  );
  assert.equal(tape.unusedWrites(), 1);
  assert.equal(tape.respond('POST', bitgetUrl, { orderId: '1', newClientOid: 'fresh' }).status, 200);
  assert.equal(tape.unusedWrites(), 0);
});

test('Bitget duplicate generated IDs inside one batch fail before response consumption', () => {
  const url = 'https://api.bitget.com/api/v3/trade/place-order';
  const tape = createTape(
    [bitgetResponse([{ clientOid: 'first' }, { clientOid: 'second' }], { url })],
    bitgetVenue,
    true,
  );
  assert.throws(() => tape.respond('POST', url, [{ clientOid: 'reused' }, { clientOid: 'reused' }]), /reused/);
  assert.equal(tape.unusedWrites(), 1);
  assert.equal(tape.respond('POST', url, [{ clientOid: 'fresh-one' }, { clientOid: 'fresh-two' }]).status, 200);
});

test('Backpack cancellation client IDs stay exact while only valid creation IDs can vary', () => {
  const venue = venueOf('backpack');
  const cancel = 'https://api.backpack.exchange/api/v1/order';
  const row = { type: 'http', method: 'DELETE', url: cancel, requestBody: '{"clientId":42}', status: 200, body: '{}' };
  const tape = new Tape([row], venue, false);
  assert.equal(tape.respond('DELETE', cancel, '{"clientId":43}').status, 599);
  assert.equal(tape.respond('DELETE', cancel, '{"clientId":"42"}').status, 599);
  assert.equal(tape.respond('DELETE', cancel, '{"clientId":42}').status, 200);
});

test('replay readiness waits for the first recorded socket before deriving subscriptions', async () => {
  const url = 'wss://stream.crypto.com/exchange/v1/market';
  const tape = new Tape([{ type: 'ws', t: 0, url, socket: 0, text: 'ticker' }], {}, false, 200);
  const seen = [];
  let marketsReady = false;
  const played = tape.play(undefined, async () => seen.push(`ready:${marketsReady}`));
  await new Promise(setImmediate);
  assert.deepEqual(seen, []);
  // The production venue commits its seed before opening this socket.
  marketsReady = true;
  tape.connect(url, () => seen.push('opened'), () => seen.push('frame'));
  assert.equal(await played, 1);
  assert.deepEqual(seen, ['opened', 'ready:true', 'frame']);
});

for (const delayedAuth of [false, true]) test(`first-socket startup fence preserves authenticated Phemex bootstrap before readiness (${delayedAuth ? 'delayed auth' : 'immediate auth'})`, async () => {
  const url = 'wss://ws.phemex.com';
  const authAck = JSON.stringify({ id: 999, error: null, result: { status: 'success' } });
  const snapshot = JSON.stringify({ type: 'snapshot', orders_p: [] });
  const ticker = JSON.stringify({ method: 'perp_market24h_pack_p.update', data: [] });
  const tape = new Tape([
    { type: 'ws', t: 0, url, bootstrap: true, text: authAck },
    { type: 'ws', t: 0, url, bootstrap: true, text: snapshot },
    { type: 'ws', t: 0, url, text: ticker },
  ], venueOf('phemex'), true, 200, 'rust');
  const seen = [];
  const publicSocket = tape.connect(url,
    () => tape.send(publicSocket, JSON.stringify({ method: 'perp_market24h_pack_p.subscribe', params: [] })),
    () => seen.push('public-ticker'),
  );
  const privateSocket = tape.connect(url, () => {
    const authenticate = () => {
      const expiry = Math.floor(Date.now() / 1000 + 60);
      const signature = createHmac('sha256', 'replay-secret').update('replay-key' + expiry).digest('hex');
      tape.send(privateSocket, JSON.stringify({ id: 999, method: 'user.auth', params: ['API', 'replay-key', signature, expiry] }));
    };
    // Opening notification occurs first; bootstrap must wake when auth later
    // identifies the private socket, even without stream-routing venue flags.
    if (delayedAuth) setTimeout(authenticate, 10);
    else authenticate();
  }, (text) => {
    if (JSON.parse(text).id === 999) {
      seen.push('private-auth-ack');
      tape.send(privateSocket, JSON.stringify({ method: 'aop_p.subscribe', params: [] }));
    } else seen.push('private-snapshot');
  });
  assert.equal(await tape.play(undefined, async () => {
    assert.deepEqual(seen, ['private-auth-ack', 'private-snapshot']);
    assert.equal(tape.audit.ready(), true);
    seen.push('ready');
  }), 1);
  assert.deepEqual(seen, ['private-auth-ack', 'private-snapshot', 'ready', 'public-ticker']);
  assert.deepEqual(tape.audit.errors(), []);
});

test('shared public/private routing rejects ambiguous closes and keeps physical socket ordinals', async () => {
  const url = 'wss://example.test/shared';
  const seen = [];
  const tape = new Tape([], {
    socketMatches: (socket, text) => Boolean(socket.private) === JSON.parse(text).private,
    onSocketSend: (socket, text) => { socket.private = JSON.parse(text).private; },
  }, true, 200);
  const publicSocket = tape.connect(url, () => {}, () => seen.push('public'), (code) => seen.push(`closed-public:${code}`));
  const privateSocket = tape.connect(url, () => {}, () => seen.push('private'));
  tape.send(privateSocket, '{"private":true}');
  await new Promise(setImmediate);
  await tape.deliverFrame({ type: 'ws', url, text: '{"private":false}' });
  await tape.deliverFrame({ type: 'ws', url, text: '{"private":true}' });
  await assert.rejects(tape.deliverFrame({ type: 'close', url }), /Ambiguous close/);
  assert.equal(publicSocket.closed, false);
  assert.equal(privateSocket.closed, false);
  await tape.deliverFrame({ type: 'close', url, socket: 0, code: 1006 });
  const replacement = tape.connect(url, () => {}, () => seen.push('replacement'));
  await new Promise(setImmediate);
  assert.deepEqual([publicSocket.ordinal, privateSocket.ordinal, replacement.ordinal], [0, 1, 2]);
  await tape.deliverFrame({ type: 'ws', url, socket: 1, text: '{"private":true}' });
  await tape.deliverFrame({ type: 'ws', url, socket: 2, text: '{"private":false}' });
  assert.deepEqual(seen, ['public', 'private', 'closed-public:1006', 'private', 'replacement']);
});
test('venue socket keys preserve bullet tokens while normalizing generated connection IDs', async () => {
  const venue = { volatileQueryFields: ['connectId'] };
  const urlKey = (value) => canonicalUrl(value, new Set(), venue);
  const tape = new Tape([], venue);
  let received;
  tape.connect(
    'wss://bullet.test/socket?token=public&connectId=runtime',
    () => {},
    (text) => {
      received = text;
    },
  );
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
  tape.connect(
    url,
    () => observed.push('open'),
    (text) => observed.push(JSON.parse(text).type),
  );
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
  for (const rows of [
    [],
    [
      { type: 'ws', t: 1, url, text: '{"type":"message"}' },
      { type: 'ws', t: 2, url, text: '{"type":"welcome"}' },
    ],
  ]) {
    const tape = new Tape(rows, { startupMessage: (r) => JSON.parse(r.text).type === 'welcome' });
    const observed = [];
    tape.connect(
      url,
      () => {},
      (text) => observed.push(text),
    );
    await new Promise(setImmediate);
    assert.deepEqual(observed, []);
  }
});

test('KuCoin shared Tape binds signed headers, caller IDs, body types and response stages', () => {
  const venue = venueOf('kucoin');
  const url = 'https://api-futures.kucoin.com/api/v1/orders';
  const body = JSON.stringify({ symbol: 'XBTUSDTM', clientOid: 'caller-exact', size: '2' });
  const tape = new Tape(
    [
      { type: 'command', op: 'placeOrders', orders: [{ clientOrderId: 'caller-exact' }] },
      { type: 'http', method: 'POST', url, requestBody: body, status: 200, body: '{}', availableAfter: 10 },
    ],
    venue,
    true,
  );
  const headers = (payload) => {
    const timestamp = String(Date.now());
    const hmac = (value) => createHmac('sha256', 'replay-secret').update(value).digest('base64');
    return {
      'KC-API-KEY': 'replay-key',
      'KC-API-KEY-VERSION': '2',
      'KC-API-TIMESTAMP': timestamp,
      'KC-API-PASSPHRASE': hmac('replay-passphrase'),
      'KC-API-SIGN': hmac(timestamp + 'POST/api/v1/orders' + (payload ?? '')),
    };
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
    {
      type: 'http',
      method: 'POST',
      url: 'https://api-futures.kucoin.com/api/v1/bullet-public',
      status: 200,
      body: JSON.stringify({ code: '200000', data: { token: 'public-token', instanceServers: [{ endpoint }] } }),
    },
    { type: 'ws', t: 1, url, socket: 0, text: '{"type":"welcome"}' },
    { type: 'ws', t: 2, url, socket: 0, text: JSON.stringify({ type: 'message', topic }) },
  ];
  const tape = new Tape(records, venue, false, 500, 'rust');
  const observed = [];
  const socket = tape.connect(
    endpoint + '?token=public-token&connectId=runtime',
    () => {},
    (text) => {
      const message = JSON.parse(text);
      observed.push(message);
      if (message.type === 'welcome') {
        tape.send(socket, JSON.stringify({ type: 'subscribe', topic, privateChannel: false }));
        tape.send(socket, '{"type":"ping","id":"heartbeat"}');
      }
    },
  );
  const ticks = [];
  const frames = await tape.play((t) => ticks.push(t));
  await new Promise(setImmediate);
  assert.equal(frames, 2);
  assert.deepEqual(ticks, [1, 2]);
  assert.equal(observed.filter((m) => m.type === 'welcome').length, 1);
  assert(observed.some((m) => m.type === 'pong' && m.id === 'heartbeat'));
  assert([...tape.protocolSubscriptions].some((key) => key.includes('\"topic\":\"' + topic + '\"')));
  assert.deepEqual(tape.audit.errors(), []);
});

test('startup welcomes match socket ordinals and cannot bypass recorded closes', async () => {
  const url = 'wss://bullet.test/socket?token=shared';
  const venue = { startupMessage: (r) => JSON.parse(r.text).type === 'welcome' };
  const tape = new Tape([{ type: 'ws', t: 1, url, socket: 1, text: '{"type":"welcome"}' }], venue);
  const seen = [];
  tape.connect(
    url,
    () => {},
    () => seen.push(0),
  );
  await new Promise(setImmediate);
  assert.deepEqual(seen, []);
  tape.connect(
    url,
    () => {},
    () => seen.push(1),
  );
  await new Promise(setImmediate);
  assert.deepEqual(seen, [1]);
  const closed = new Tape(
    [
      { type: 'close', t: 0, url, socket: 0 },
      { type: 'ws', t: 1, url, socket: 0, text: '{"type":"welcome"}' },
    ],
    venue,
  );
  closed.connect(
    url,
    () => {},
    () => assert.fail('welcome bypassed close'),
  );
  await new Promise(setImmediate);
});

test('factory keeps command stage, response time, headers and generated-ID validation independent', () => {
  const url = 'https://api.bitget.com/api/v2/mix/order/modify-order';
  const response = (requestBody, extras) => ({
    type: 'http',
    method: 'POST',
    url,
    requestBody,
    status: 200,
    body: '{\"code\":\"00000\"}',
    ...extras,
  });
  const stagedVenue = {
    rust: 'bitget',
    checkRequest: (_method, _url, _body, headers) => headers?.auth === 'fixture',
  };
  const tape = createTape(
    [response({ orderId: '1', newClientOid: 'expected' }, { afterCommand: 2, availableAfter: 100 })],
    stagedVenue,
    true,
  );
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
  assert.equal(tape.unusedWrites(), 1);
  assert.equal(tape.respond('POST', url, body, headers).status, 200);
  assert.equal(tape.unusedWrites(), 0);
  assert.equal(tape.respond('POST', url, body, headers).status, 599);
});

test('missing recorded welcome keeps readiness bounded and wrong tokens never match', async () => {
  const url = 'wss://bullet.test/socket?token=public';
  const venue = { startupMessage: (record) => JSON.parse(record.text).type === 'welcome' };
  const tape = new Tape([], venue, false, 25);
  tape.connect(
    url,
    () => {},
    () => assert.fail('invented welcome'),
  );
  await assert.rejects(
    tape.play(undefined, () => new Promise(() => {})),
    /Replay startup/,
  );
  await assert.rejects(tape.socketFor(url.replace('public', 'private')), /socket/);
});

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
  test('extracted Tape scopes both HTTP stores and write ordering to the selected implementation', () => {
    const records = [
      { ...record('POST', { family: 'shared' }) },
      { ...record('POST', { family: 'legacy' }), impl: 'ts' },
      { ...record('POST', { family: 'Order' }), impl: 'rust' },
      { ...record('POST', { family: 'StopOrder' }), impl: 'rust' },
    ];
    for (const typed of [false, true]) {
      const venue = { compareWriteOrder: true, ...(typed ? { requestBodies: true } : {}) };
      for (const impl of ['ts', 'rust']) {
        const tape = new Tape(records, venue, true, 100, impl);
        assert.equal(tape.missingWrites, impl === 'ts' ? 2 : 3);
        assert.equal(tape.respond('POST', URL, { family: impl === 'ts' ? 'Order' : 'legacy' }).status, 599);
        assert.equal(tape.respond('POST', URL, { family: 'shared' }).status, 200);
        for (const family of impl === 'ts' ? ['legacy'] : ['Order', 'StopOrder']) {
          assert.equal(tape.respond('POST', URL, { family }).status, 200);
          assert.equal(tape.respond('POST', URL, { family }).status, 599);
        }
        assert.equal(tape.missingWrites, 0);
        tape.assertWritesConsumed();
      }
      assert.throws(() => new Tape(records, venue), /require a replay implementation/);
      assert.throws(() => new Tape(records, venue, false, 100, 'typo'), /Unknown replay implementation/);
      assert.throws(
        () => new Tape([{ ...records[0], impl: 'typo' }], venue, false, 100, 'rust'),
        /Unknown tape record/,
      );
    }
  });

  test('Bybit typed Tape matching retains exact types, category and caller identity', () => {
    const venue = venueOf('bybitspot');
    const url = 'https://api.bybit.com/v5/order/create';
    const requestBody = { category: 'spot', qty: '1', orderLinkId: 'caller' };
    const tape = new Tape(
      [
        { type: 'command', t: 0, op: 'placeOrders', orders: [{ clientOrderId: 'caller' }] },
        { ...record('POST', requestBody), url },
      ],
      venue,
      true,
      100,
      'rust',
    );
    for (const body of [
      { ...requestBody, qty: 1 },
      { ...requestBody, category: 'linear' },
      { ...requestBody, orderLinkId: 'other' },
    ])
      assert.equal(tape.respond('POST', url, body).status, 599);
    assert.equal(tape.respond('POST', url, requestBody).status, 200);
    assert.equal(tape.missingWrites, 0);
  });

  test('Hyro taped handshakes bootstrap before full account readiness and echo each connection request', async () => {
    const url = 'wss://hyro.example/v5/private';
    const records = [
      { type: 'ws', t: 0, url, text: '{"op":"auth","req_id":"@latest","success":true}' },
      { type: 'command', t: 0, op: 'write' },
    ];
    const tape = new Tape(records, { bootstrapPrivateFromTape: true }, true, 100);
    let authenticated = false;
    const seen = [];
    const socket = tape.connect(
      url,
      () => {
        setImmediate(() => tape.send(socket, '{"op":"auth","req_id":"connection-local"}'));
      },
      (text) => {
        assert.equal(JSON.parse(text).req_id, 'connection-local');
        authenticated = true;
      },
    );
    await tape.play(
      undefined,
      async (accountReady) => {
        seen.push(accountReady);
        if (accountReady) assert.equal(authenticated, true);
      },
      async () => {
        assert.equal(authenticated, true);
        seen.push('command');
        return [];
      },
    );
    assert.deepEqual(seen, [false, true, 'command']);
    const missing = new Tape(records.slice(0, 1), { bootstrapPrivateFromTape: true }, true, 20);
    missing.connect(
      url,
      () => {},
      () => assert.fail('unsolicited ack was delivered'),
    );
    await assert.rejects(missing.play(), /auth request for acknowledgement/);
  });
}

test('caught WOO request validation failures remain fatal after a valid retry', () => {
  const venue = venueOf('woo');
  const url = 'https://api.woox.io/v3/accountinfo';
  const timestamp = Date.now();
  const headers = {
    'x-api-key': 'replay-key',
    'x-api-timestamp': timestamp,
    'x-api-signature': createHmac('sha256', 'replay-secret').update(`${timestamp}GET/v3/accountinfo`).digest('hex'),
  };
  const tape = new Tape([{ type: 'http', method: 'GET', url, status: 200, body: '{"success":true}' }], venue, true);
  assert.throws(() => tape.respond('GET', url, undefined, { ...headers, 'x-api-signature': 'bad' }), /signature/);
  assert.equal(tape.respond('GET', url, undefined, headers).status, 200);
  assert.equal(tape.unserved, 0);
  assert.equal(tape.invalidSignatures, 1);
  assert.throws(() => tape.assertWritesConsumed(), /1 invalid replay signature/);
});

test('Coinbase same-URL sockets retain exact channel and product scope across batches and unsubscribe', async () => {
  const url = 'wss://advanced-trade-ws.coinbase.com';
  const tape = new Tape([], VENUES.coinbase, false, 20);
  const receivedA = [],
    receivedB = [];
  const a = tape.connect(
    url,
    () => {},
    (text) => receivedA.push(JSON.parse(text)),
  );
  const b = tape.connect(
    url,
    () => {},
    (text) => receivedB.push(JSON.parse(text)),
  );
  await new Promise(setImmediate);
  const send = (socket, channel, product_ids, type = 'subscribe') =>
    tape.send(socket, JSON.stringify({ type, channel, product_ids }));
  send(a, 'level2', ['BTC-USD', 'ETH-USD']);
  send(a, 'heartbeats', []);
  send(b, 'level2', ['SOL-USD']);
  send(b, 'candles', ['ETH-USD']);
  const frame = (channel, product) => ({
    url,
    text: JSON.stringify({
      channel,
      sequence_num: 0,
      events: [{ type: 'snapshot', product_id: product, updates: [] }],
    }),
  });
  for (const product of ['BTC-USD', 'ETH-USD', 'SOL-USD']) {
    const record = frame('l2_data', product);
    const socket = await tape.socketFor(url, record.text);
    assert.equal(socket, product === 'SOL-USD' ? b : a);
    tape.deliver(socket, record.text);
  }
  const candle = frame('candles', 'ETH-USD');
  const candleSocket = await tape.socketFor(url, candle.text);
  assert.equal(candleSocket, b);
  tape.deliver(candleSocket, candle.text);
  assert.deepEqual(
    receivedA.map((r) => r.events[0].product_id),
    ['BTC-USD', 'ETH-USD'],
  );
  assert.deepEqual(
    receivedB.map((r) => [r.channel, r.events[0].product_id]),
    [
      ['l2_data', 'SOL-USD'],
      ['candles', 'ETH-USD'],
    ],
  );
  send(a, 'level2', ['ETH-USD'], 'unsubscribe');
  assert.equal(await tape.socketFor(url, frame('l2_data', 'BTC-USD').text), a);
  assert.equal(await tape.socketFor(url, candle.text), b);
  await assert.rejects(tape.socketFor(url, frame('l2_data', 'ETH-USD').text), /replay timeout/);
  tape.close(a);
  await assert.rejects(tape.socketFor(url, frame('l2_data', 'BTC-USD').text), /replay timeout/);
  tape.close(b);
});

test('implementation tags independently scope websocket requests and replies', () => {
  const records = ['ts', 'rust'].flatMap((impl) => [
    { impl, type: 'ws-send', url: 'wss://example.test/ws', text: JSON.stringify({id: impl, method: impl}) },
    { impl, type: 'ws', t: 1, url: 'wss://example.test/ws', text: JSON.stringify({id: impl, status: 200}) },
  ]);
  const venue = { socketRequest: (message) => ({id: message.id, key: message.method}) };
  for (const impl of ['ts', 'rust']) {
    const tape = new Tape(records, venue, true, 100, impl);
    assert.equal(tape.frames.length, 1);
    assert.equal(JSON.parse(tape.frames[0].text).id, impl);
    assert.deepEqual([...tape.recordedSocketRequests.values()], [impl]);
  }
  assert.throws(() => new Tape(records, venue), /require a replay implementation/);
  assert.throws(() => new Tape([{...records[0], impl:'typo'}], venue, true, 100, 'rust'), /Unknown tape record/);
});

test('wait markers advance virtual timers and onTick without sockets or commands, including Bitget', async () => {
  const clock = new ReplayClock(1000);
  clock.install();
  try {
    const tape = new Tape([{ type: 'wait', t: 50, url: 'wss://must-not-connect.test' }], { rust: 'bitget' });
    tape.socketFor = () => assert.fail('wait performed a socket lookup');
    tape.deliverFrame = () => assert.fail('wait delivered a Bitget frame');
    tape.respond = () => assert.fail('wait performed an HTTP request');
    const observed = [];
    const count = await tape.play(
      (time) => observed.push(['tick', time]),
      undefined,
      () => assert.fail('wait executed a command'),
      clock,
      () => setTimeout(() => observed.push(['timer']), 10),
    );
    assert.equal(count, 1);
    assert.equal(tape.time, 50);
    assert.equal(Date.now(), 1050);
    assert.deepEqual(observed, [['timer'], ['tick', 50]]);
    assert.equal(tape.commandIndex, 0);
    assert.deepEqual(tape.results, []);
  } finally {
    clock.restore();
  }
});

test('paced waits observe real timer work without socket traffic or extra commands', async () => {
  const tape = new Tape([{ type: 'wait', t: 30 }], {}, true);
  tape.socketFor = () => assert.fail('wait performed a socket lookup');
  tape.respond = () => assert.fail('wait performed an HTTP request');
  let fired = false;
  const ticks = [];
  const before = performance.now();
  const count = await tape.play(
    (time) => ticks.push(time),
    undefined,
    () => assert.fail('wait executed a command'),
    undefined,
    () =>
      setTimeout(() => {
        fired = true;
      }, 5),
  );
  assert(performance.now() - before >= 25);
  assert.equal(fired, true);
  assert.equal(count, 1);
  assert.deepEqual(ticks, [30]);
  assert.equal(tape.commandIndex, 0);
});

test('wait markers reject absent, negative and nonfinite observation times', () => {
  for (const time of [undefined, -1, NaN, Infinity, '30'])
    assert.throws(() => new Tape([{ type: 'wait', t: time }]), /Invalid replay wait timestamp/);
});

for (const connectionOrder of [['public', 'private'], ['private', 'public']]) {
  test(`MEXC recorded socket roles preserve authenticated delivery when ${connectionOrder[0]} connects first`, async () => {
    const endpoint = 'wss://contract.mexc.com/edge';
    const tape = new Tape([], VENUES.mexc, true, 1000);
    const seen = [];
    for (const role of connectionOrder) {
      const socket = tape.connect(endpoint, () => {
        const reqTime = String(Date.now());
        tape.send(socket, JSON.stringify(role === 'private' ? {
          method: 'login', param: {
            apiKey: 'replay-key', reqTime,
            signature: createHmac('sha256', 'replay-secret').update(`replay-key${reqTime}`).digest('hex'),
          },
        } : { method: 'sub.ticker', param: { symbol: 'BTC_USDT' } }));
      }, (text) => seen.push([role, JSON.parse(text).channel]));
    }
    await new Promise(setImmediate);
    for (const [socket, channel, data] of [
      [1, 'rs.login', 'success'], [1, 'push.personal.order', { orderId: '739113577038255618' }],
      [0, 'push.ticker', { symbol: 'BTC_USDT', lastPrice: 60000 }], [1, 'pong', {}],
    ]) {
      await tape.deliverFrame({ type: 'ws', url: endpoint, socket, text: JSON.stringify({ channel, data }) });
    }
    assert.deepEqual(seen, [
      ['private', 'rs.login'], ['private', 'push.personal.order'], ['public', 'push.ticker'], ['private', 'pong'],
    ]);
    assert.equal(tape.invalidSignatures, 0);
  });
}
