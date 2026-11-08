import type { EngineSnapshot, OHLCVFrameRejection, OrderBookFrameRejection } from '../exchanges/base';

import { execFileSync } from 'node:child_process';
import { resolve } from 'node:path';

import { afterEach, beforeAll, beforeEach, expect, it, vi } from 'vitest';

import { Frame } from '@tealstreet/safe-cex-wasm/wire';
import { consumerFrame } from '../__tests__/cutover/consumerFrame';
import { marketDataFrameApplier } from './marketDataFrameApplier.fixture';
import { marketDataWorkerExchange } from './marketDataWorkerExchange.fixture';
import { DefaultStore } from './store.base';

const BaseExchange = marketDataWorkerExchange();
type BaseExchange = InstanceType<typeof BaseExchange>;

const mirrors: BaseExchange[] = [];
const valid = () =>
  consumerFrame({
    markets: [],
    loaded: { markets: true, balance: false, orders: false, tickers: false, positions: false },
  });
const settle = () => vi.advanceTimersByTimeAsync(0);

function mirror(snapshot: () => EngineSnapshot) {
  const exchange = new BaseExchange('bybit', { key: '', secret: '' }, new DefaultStore());
  mirrors.push(exchange);
  const calls = vi.fn();
  exchange.on('callMethod', (event: { requestId: string; method: string }) => {
    calls();
    exchange.emitter.emit('callMethod:done', { requestId: event.requestId, success: true, result: snapshot() });
  });
  return { exchange, calls };
}

beforeEach(() => {
  vi.useFakeTimers();
  vi.setSystemTime(1_700_000_000_000);
  vi.spyOn(Math, 'random').mockReturnValue(0);
  vi.spyOn(console, 'error').mockImplementation(() => {});
});
afterEach(() => {
  for (const exchange of mirrors.splice(0)) exchange.dispose();
  vi.useRealTimers();
  vi.restoreAllMocks();
});

it.each([0, 0.5, 0.999])(
  'bounds malformed frames and snapshots with exponential backoff and a capped delay (jitter %s)',
  async (jitter) => {
    vi.mocked(Math.random).mockReturnValue(jitter);
    const { exchange, calls } = mirror(() => ({ frame: new ArrayBuffer(0), fetchedLeverage: {} }));
    for (let i = 0; i < 600; i++) {
      exchange.applyWorkerFrame(new ArrayBuffer(0));
      await settle();
      await vi.advanceTimersByTimeAsync(100);
    }
    expect(calls.mock.calls.length).toBeGreaterThan(4);
    expect(calls.mock.calls.length).toBeLessThanOrEqual(10);
  },
);

it('resets recovery after a good replacement frame', async () => {
  const { exchange, calls } = mirror(() => ({ frame: new ArrayBuffer(0), fetchedLeverage: {} }));
  exchange.applyWorkerFrame(new ArrayBuffer(0));
  await settle();
  exchange.applyWorkerFrame(new ArrayBuffer(0));
  await settle();
  expect(calls).toHaveBeenCalledTimes(1);
  exchange.applyWorkerFrame(valid());
  exchange.applyWorkerFrame(new ArrayBuffer(0));
  await settle();
  expect(calls).toHaveBeenCalledTimes(2);
});

let fixtures: Record<string, number[]>;
beforeAll(() => {
  const root = resolve(__dirname, '../../../..');
  fixtures = JSON.parse(
    execFileSync(
      'cargo',
      [
        'run',
        '--locked',
        '-j2',
        '--quiet',
        '--manifest-path',
        `${root}/sdk-rs/Cargo.toml`,
        '-p',
        'safe-cex-engine',
        '--example',
        'market_data_fixture',
      ],
      { cwd: root, encoding: 'utf8' },
    ),
  );
}, 120_000);
const nativeFrame = (name: string) => Uint8Array.from(fixtures[name]).buffer;

