// @vitest-environment node
import type { WasmEngineLike } from '@tealstreet/safe-cex-wasm/host';
import type { WasmEngineModule } from '../../exchanges/rust/rust.exchange';
import type { ExchangeName, ExchangeOptions, Market, OHLCVOptions, StoreData } from '../../types';

import { MessageChannel } from 'node:worker_threads';

import { afterEach, expect, it, vi } from 'vitest';

import { Frame } from '@tealstreet/safe-cex-wasm/wire';
import { loadNodeEngine } from '../../exchanges/rust/loadNodeEngine';
import { RustEngineExchange, setRustEngineTransport, setWasmEngineLoader } from '../../exchanges/rust/rust.exchange';
import { DefaultStore } from '../../store/store.base';
import { directTransport } from '../../transport/direct';
import { engineCapabilities } from '../fixtures/engineCapabilities';
import { consumerFrame } from '../cutover/consumerFrame';

const accounts: RustEngineExchange[] = [];
const cleanups: Array<() => void> = [];
const flags = (publicReady = false, privateReady = false) => ({
  markets: publicReady,
  tickers: publicReady,
  balance: privateReady,
  orders: privateReady,
  positions: privateReady,
});
const market = (symbol: string): Market => ({
  id: symbol,
  symbol,
  base: symbol,
  quote: 'USD',
  active: true,
  precision: { amount: 1, price: 0.1 },
  limits: { amount: { min: 1, max: 100 }, leverage: { min: 1, max: 10 } },
});
const symbols = ['BTC:USD', 'OLD', 'NEW'];
const markets = symbols.map(market);
const tickers = [
  {
    id: 'BTC:USD',
    symbol: 'BTC:USD',
    bid: 99,
    ask: 101,
    last: 100,
    mark: 100,
    index: 100,
    percentage: 0,
    fundingRate: 0,
    fundingTime: 0,
    openInterest: 0,
    volume: 1,
    quoteVolume: 100,
  },
];

afterEach(() => {
  accounts.splice(0).forEach((account) => account.dispose());
  cleanups.splice(0).forEach((cleanup) => cleanup());
  setWasmEngineLoader(loadNodeEngine);
  vi.useRealTimers();
});

function fixture(name: ExchangeName = 'binance', options: Partial<ExchangeOptions> = {}) {
  const commands: Array<{ op: string; symbol?: string; interval?: string }> = [];
  let onCommand: (command: (typeof commands)[number]) => void = () => {};
  let release!: () => void;
  let constructed = 0;
  const idle = () => '';
  const native: WasmEngineLike = {
    capabilities: () => engineCapabilities(name),
    start: idle,
    wsOpen: idle,
    wsMessage: idle,
    wsClose: idle, wsRouteExhausted: idle,
    httpStart: idle,
    httpChunk: idle,
    httpEnd: idle,
    httpFailed: idle,
    timer: idle,
    takeFrame: () => undefined,
    snapshotFrame: () => new Uint8Array(consumerFrame({ markets: [], tickers: [], loaded: flags() })),
    command(json) {
      const command = JSON.parse(json);
      commands.push(command);
      onCommand(command);
      return '';
    },
  };
  const module: WasmEngineModule = {
    WasmEngine: function () {
      constructed++;
      return native;
    } as unknown as WasmEngineModule['WasmEngine'],
  };
  const loading = new Promise<WasmEngineModule>((resolve) => {
    release = () => resolve(module);
  });
  setWasmEngineLoader(() => loading);
  setRustEngineTransport(() => ({
    now: Date.now,
    connect() {
      throw new Error('Control fixture must not connect');
    },
    async request() {
      throw new Error('Control fixture must not request');
    },
    setTimeout,
    clearTimeout: (handle) => clearTimeout(handle as ReturnType<typeof setTimeout>),
  }));
  const store = new DefaultStore();
  // A worker restores old cached state before constructing its new native engine.
  store.update({ markets: [market('CACHE')], tickers: [], loaded: flags(true, true) });
  const exchange = new RustEngineExchange(
    name,
    { key: 'key', secret: 'secret', transport: directTransport, ...options },
    store,
  );
  accounts.push(exchange);
  const deliver = (data: Partial<StoreData>, events = false) =>
    (exchange as unknown as { receiveFrame(buffer: ArrayBuffer): void }).receiveFrame(
      consumerFrame(data, undefined, events, symbols),
    );
  // Private readiness needs decoded account sections as well as STATUS flags.
  const account = { balance: { used: 0, free: 0, total: 0, upnl: 0 }, orders: [], positions: [] };
  const ready = (privateReady = false) =>
    deliver({ markets, tickers, ...(privateReady ? account : {}), loaded: flags(true, privateReady) });
  return {
    exchange,
    commands,
    native,
    release,
    ready,
    deliver,
    onCommand(fn: typeof onCommand) {
      onCommand = fn;
    },
    get constructed() {
      return constructed;
    },
  };
}

