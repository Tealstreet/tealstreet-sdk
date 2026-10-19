import assert from 'node:assert/strict';
import test from 'node:test';
import { Responses } from './responses.mjs';
import { bodyBytes, canonicalBody, canonicalUrl, replayStoreReady, ReplayClock, Tape } from './tape.mjs';

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

test('explicit lifecycle retains startup opens and verifies local closes', async () => {
  const url = 'wss://example.test/public';
  const records = [
    { type: 'connect', t: 0, url, connection: 0 },
    { type: 'open', t: 1, url, connection: 0 },
    { type: 'close', t: 2, url, connection: 0, local: true },
  ];
  const tape = new Tape(records, { explicitOpens: true, legacyLatest: false });
  const seen = [];
  const socket = tape.connect(url, () => seen.push('open'), () => {}, () => seen.push('close'));
  await new Promise(setImmediate);
  assert.deepEqual(seen, []);
  await tape.lifecycle(tape.frames[0]);
  assert.deepEqual(seen, ['open']);
  await assert.rejects(tape.lifecycle(tape.frames[1]), /local close was not requested/);
  tape.close(socket, 1000, true);
  await tape.lifecycle(tape.frames[1]);
  assert.deepEqual(seen, ['open']);
});

test('frame URL preparation shares canonical spelling and preserves both ordinal formats', () => {
  const url = 'wss://example.test/shared';
  let canonicalizations = 0;
  const tape = new Tape([
    { type: 'ws', t: 0, url, socket: 0, text: '{}' },
    { type: 'ws', t: 1, url, connection: 1, text: '{}' },
  ], { canonicalize: (value) => { canonicalizations++; return canonicalUrl(value); } });
  assert.equal(canonicalizations, 1);
  assert.deepEqual(tape.frames.map((frame) => frame.connection), [0, 1]);
  assert.equal(tape.frames[0].url, tape.frames[1].url);
});

test('explicit remote closes notify once and prevent delivery to the old connection', async () => {
  const url = 'wss://example.test/private';
  const tape = new Tape([{ type: 'close', t: 0, url, connection: 0, code: 1006 }]);
  const seen = [];
  tape.connect(url, () => {}, () => {}, (code) => seen.push(code));
  await new Promise(setImmediate);
  await tape.lifecycle(tape.frames[0]);
  assert.deepEqual(seen, [1006]);
  await assert.rejects(tape.lifecycle(tape.frames[0]), /before recorded remote close/);
});

test('timer HTTP reads and mutations use the timer time while advancing toward a later frame', async () => {
  const read = { type: 'http', method: 'GET', url: 'https://example.test/state', status: 200 };
  const write = { type: 'http', method: 'POST', url: 'https://example.test/change', status: 200, body: 'written', effectiveAt: 3000 };
  const records = [
    { ...read, body: 'initial', effectiveAt: 0 },
    { ...read, body: 'early', effectiveAt: 500 },
    { ...read, body: 'later', effectiveAt: 3000 }, write,
    { type: 'command', t: 5000, op: 'finish' },
  ];
  const responses = new Responses(records, (method, url) => method + url);
  const tape = new Tape(records, { responses });
  const clock = new ReplayClock(1700000000000);
  const seen = [];
  clock.install();
  try {
    await tape.play(undefined, async () => {
      for (const time of [1000, 4000]) setTimeout(() => {
        seen.push([tape.respond('GET', read.url).body, tape.respond('POST', write.url).status]);
      }, time);
    }, async () => [], clock);
    assert.deepEqual(seen, [['early', 599], ['later', 200]]);
    assert.equal(responses.unusedWrites(), 0);
  } finally { clock.restore(); }
});

test('legacy shared-URL initial sockets auto-open but recorded reconnects wait for their open', async () => {
  const url = 'wss://example.test/shared';
  const tape = new Tape([
    { type: 'open', t: 0, url, socket: 0 },
    { type: 'open', t: 1, url, socket: 1 },
    { type: 'close', t: 2, url, socket: 0, code: 1006 },
    { type: 'open', t: 50, url, socket: 2 },
  ]);
  const seen = [];
  tape.connect(url, () => seen.push('public'), () => {}, () => {
    tape.connect(url, () => seen.push('reconnect'), () => {});
  });
  tape.connect(url, () => seen.push('private'), () => {});
  await new Promise(setImmediate);
  assert.deepEqual(seen, ['public', 'private']);
  await tape.lifecycle(tape.frames[2]);
  await new Promise(setImmediate);
  assert.deepEqual(seen, ['public', 'private']);
  await tape.lifecycle(tape.frames[3]);
  assert.deepEqual(seen, ['public', 'private', 'reconnect']);
});

test('recorded startup opens can finish asynchronous socket and subscription bootstrap before tape time', async () => {
  const url = 'wss://example.test/explicit';
  const clock = new ReplayClock(1700000000000);
  const tape = new Tape([
    { type: 'connect', t: 0, url, connection: 0 },
    { type: 'open', t: 702, url, connection: 0 },
  ], { explicitOpens: true });
  const seen = [];
  let subscribed;
  const ready = new Promise((resolve) => { subscribed = resolve; });
  clock.install();
  try {
    const constructed = new Promise((resolve) => setTimeout(() => {
      tape.connect(url, () => setTimeout(() => {
        seen.push('subscribed');
        subscribed();
      }, 5), () => {});
      resolve();
    }, 5));
    // Playback may encounter the first recorded open before async startup runs.
    // An inactive clock leaves those timers attached to real time.
    await clock.advance(702);
    assert.equal(clock.active, false);
    await constructed;
    await tape.lifecycle(tape.frames[0]);
    await ready;
    assert.deepEqual(seen, ['subscribed']);
    clock.start(702);
    assert.equal(Date.now(), 1700000000702);
    setTimeout(() => seen.push(Date.now()), 100);
    await clock.advance(801);
    assert.deepEqual(seen, ['subscribed']);
    await clock.advance(802);
    assert.deepEqual(seen, ['subscribed', 1700000000802]);
  } finally { clock.restore(); }
});