it('keeps callback signals scoped to the rejected section and subscription identity and removes them on unsubscribe', async () => {
  const { exchange } = mirror(() => ({ frame: nativeFrame('snapshot'), fetchedLeverage: {} }));
  const Applier = marketDataFrameApplier();
  Object.defineProperty(exchange, 'workerFrameApplier', { value: new Applier(exchange.store) });
  const book = vi.fn();
  const candle = vi.fn();
  const stopBook = exchange.listenOrderBookWebworker('ZERO/USD', () => {}, book);
  const stopCandle = exchange.listenOHLCVWebworker({ symbol: 'ZERO/USD', interval: '1m' }, () => {}, candle);
  exchange.applyWorkerFrame(nativeFrame('snapshot'));
  exchange.applyWorkerFrame(nativeFrame('invalidCandle'));
  expect(book).not.toHaveBeenCalled();
  expect(candle).toHaveBeenCalledExactlyOnceWith({
    stream: 'ohlcv',
    symbol: 'ZERO/USD',
    interval: '1m',
    error: expect.any(Error),
  });
  await settle();
  candle.mockClear();
  exchange.applyWorkerFrame(nativeFrame('unordered'));
  expect(book).toHaveBeenCalledExactlyOnceWith({ stream: 'orderBook', symbol: 'ZERO/USD', error: expect.any(Error) });
  expect(candle).not.toHaveBeenCalled();
  await settle();
  stopBook();
  stopCandle();
  exchange.applyWorkerFrame(new ArrayBuffer(0));
  expect(book).toHaveBeenCalledTimes(1);
  expect(candle).not.toHaveBeenCalled();
});

it.each([
  { rejected: 'invalidCandle', unrelated: 'bookOnly' },
  { rejected: 'unordered', unrelated: 'candleOnly' },
])('does not reset $rejected on unrelated valid $unrelated traffic', async ({ rejected, unrelated }) => {
  const { exchange, calls } = mirror(() => ({ frame: nativeFrame(unrelated), fetchedLeverage: {} }));
  const Applier = marketDataFrameApplier();
  Object.defineProperty(exchange, 'workerFrameApplier', { value: new Applier(exchange.store) });
  exchange.applyWorkerFrame(nativeFrame('snapshot'));
  exchange.applyWorkerFrame(nativeFrame(rejected));
  await settle();
  exchange.applyWorkerFrame(nativeFrame(unrelated));
  exchange.applyWorkerFrame(nativeFrame(rejected));
  await settle();
  expect(calls).toHaveBeenCalledTimes(1);
  exchange.applyWorkerFrame(nativeFrame('snapshot'));
  exchange.applyWorkerFrame(nativeFrame(rejected));
  await settle();
  expect(calls).toHaveBeenCalledTimes(2);
});

it('doubles the cooldown to the jittered cap instead of polling at a fixed interval', async () => {
  const { exchange, calls } = mirror(() => ({ frame: new ArrayBuffer(0), fetchedLeverage: {} }));
  exchange.applyWorkerFrame(new ArrayBuffer(0));
  await settle();
  let count = 1;
  for (const delay of [250, 500, 1000, 2000, 4000, 8000, 15000, 15000]) {
    await vi.advanceTimersByTimeAsync(delay - 1);
    exchange.applyWorkerFrame(new ArrayBuffer(0));
    await settle();
    expect(calls).toHaveBeenCalledTimes(count);
    await vi.advanceTimersByTimeAsync(1);
    exchange.applyWorkerFrame(new ArrayBuffer(0));
    await settle();
    expect(calls).toHaveBeenCalledTimes(++count);
  }
});

it('ignores an older in-flight recovery snapshot after an accepted replacement frame', async () => {
  const exchange = new BaseExchange('bybit', { key: '', secret: '' }, new DefaultStore());
  mirrors.push(exchange);
  let reply: ((snapshot: EngineSnapshot) => void) | undefined;
  exchange.on('callMethod', (event: { requestId: string }) => {
    reply = (snapshot) =>
      exchange.emitter.emit('callMethod:done', { requestId: event.requestId, success: true, result: snapshot });
  });
  const rejection = vi.fn();
  exchange.listenOrderBookWebworker('ZERO/USD', () => {}, rejection);
  exchange.applyWorkerFrame(new ArrayBuffer(0));
  await settle();
  exchange.applyWorkerFrame(valid());
  if (!reply) throw new Error('Expected pending snapshot request');
  reply({ frame: new ArrayBuffer(0), fetchedLeverage: {} });
  await settle();
  expect(rejection).toHaveBeenCalledTimes(1);
});

