import assert from 'node:assert/strict';
import test from 'node:test';
import { bodyBytes, canonicalBody, canonicalUrl, ReplayClock, Tape } from './tape.mjs';

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
