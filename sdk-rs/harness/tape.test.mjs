import assert from 'node:assert/strict';
import test from 'node:test';
import { bodyBytes, canonicalBody, canonicalUrl, deliverSocketFrame, replayStoreReady, ReplayClock, Tape } from './tape.mjs';

const url = 'https://api.mexc.com/api/v1/private/order/cancel';
test('POST matching preserves snowflakes and consumes every accepted write once', () => {
  const tape = new Tape([
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
  assert.notEqual(canonicalBody('{"externalOid":"mine","vol":1}', caller), canonicalBody('{"externalOid":"other","vol":1}', caller));
  assert.notEqual(canonicalBody('{"externalOid":"generated","vol":1}'), canonicalBody('{"vol":1}'));
  assert.notEqual(canonicalBody('{"vol":739113577038255617}'), canonicalBody('{"vol":"739113577038255617"}'));
  assert.notEqual(canonicalBody('{"price":1}'), canonicalBody('{"price":"1"}'));
  assert.notEqual(canonicalBody('{"id":739113577038255617}'), canonicalBody('{"id":739113577038255618}'));
});

test('MEXC historical trigger windows and unrelated hosts keep absolute bounds', () => {
  for (const base of [
    'https://api.mexc.com/api/v1/private/planorder/list/orders?states=2',
    'https://api.mexc.com/api/v1/private/stoporder/list/orders?is_finished=1',
    'https://other.com/api/v1/private/planorder/list/orders?states=1',
  ]) assert.notEqual(canonicalUrl(`${base}&start_time=10&end_time=100`), canonicalUrl(`${base}&start_time=110&end_time=200`));
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
  auth.sig = createHmac('sha256', 'replay-secret').update(auth.method + auth.id + auth.api_key + nonce).digest('hex');
  audit.sent(url, JSON.stringify(auth));
  const socket = { url, onMessage(text) {
    const frame = JSON.parse(text);
    if (frame.method === 'public/auth') audit.sent(url, JSON.stringify({ method: 'subscribe', params: { channels: ['user.order', 'user.advance.order', 'user.balance', 'user.positions', 'user.trade'] } }));
    else audit.sent(url, JSON.stringify({ method: 'public/respond-heartbeat', id: frame.id }));
  } };
  deliverSocketFrame(socket, '{"method":"public/auth","code":0}', audit);
  deliverSocketFrame(socket, '{"method":"public/heartbeat","id":7}', audit);
  assert.deepEqual(audit.errors(), []);
});


test('MEXC login timestamps and signatures are checked before identity normalization', async () => {
  const { VENUES } = await import('./venues.mjs');
  const { createHmac } = await import('node:crypto');
  const now = Date.now();
  const login = (reqTime, signature) => JSON.stringify({ method: 'login', param: {
    apiKey: 'replay-key', reqTime,
    signature: signature ?? createHmac('sha256', 'replay-secret').update(`replay-key${reqTime}`).digest('hex'),
  } });
  const socket = {};
  assert.throws(() => VENUES.mexc.validateSocketFrame(socket, '{"channel":"rs.login","data":"success"}'));
  for (const time of [undefined, 'bad', String(now - 30001), String(now + 30001)])
    assert.throws(() => VENUES.mexc.validateSocketMessage(socket, login(time), now));
  assert.throws(() => VENUES.mexc.validateSocketMessage(socket, login(String(now), 'wrong'), now));
  VENUES.mexc.validateSocketMessage(socket, login(String(now)), now);
  assert.equal(socket.loginVerified, true);
  VENUES.mexc.validateSocketFrame(socket, '{"channel":"rs.login","data":"success"}');
});