it('isolates rejection observers and never requests recovery after disposal', async () => {
  const { exchange, calls } = mirror(() => ({ frame: valid(), fetchedLeverage: {} }));
  exchange.listenOrderBookWebworker(
    'A',
    () => {},
    () => {
      throw new Error('observer failed');
    },
  );
  const healthyObserver = vi.fn();
  exchange.listenOrderBookWebworker('B', () => {}, healthyObserver);
  exchange.applyWorkerFrame(new ArrayBuffer(0));
  expect(healthyObserver).toHaveBeenCalledExactlyOnceWith({
    stream: 'orderBook',
    symbol: 'B',
    error: expect.any(Error),
  });
  exchange.dispose();
  await settle();
  expect(calls).not.toHaveBeenCalled();
});

it.each([
  { rejected: 'unordered', recovered: 'bookUpdate' },
  { rejected: 'invalidCandle', recovered: 'candleUpdate' },
])('resets $rejected after a good $recovered without requiring a new catalogue', async ({ rejected, recovered }) => {
  const { exchange, calls } = mirror(() => {
    throw new Error('snapshot unavailable');
  });
  const Applier = marketDataFrameApplier();
  Object.defineProperty(exchange, 'workerFrameApplier', { value: new Applier(exchange.store) });
  exchange.applyWorkerFrame(nativeFrame('snapshot'));
  exchange.applyWorkerFrame(nativeFrame(rejected));
  await settle();
  expect(calls).toHaveBeenCalledTimes(1);
  exchange.applyWorkerFrame(nativeFrame(recovered));
  exchange.applyWorkerFrame(nativeFrame(rejected));
  await settle();
  expect(calls).toHaveBeenCalledTimes(2);
});

it('queues a newer rejection received while a now-stale snapshot RPC is in flight', async () => {
  const exchange = new BaseExchange('bybit', { key: '', secret: '' }, new DefaultStore());
  mirrors.push(exchange);
  let requests = 0;
  let reply: ((snapshot: EngineSnapshot) => void) | undefined;
  exchange.on('callMethod', (event: { requestId: string }) => {
    requests++;
    reply = (snapshot) =>
      exchange.emitter.emit('callMethod:done', { requestId: event.requestId, success: true, result: snapshot });
  });
  exchange.applyWorkerFrame(new ArrayBuffer(0));
  await settle();
  exchange.applyWorkerFrame(valid());
  exchange.applyWorkerFrame(new ArrayBuffer(0));
  expect(requests).toBe(1);
  if (!reply) throw new Error('Expected pending snapshot');
  reply({ frame: new ArrayBuffer(0), fetchedLeverage: {} });
  await settle();
  expect(requests).toBe(2);
});

function identityFrame(name: string, market = 0, interval = '1m') {
  const bytes = new Uint8Array(nativeFrame(name));
  const frame = new Frame(bytes.buffer);
  let marketOffset: number;
  if (name.toLowerCase().includes('candle')) {
    const rows = frame.candles();
    if (!rows) throw new Error('Missing candle fixture');
    marketOffset = rows.market.byteOffset;
  } else {
    const [book] = frame.books();
    if (!book) throw new Error('Missing book fixture');
    marketOffset = book.bids.byteOffset - 16;
  }
  new DataView(bytes.buffer).setUint32(marketOffset, market, true);
  if (interval === '5m') {
    for (let i = 0; i < bytes.length - 1; i++) {
      if (bytes[i] === 49 && bytes[i + 1] === 109) bytes[i] = 53;
    }
  }
  return bytes.buffer;
}

function identityMirror(snapshot: () => ArrayBuffer) {
  const result = mirror(() => ({ frame: snapshot(), fetchedLeverage: {} }));
  const Applier = marketDataFrameApplier();
  Object.defineProperty(result.exchange, 'workerFrameApplier', { value: new Applier(result.exchange.store) });
  const market = (symbol: string, id: string) => ({
    symbol,
    id,
    base: id,
    quote: 'USD',
    active: true,
    precision: { amount: 0.001, price: 0.01 },
    limits: { amount: { min: 0, max: 100 }, leverage: { min: 1, max: 100 } },
  });
  result.exchange.applyWorkerFrame(consumerFrame({ markets: [market('A/USD', 'a'), market('B/USD', 'b')] }));
  return result;
}