const subscriptions = (commands: Array<{ op: string; symbol?: string }>) =>
  commands.filter(({ op }) => op.startsWith('subscribe'));

it('replays current demand after native metadata, before private balances, ignoring seeded flags', async () => {
  const f = fixture();
  const callback = vi.fn();
  f.exchange.listenOrderBook('BTC:USD', callback);
  f.exchange.listenOrderBook('BTC:USD', callback);
  const stop = f.exchange.listenOrderBook('OLD', vi.fn());
  stop();
  f.exchange.listenOHLCV({ symbol: 'BTC:USD', interval: '1m' }, vi.fn());
  const started = vi.fn();
  const startup = f.exchange.start().then(started);
  f.release();
  await vi.waitFor(() => expect(f.constructed).toBe(1));
  expect(subscriptions(f.commands)).toEqual([]);
  expect(started).not.toHaveBeenCalled();
  f.ready();
  expect(subscriptions(f.commands)).toEqual([
    { op: 'subscribeOrderBook', symbol: 'BTC:USD' },
    { op: 'subscribeCandles', symbol: 'BTC:USD', interval: '1m' },
  ]);
  expect(started).not.toHaveBeenCalled();
  f.ready(true);
  await startup;
  expect(subscriptions(f.commands)).toHaveLength(2);
});

it('replay snapshots both maps and checks live ownership during synchronous add/remove/re-add', async () => {
  const f = fixture();
  f.exchange.listenOrderBook('BTC:USD', vi.fn());
  const stopOld = f.exchange.listenOrderBook('OLD', vi.fn());
  const stopCandle = f.exchange.listenOHLCV({ symbol: 'OLD', interval: '1m' }, vi.fn());
  const read = f.exchange.fetchMarkets();
  f.release();
  await vi.waitFor(() => expect(f.constructed).toBe(1));
  f.onCommand(({ op, symbol }) => {
    if (op === 'subscribeOrderBook' && symbol === 'BTC:USD') {
      stopOld();
      stopCandle();
      f.exchange.listenOrderBook('OLD', vi.fn());
      f.exchange.listenOrderBook('NEW', vi.fn());
      f.exchange.listenOHLCV({ symbol: 'NEW', interval: '1m' }, vi.fn());
    }
  });
  f.ready();
  await read;
  expect(subscriptions(f.commands).map(({ op, symbol }) => `${op}:${symbol}`)).toEqual([
    'subscribeOrderBook:BTC:USD',
    'subscribeOrderBook:OLD',
    'subscribeOrderBook:NEW',
    'subscribeCandles:NEW',
  ]);
});

it('registers the first book/candle callback before immediate data and forwards catalog first', async () => {
  const f = fixture();
  const book = vi.fn();
  const candle = vi.fn();
  f.exchange.listenOrderBookForFeed('BTC:USD', book);
  f.exchange.listenOHLCVForFeed({ symbol: 'BTC:USD', interval: '1m' }, candle);
  const forwarded: string[] = [];
  f.exchange.onFrame = (buffer) => {
    const received = structuredClone(buffer, { transfer: [buffer] });
    const frame = new Frame(received);
    forwarded.push(frame.markets() ? 'catalog' : [...frame.books()].length ? 'data' : 'other');
  };
  const startup = f.exchange.start();
  f.release();
  await vi.waitFor(() => expect(f.constructed).toBe(1));
  f.onCommand(({ op }) => {
    if (op.startsWith('subscribe')) f.deliver({}, true);
  });
  f.ready(true);
  await startup;
  expect(forwarded.slice(-3)).toEqual(['catalog', 'data', 'data']);
  expect(book).toHaveBeenCalled();
  expect(candle).toHaveBeenCalled();
  const newBook = vi.fn();
  const newCandle = vi.fn();
  const stopBook = f.exchange.listenOrderBookForFeed('BTC:USD', newBook);
  const stopCandle = f.exchange.listenOHLCVForFeed({ symbol: 'BTC:USD', interval: '1m' }, newCandle);
  stopBook();
  stopCandle();
  expect(f.commands.some(({ op }) => op.startsWith('unsubscribe'))).toBe(false);
});

// Intervals reach the engine unchanged; it rejects what Aster cannot serve.
it('Aster retains independent registrations and forwards interval demand unchanged', async () => {
  const f = fixture('asterdex', { publicDataOnly: true });
  const callback = vi.fn();
  const stop1 = f.exchange.listenOrderBook('BTC:USD', callback);
  const stop2 = f.exchange.listenOrderBook('BTC:USD', callback);
  const stopCandle = f.exchange.listenOHLCV({ symbol: 'BTC:USD', interval: '8h' as OHLCVOptions['interval'] }, vi.fn());
  stop1();
  const read = f.exchange.fetchMarkets();
  f.release();
  await vi.waitFor(() => expect(f.constructed).toBe(1));
  f.ready();
  await read;
  expect(subscriptions(f.commands)).toContainEqual({ op: 'subscribeCandles', symbol: 'BTC:USD', interval: '8h' });
  f.deliver({}, true);
  expect(callback).toHaveBeenCalledOnce();
  stop2();
  stopCandle();
  expect(f.commands).toContainEqual({ op: 'unsubscribeOrderBook', symbol: 'BTC:USD' });
  expect(f.commands).toContainEqual({ op: 'unsubscribeCandles', symbol: 'BTC:USD', interval: '8h' });
});