const unrelatedStreams = [
  { name: 'different book symbol', bad: 'unordered', good: 'bookUpdate', market: 1, interval: '1m' },
  { name: 'different candle symbol', bad: 'invalidCandle', good: 'candleUpdate', market: 1, interval: '1m' },
  { name: 'different candle interval', bad: 'invalidCandle', good: 'candleUpdate', market: 0, interval: '5m' },
];

it.each(unrelatedStreams)(
  'keeps stream backoff when recovery returns a $name',
  async ({ bad, good, market, interval }) => {
    const { exchange, calls } = identityMirror(() => identityFrame(good));
    for (let i = 0; i < 20; i++) {
      exchange.applyWorkerFrame(identityFrame(bad, market, interval));
      await settle();
    }
    expect(calls).toHaveBeenCalledTimes(1);
    exchange.applyWorkerFrame(identityFrame(good, market, interval));
    exchange.applyWorkerFrame(identityFrame(bad, market, interval));
    await settle();
    expect(calls).toHaveBeenCalledTimes(2);
  },
);

it.each(unrelatedStreams)(
  'keeps stream backoff across healthy traffic for a $name',
  async ({ bad, good, market, interval }) => {
    const { exchange, calls } = identityMirror(() => identityFrame(bad, market, interval));
    for (let i = 0; i < 20; i++) {
      exchange.applyWorkerFrame(identityFrame(bad, market, interval));
      await settle();
      exchange.applyWorkerFrame(identityFrame(good));
      await settle();
    }
    expect(calls).toHaveBeenCalledTimes(1);
    exchange.applyWorkerFrame(identityFrame(good, market, interval));
    exchange.applyWorkerFrame(identityFrame(bad, market, interval));
    await settle();
    expect(calls).toHaveBeenCalledTimes(2);
  },
);

it.each(unrelatedStreams)('reports only the rejected identity for a $name', ({ bad, market, interval }) => {
  const { exchange } = identityMirror(() => identityFrame(bad, market, interval));
  const signals: Array<OrderBookFrameRejection | OHLCVFrameRejection> = [];
  for (const symbol of ['A/USD', 'B/USD']) {
    exchange.listenOrderBookWebworker(
      symbol,
      () => {},
      (signal) => signals.push(signal),
    );
    const intervals: Array<'1m' | '5m'> = ['1m', '5m'];
    for (const candleInterval of intervals) {
      exchange.listenOHLCVWebworker(
        { symbol, interval: candleInterval },
        () => {},
        (signal) => signals.push(signal),
      );
    }
  }
  exchange.applyWorkerFrame(identityFrame(bad, market, interval));
  expect(signals).toHaveLength(1);
  expect(signals[0]).toMatchObject({
    stream: bad === 'unordered' ? 'orderBook' : 'ohlcv',
    symbol: market === 0 ? 'A/USD' : 'B/USD',
    ...(bad === 'unordered' ? {} : { interval }),
    error: expect.any(Error),
  });
});

it('keeps backoff until both unresolved book identities recover', async () => {
  const { exchange, calls } = identityMirror(() => identityFrame('unordered', 1));
  exchange.applyWorkerFrame(identityFrame('unordered', 0));
  await settle();
  exchange.applyWorkerFrame(identityFrame('unordered', 1));
  exchange.applyWorkerFrame(identityFrame('bookUpdate', 0));
  exchange.applyWorkerFrame(identityFrame('unordered', 1));
  await settle();
  expect(calls).toHaveBeenCalledTimes(1);
  exchange.applyWorkerFrame(identityFrame('bookUpdate', 1));
  exchange.applyWorkerFrame(identityFrame('unordered', 1));
  await settle();
  expect(calls).toHaveBeenCalledTimes(2);
});

it.each(['unordered', 'invalidCandle'])(
  'retains unidentified %s recovery across unrelated catalogue snapshots',
  async (bad) => {
    const { exchange, calls } = identityMirror(() => nativeFrame('snapshot'));
    for (let i = 0; i < 20; i++) {
      exchange.applyWorkerFrame(identityFrame(bad, 99));
      await settle();
      exchange.applyWorkerFrame(nativeFrame('snapshot'));
      await settle();
    }
    expect(calls).toHaveBeenCalledTimes(1);
  },
);