it('a newly registered topic receives the synchronous first book and candle', async () => {
  const f = fixture();
  const read = f.exchange.fetchMarkets();
  f.release();
  await vi.waitFor(() => expect(f.constructed).toBe(1));
  f.ready();
  await read;
  f.onCommand(({ op }) => {
    if (op.startsWith('subscribe')) f.deliver({}, true);
  });
  const book = vi.fn();
  const candle = vi.fn();
  f.exchange.listenOrderBook('BTC:USD', book);
  expect(book).toHaveBeenCalledOnce();
  f.exchange.listenOHLCV({ symbol: 'BTC:USD', interval: '1m' }, candle);
  expect(candle).toHaveBeenCalledOnce();
});

it('concurrent unstarted public reads wait for their native flags, not private balances', async () => {
  const f = fixture();
  const marketRead = f.exchange.fetchMarkets();
  const tickerReady = vi.fn();
  const tickerRead = f.exchange.fetchTickers().then((rows) => {
    tickerReady();
    return rows;
  });
  f.release();
  await vi.waitFor(() => expect(f.constructed).toBe(1));
  f.deliver({ markets, loaded: { ...flags(), markets: true } });
  expect((await marketRead).map(({ symbol }) => symbol)).toEqual(symbols);
  expect(tickerReady).not.toHaveBeenCalled();
  f.deliver({ tickers, loaded: flags(true) });
  expect(await tickerRead).toMatchObject([{ symbol: 'BTC:USD', bid: 99 }]);
  expect(f.constructed).toBe(1);
  expect(f.exchange.store.loaded.balance).toBe(false);
});

it('bounds public waits and wakes them on disposal without returning cached data', async () => {
  vi.useFakeTimers();
  const f = fixture();
  const read = expect(f.exchange.fetchMarkets()).rejects.toThrow('Timed out waiting for native markets');
  await vi.advanceTimersByTimeAsync(30_000);
  await read;
  const tickerRead = expect(f.exchange.fetchTickers()).rejects.toThrow('disposed');
  f.exchange.dispose();
  await tickerRead;
  f.release();
  await vi.advanceTimersByTimeAsync(0);
  expect(f.constructed).toBe(0);
});

it('public reads reject startup failure and a native trap instead of exposing cached state', async () => {
  const f = fixture('binance', {
    externalPublicData: { type: 'websocket', url: 'wss://example.test', exchangeId: 'binance' },
  });
  await expect(f.exchange.fetchMarkets()).rejects.toThrow('externalPublicData');
  const g = fixture();
  g.native.start = () => {
    throw new WebAssembly.RuntimeError('unreachable');
  };
  const read = expect(g.exchange.fetchTickers()).rejects.toMatchObject({ code: 'ENGINE_DEAD' });
  g.release();
  await read;
  await expect(g.exchange.fetchMarkets()).rejects.toMatchObject({ code: 'ENGINE_DEAD' });
});

it('shared catalog mismatch sends zero book/candle demand, even with a reentrant public read', async () => {
  const channel = new MessageChannel();
  cleanups.push(() => {
    channel.port1.close();
    channel.port2.close();
  });
  const requests: Array<{ op: string }> = [];
  channel.port1.on('message', (message) => {
    requests.push(message);
    if (message.op === 'hello') channel.port1.postMessage({ op: 'ready', markets: [] });
  });
  const f = fixture('binance', { publicFeed: { mode: 'shared', port: channel.port2 as unknown as MessagePort } });
  f.exchange.listenOrderBook('BTC:USD', vi.fn());
  f.exchange.listenOHLCV({ symbol: 'BTC:USD', interval: '1m' }, vi.fn());
  let reentrantRead: Promise<unknown> | undefined;
  f.exchange.observePublicFeed({
    updated: () => {
      reentrantRead = expect(f.exchange.fetchMarkets()).rejects.toThrow('mismatch');
    },
    closed() {},
  });
  const read = expect(f.exchange.fetchMarkets()).rejects.toThrow('mismatch');
  f.release();
  await vi.waitFor(() => expect(requests.some(({ op }) => op === 'hello')).toBe(true));
  await new Promise((resolve) => setTimeout(resolve, 10));
  f.ready();
  await read;
  await reentrantRead;
  await new Promise((resolve) => setTimeout(resolve, 0));
  expect(requests.filter(({ op }) => op === 'book' || op === 'candle')).toEqual([]);
  expect(subscriptions(f.commands)).toEqual([]);
});
