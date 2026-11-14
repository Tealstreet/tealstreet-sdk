import type {
  CallableMethodsInWorker,
  EngineSnapshot,
  OHLCVFrameRejection,
  OrderBookFrameRejection,
} from '../../exchanges/base';
import type { OrderFillEvent } from '../../types';

import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import { Frame, LOADED_FLAG, SECTION } from '@tealstreet/safe-cex-wasm/wire';
import { BaseExchange } from '../../exchanges/base';
import { InProcessPublicDataProvider } from '../../exchanges/in-process-public-data';
import { RustEngineExchange } from '../../exchanges/rust/rust.exchange';
import { FrameApplier } from '../../store/frameApplier';
import { DefaultStore } from '../../store/store.base';
import { ExchangeWorker } from '../../webworker/ExchangeWorker';
import { CallMethodHandler } from '../../webworker/handlers/CallMethodHandler';
import { OHLCVSubscriptionsHandler } from '../../webworker/handlers/OHLCVSubscriptionsHandler';
import { OrderBookSubscriptionsHandler } from '../../webworker/handlers/OrderBookSubscriptionsHandler';
import { adoptEngineCapabilities } from '../fixtures/engineCapabilities';

vi.mock('../../exchanges/rust/betaVenues', async (importOriginal) => {
  const { retainedRustPolicy } = await import('../reference/retainedRustPolicy');
  return retainedRustPolicy(await importOriginal<typeof import('../../exchanges/rust/betaVenues')>());
});

const align8 = (value: number) => Math.ceil(value / 8) * 8;
type FixtureOptions = {
  invalidStats?: boolean;
  brokenBook?: boolean;
  invalidBook?: boolean;
  invalidCandle?: boolean;
  unknownIdentity?: boolean;
  fills?: number;
  book?: boolean;
  total?: number;
  omitStrings?: boolean;
  settings?: boolean;
  omitSettings?: boolean;
  leverage?: number;
  buyLeverage?: number;
  sellLeverage?: number;
  markets?: boolean;
  secondaryMarket?: boolean;
  reversedMarkets?: boolean;
  fillSymbol?: string;
  loaded?: number;
};

// Independent binary sections exercise the real readers and caller recovery.
const fixture = ({
  invalidStats = false,
  brokenBook = false,
  invalidBook = false,
  invalidCandle = false,
  unknownIdentity = false,
  fills = 2,
  book = true,
  total = 42,
  omitStrings = false,
  settings = true,
  omitSettings = false,
  leverage = 5,
  buyLeverage = leverage,
  sellLeverage = leverage,
  markets = true,
  secondaryMarket = false,
  reversedMarkets = false,
  fillSymbol = 'BTCUSDT',
  loaded,
}: FixtureOptions = {}) => {
  const fillIds = Array.from({ length: fills }, (_, i) => `execution-${i + 1}`);
  const strings = ['BTCUSDT', 'BTC', 'USDT', 'order-1', '1m', 'ETHUSDT', 'ETH', ...fillIds];
  const sections: Array<{ kind: number; bytes: Uint8Array; revision?: number }> = [];
  const table = (kind: number, words: number[], numbers: number[], tail: number[] = [], financials: number[] = []) => {
    const numericOffset = align8(words.length * 4);
    const financialOffset = align8(numericOffset + numbers.length * 8 + tail.length * 4);
    const bytes = new Uint8Array(financialOffset + financials.length * 8);
    const view = new DataView(bytes.buffer);
    words.forEach((word, i) => view.setUint32(i * 4, word, true));
    numbers.forEach((number, i) => view.setFloat64(numericOffset + i * 8, number, true));
    tail.forEach((word, i) => view.setUint32(numericOffset + numbers.length * 8 + i * 4, word, true));
    financials.forEach((number, i) => view.setFloat64(financialOffset + i * 8, number, true));
    sections.push({
      kind,
      bytes,
      revision: kind === SECTION.POSITIONS || kind === SECTION.FILLS ? 2 : tail.length ? 1 : 0,
    });
  };
  if (markets) {
    const columns = [0.001, 1, 0.001, 100, 1, 100, NaN, 1, NaN, NaN];
    const references = reversedMarkets ? [5, 5, 6, 2, 0, 0, 1, 2] : [0, 0, 1, 2, 5, 5, 6, 2];
    table(
      SECTION.MARKETS,
      secondaryMarket ? [2, ...references, 1, 1] : [1, 0, 0, 1, 2, 1],
      secondaryMarket ? columns.flatMap((value) => [value, value]) : columns,
    );
  }
  if (invalidStats) {
    table(SECTION.MESSAGE_HANDLER_STATS, [0, 2], []);
    table(SECTION.TICKERS, [1, 0], [99, 101, 100, 100, 100, 1, 2, 0, 1234, 10, 1000]);
  }
  table(SECTION.ORDERS, [1, 3, 0xffffffff, 0xffffffff, 0, 256], [100, 1, 0, 1, 1234, NaN, NaN]);
  table(SECTION.POSITIONS, [1, 0, 2 << 16], [100, 50, 5, 0, 0, 0.5, 80, 10, 1234, NaN, NaN], [0, 0]);
  table(SECTION.BALANCE, [], [total - 30, 30, total, 0]);
  if (!omitSettings)
    table(SECTION.SETTINGS, settings ? [1, 0, 2] : [0], settings ? [leverage, buyLeverage, sellLeverage] : []);
  table(
    SECTION.FILLS,
    [fills, ...Array(fills).fill(3), ...Array(fills).fill(0), ...Array(fills).fill(0)],
    Array.from({ length: fills }, () => [100, 0.25]).flat(),
    [...Array(fills).fill(strings.indexOf(fillSymbol)), ...fillIds.map((id) => strings.indexOf(id))],
    Array(fills * 2).fill(NaN),
  );
  table(SECTION.CANDLES, [1, unknownIdentity ? 999 : 0, 4], [1234, 100, 101, 99, invalidCandle ? NaN : 100, 5]);
  if (book) table(SECTION.BOOK, [unknownIdentity ? 999 : 0, brokenBook ? 2 : 1, 0, 0], [invalidBook ? NaN : 100, 1, 1]);
  if (loaded !== undefined) table(SECTION.STATUS, [loaded, 0], [0]);
  const encoded = strings.map((value) => new TextEncoder().encode(value));
  const stringBytes = new Uint8Array(
    align8(4 + (strings.length + 1) * 4 + encoded.reduce((size, value) => size + value.length, 0)),
  );
  const stringView = new DataView(stringBytes.buffer);
  stringView.setUint32(0, strings.length, true);
  let end = 0;
  encoded.forEach((value, i) => {
    stringView.setUint32(4 + i * 4, end, true);
    stringBytes.set(value, 4 + (strings.length + 1) * 4 + end);
    end += value.length;
  });
  stringView.setUint32(4 + strings.length * 4, end, true);
  if (!omitStrings) sections.push({ kind: SECTION.STRINGS, bytes: stringBytes });
  const buffer = new ArrayBuffer(16 + sections.reduce((size, section) => size + 8 + section.bytes.length, 0));
  const view = new DataView(buffer);
  view.setUint32(0, 0x46584353, true);
  view.setUint16(4, 1, true);
  view.setUint16(6, sections.length, true);
  let offset = 16;
  for (const section of sections) {
    view.setUint16(offset, section.kind, true);
    view.setUint16(offset + 2, section.revision ?? 0, true);
    view.setUint32(offset + 4, section.bytes.length, true);
    new Uint8Array(buffer, offset + 8, section.bytes.length).set(section.bytes);
    offset += 8 + section.bytes.length;
  }
  return buffer;
};

const receive = (exchange: RustEngineExchange, buffer: ArrayBuffer) =>
  (exchange as unknown as { receiveFrame: (buffer: ArrayBuffer) => void }).receiveFrame(buffer);

const installHost = (exchange: RustEngineExchange, snapshot: ReturnType<typeof vi.fn>) => {
  (exchange as unknown as { host: unknown }).host = { snapshot, flush: vi.fn(), dispose: vi.fn() };
};

const bundle = (frame: ArrayBuffer): EngineSnapshot => ({ frame, fetchedLeverage: {} });

const settle = async () => {
  for (let i = 0; i < 8; i++) await Promise.resolve();
};

describe('Rust frame recovery callers', () => {
  it('isolates book callback copies from the engine policy rather than the venue name', () => {
    const exchange = new RustEngineExchange('binance', { key: '', secret: '' }, new DefaultStore());
    adoptEngineCapabilities(exchange, { isolatedBookSubscriptions: true });
    const observed: number[] = [];
    exchange.listenOrderBook('BTCUSDT', (book) => {
      book.bids[0].price = 999;
    });
    exchange.listenOrderBook('BTCUSDT', (book) => {
      observed.push(book.bids[0].price);
    });
    try {
      receive(exchange, fixture());
      expect(observed).toEqual([100]);
    } finally {
      exchange.dispose();
    }
  });

  beforeEach(() => {
    vi.spyOn(console, 'error').mockImplementation(() => {});
  });

  afterEach(() => vi.restoreAllMocks());

  it('keeps account sections and every original fill when a worker-local book is malformed', () => {
    const store = new DefaultStore();
    const exchange = new RustEngineExchange('binance', { key: '', secret: '' }, store);
    const snapshot = vi.fn(() => fixture({ book: true, fills: 0, total: 100 }));
    installHost(exchange, snapshot);
    const fills: OrderFillEvent[] = [];
    const accountAtFill: unknown[] = [];
    exchange.on('fill', (fill: OrderFillEvent) => {
      accountAtFill.push({
        total: store.balance.total,
        orderId: store.orders[0]?.id,
        contracts: store.positions[0]?.contracts,
        leverage: store.fetchedLeverage.BTCUSDT?.buyLeverage,
      });
      fills.push(fill);
    });

    expect(() => receive(exchange, fixture({ brokenBook: true }))).not.toThrow();
    expect(fills).toHaveLength(2);
    expect(fills.map((fill) => fill.id)).toEqual(['execution-1', 'execution-2']);
    expect(fills.map((fill) => fill.orderId)).toEqual(['order-1', 'order-1']);
    for (const fill of fills) {
      expect(fill).not.toHaveProperty('fee');
      expect(fill).not.toHaveProperty('realizedPnl');
    }
    expect(accountAtFill).toEqual(Array(2).fill({ total: 42, orderId: 'order-1', contracts: 0.5, leverage: 5 }));
    expect(snapshot).toHaveBeenCalledTimes(1);
    expect(store.balance.total).toBe(100);
    exchange.dispose();
  });

  it.each([
    [SECTION.MARKETS, 'MARKETS'],
    [SECTION.ORDERS, 'ORDERS'],
    [SECTION.FILLS, 'FILLS'],
  ] as const)('rejects a required NONE identity in %s %s while retaining BALANCE and requesting recovery', (kind, _sectionName) => {
    const store = new DefaultStore();
    const exchange = new RustEngineExchange('binance', { key: '', secret: '' }, store);
    const bytes = fixture({ fills: 1 });
    const view = new DataView(bytes);
    let offset = 16;
    let corrupted = false;
    for (let i = 0; i < view.getUint16(6, true); i++) {
      if (view.getUint16(offset, true) === kind) {
        view.setUint32(offset + 12, 0xffffffff, true);
        corrupted = true;
        break;
      }
      offset += 8 + view.getUint32(offset + 4, true);
    }
    expect(corrupted).toBe(true);
    let atRequest: unknown;
    const snapshot = vi.fn(() => {
      atRequest = {
        total: store.balance.total,
        marketIds: store.markets.map((row) => row.id),
        orderIds: store.orders.map((row) => row.id),
      };
      return fixture({ fills: 0 });
    });
    installHost(exchange, snapshot);
    const fills: OrderFillEvent[] = [];
    exchange.on('fill', (fill: OrderFillEvent) => fills.push(fill));
    receive(exchange, bytes);
    expect(snapshot).toHaveBeenCalledTimes(1);
    expect(atRequest).toEqual({
      total: 42,
      marketIds: kind === SECTION.MARKETS ? [] : ['BTCUSDT'],
      orderIds: kind === SECTION.FILLS ? ['order-1'] : [],
    });
    expect(fills.map((fill) => fill.orderId)).toEqual(kind === SECTION.FILLS ? [] : ['order-1']);
    exchange.dispose();
  });

  it('coalesces main-thread snapshot RPCs and applies a recovery without replaying original fills', async () => {
    const store = new DefaultStore();
    const exchange = new BaseExchange('binance', { key: '', secret: '' }, store);
    let resolve!: (snapshot: EngineSnapshot) => void;
    const pending = new Promise<EngineSnapshot>((done) => {
      resolve = done;
    });
    const rpc = vi.spyOn(exchange, 'callMethodInWorker').mockReturnValue(pending as never);
    const fills = vi.fn();
    exchange.on('fill', fills);
    exchange.applyWorkerFrame(fixture({ brokenBook: true }));
    exchange.applyWorkerFrame(fixture({ brokenBook: true }));
    expect(store.balance.total).toBe(42);
    expect(store.orders[0].id).toBe('order-1');
    expect(store.positions[0].contracts).toBe(0.5);
    expect(fills).toHaveBeenCalledTimes(4);
    await settle();
    expect(rpc).toHaveBeenCalledExactlyOnceWith('fetchEngineSnapshot');
    resolve(bundle(fixture({ fills: 0, total: 100 })));
    await settle();
    expect(store.balance.total).toBe(100);
    expect(fills).toHaveBeenCalledTimes(4);
    exchange.dispose();
  });

  it('does not recursively resnapshot a malformed recovery in either realm', async () => {
    const worker = new RustEngineExchange('binance', { key: '', secret: '' }, new DefaultStore());
    const snapshot = vi.fn(() => new ArrayBuffer(0));
    installHost(worker, snapshot);
    receive(worker, new ArrayBuffer(0));
    expect(snapshot).toHaveBeenCalledTimes(1);
    const main = new BaseExchange('binance', { key: '', secret: '' }, new DefaultStore());
    const rpc = vi.spyOn(main, 'callMethodInWorker').mockResolvedValue(bundle(new ArrayBuffer(0)) as never);
    main.applyWorkerFrame(new ArrayBuffer(0));
    await settle();
    expect(rpc).toHaveBeenCalledTimes(1);
    worker.dispose();
    main.dispose();
  });

  it.each(['worker', 'main'] as const)(
    'invalidates %s market identity after a rejected envelope until a fresh MARKETS table arrives',
    async (realm) => {
      const store = new DefaultStore();
      const exchange =
        realm === 'worker'
          ? new RustEngineExchange('binance', { key: '', secret: '' }, store)
          : new BaseExchange('binance', { key: '', secret: '' }, store);
      const send = (frame: ArrayBuffer) => {
        if (exchange instanceof RustEngineExchange) receive(exchange, frame);
        else exchange.applyWorkerFrame(frame);
      };
      send(fixture({ fills: 0, secondaryMarket: true }));
      expect(store.markets.map((market) => market.symbol)).toEqual(['BTCUSDT', 'ETHUSDT']);
      let resolve!: (snapshot: EngineSnapshot) => void;
      const pending = new Promise<EngineSnapshot>((done) => {
        resolve = done;
      });
      const snapshot = vi.fn(() => {
        throw new Error('snapshot unavailable');
      });
      const rpc =
        exchange instanceof RustEngineExchange
          ? undefined
          : vi.spyOn(exchange, 'callMethodInWorker').mockReturnValue(pending as never);
      if (exchange instanceof RustEngineExchange) installHost(exchange, snapshot);
      const fills: OrderFillEvent[] = [];
      exchange.on('fill', (fill: OrderFillEvent) => fills.push(fill));

      const lostReorder = fixture({ fills: 0, secondaryMarket: true, reversedMarkets: true });
      new DataView(lostReorder).setUint16(4, 2, true);
      send(lostReorder);
      await settle();
      send(fixture({ markets: false, fills: 1, book: false, fillSymbol: 'ETHUSDT' }));
      expect(fills.map((fill) => fill.symbol)).toEqual(['ETHUSDT']);
      send(fixture({ markets: false, fills: 1, book: false }));
      expect(fills.map((fill) => fill.symbol)).toEqual(['ETHUSDT', 'BTCUSDT']);

      const recovered = fixture({ fills: 0, secondaryMarket: true, reversedMarkets: true });
      if (exchange instanceof RustEngineExchange) {
        expect(snapshot).toHaveBeenCalled();
        send(recovered);
      } else {
        expect(rpc).toHaveBeenCalledExactlyOnceWith('fetchEngineSnapshot');
        resolve(bundle(recovered));
        await settle();
      }
      send(fixture({ markets: false, fills: 1, book: false, fillSymbol: 'ETHUSDT' }));
      expect(fills.map((fill) => fill.symbol)).toEqual(['ETHUSDT', 'BTCUSDT', 'ETHUSDT']);
      exchange.dispose();
    },
  );

  it.each(['worker', 'main'] as const)(
    'keeps protocol recovery despite a throwing %s store subscriber',
    async (realm) => {
      const store = new DefaultStore();
      const exchange =
        realm === 'worker'
          ? new RustEngineExchange('binance', { key: '', secret: '' }, store)
          : new BaseExchange('binance', { key: '', secret: '' }, store);
      const bytes = fixture({ fills: 0 });
      const snapshot = vi.fn(() => bytes);
      if (exchange instanceof RustEngineExchange) installHost(exchange, snapshot);
      else vi.spyOn(exchange, 'callMethodInWorker').mockImplementation(async () => bundle(snapshot()) as never);
      const unsubscribe = store.subscribe(() => {
        throw new Error('store consumer failed');
      });
      const fill = vi.fn();
      exchange.on('fill', fill);
      if (exchange instanceof RustEngineExchange) receive(exchange, fixture({ brokenBook: true }));
      else exchange.applyWorkerFrame(fixture({ brokenBook: true }));
      await settle();
      expect(store.balance.total).toBe(42);
      expect(fill).toHaveBeenCalledTimes(2);
      expect(snapshot).toHaveBeenCalledTimes(1);
      unsubscribe();
      exchange.dispose();
    },
  );

  it.each(['worker', 'main'] as const)(
    'retains %s numeric account sections when STRINGS is missing and requests recovery',
    async (realm) => {
      const store = new DefaultStore();
      const exchange =
        realm === 'worker'
          ? new RustEngineExchange('binance', { key: '', secret: '' }, store)
          : new BaseExchange('binance', { key: '', secret: '' }, store);
      let atRequest: unknown;
      const snapshot = vi.fn(() => {
        atRequest = {
          total: store.balance.total,
          markets: store.markets,
          orders: store.orders,
          positions: store.positions,
          leverage: store.fetchedLeverage,
        };
        return fixture({ fills: 0, total: 100 });
      });
      if (exchange instanceof RustEngineExchange) installHost(exchange, snapshot);
      else vi.spyOn(exchange, 'callMethodInWorker').mockImplementation(async () => bundle(snapshot()) as never);
      const fill = vi.fn();
      exchange.on('fill', fill);
      if (exchange instanceof RustEngineExchange) receive(exchange, fixture({ omitStrings: true }));
      else exchange.applyWorkerFrame(fixture({ omitStrings: true }));
      await settle();
      expect(snapshot).toHaveBeenCalledTimes(1);
      expect(atRequest).toEqual({ total: 42, markets: [], orders: [], positions: [], leverage: {} });
      expect(fill).not.toHaveBeenCalled();
      expect(store.balance.total).toBe(100);
      expect(store.markets[0].symbol).toBe('BTCUSDT');
      exchange.dispose();
    },
  );

  it.each(['worker', 'main'] as const)(
    'does not mistake a plain %s store subscriber failure for malformed bytes',
    async (realm) => {
      const store = new DefaultStore();
      const exchange =
        realm === 'worker'
          ? new RustEngineExchange('binance', { key: '', secret: '' }, store)
          : new BaseExchange('binance', { key: '', secret: '' }, store);
      const snapshot = vi.fn();
      if (exchange instanceof RustEngineExchange) installHost(exchange, snapshot);
      else vi.spyOn(exchange, 'callMethodInWorker').mockImplementation(async () => bundle(snapshot()) as never);
      const unsubscribe = store.subscribe(() => {
        throw new Error('store consumer failed');
      });
      const fill = vi.fn();
      exchange.on('fill', fill);
      if (exchange instanceof RustEngineExchange) receive(exchange, fixture());
      else exchange.applyWorkerFrame(fixture());
      await settle();
      expect(store.balance.total).toBe(42);
      expect(fill).toHaveBeenCalledTimes(2);
      expect(snapshot).not.toHaveBeenCalled();
      unsubscribe();
      exchange.dispose();
    },
  );

  it('isolates main-thread listener errors from recovery and still emits later fills and sections', async () => {
    const exchange = new BaseExchange('binance', { key: '', secret: '' }, new DefaultStore());
    const rpc = vi.spyOn(exchange, 'callMethodInWorker');
    const candle = vi.fn();
    const fill = vi.fn(() => {
      throw new Error('consumer failed');
    });
    exchange.on('listenOrderBook', () => {
      throw new Error('book consumer failed');
    });
    exchange.on('listenOHLCV', candle);
    exchange.on('fill', fill);
    expect(() => exchange.applyWorkerFrame(fixture())).not.toThrow();
    await settle();
    expect(rpc).not.toHaveBeenCalled();
    expect(candle).toHaveBeenCalledTimes(1);
    expect(fill).toHaveBeenCalledTimes(2);
    exchange.dispose();
  });

  it('isolates worker fill/book/candle/forward listeners without resynchronizing or dropping later events', () => {
    const exchange = new RustEngineExchange('binance', { key: '', secret: '' }, new DefaultStore());
    const snapshot = vi.fn();
    installHost(exchange, snapshot);
    const fill = vi.fn(() => {
      throw new Error('fill consumer failed');
    });
    const book = vi.fn(() => {
      throw new Error('book consumer failed');
    });
    const candle = vi.fn();
    exchange.on('fill', fill);
    (exchange as unknown as { bookListeners: Map<string, Map<number, () => void>> }).bookListeners.set(
      'BTCUSDT',
      new Map([[1, book]]),
    );
    (exchange as unknown as { candleListeners: Map<string, Set<() => void>> }).candleListeners.set(
      'BTCUSDT:1m',
      new Set([candle]),
    );
    expect(() => receive(exchange, fixture())).not.toThrow();
    expect(fill).toHaveBeenCalledTimes(2);
    expect(book).toHaveBeenCalledTimes(1);
    expect(candle).toHaveBeenCalledTimes(1);
    exchange.onFrame = vi.fn(() => {
      throw new Error('forward consumer failed');
    });
    expect(() => receive(exchange, fixture())).not.toThrow();
    expect(fill).toHaveBeenCalledTimes(4);
    expect(exchange.onFrame).toHaveBeenCalledTimes(1);
    expect(snapshot).not.toHaveBeenCalled();
    exchange.dispose();
  });

  it('isolates public observers and provider listeners before forwarding valid account frames', () => {
    const store = new DefaultStore();
    const exchange = new RustEngineExchange('binance', { key: '', secret: '' }, store);
    const snapshot = vi.fn();
    installHost(exchange, snapshot);
    const observer = vi.fn(() => {
      throw new Error('public observer failed');
    });
    const laterObserver = vi.fn();
    exchange.observePublicFeed({ updated: observer, closed: vi.fn() });
    exchange.observePublicFeed({ updated: laterObserver, closed: vi.fn() });
    const provider = vi.fn(() => {
      throw new Error('provider listener failed');
    });
    const laterProvider = vi.fn();
    const ordinary = vi.fn();
    const internal = exchange as unknown as {
      bookListeners: Map<string, Map<number, () => void>>;
      feedBookListeners: Set<() => void>;
    };
    internal.bookListeners.set(
      'BTCUSDT',
      new Map([
        [1, provider],
        [2, laterProvider],
        [3, ordinary],
      ]),
    );
    internal.feedBookListeners.add(provider);
    internal.feedBookListeners.add(laterProvider);
    const fills: OrderFillEvent[] = [];
    const accountAtFill: unknown[] = [];
    exchange.on('fill', (fill: OrderFillEvent) => {
      fills.push(fill);
      accountAtFill.push({ total: store.balance.total, orderId: store.orders[0]?.id });
    });
    let forwarded: ArrayBuffer | undefined;
    const forward = vi.fn((buffer: ArrayBuffer) => {
      forwarded = structuredClone(buffer, { transfer: [buffer] });
    });
    exchange.onFrame = forward;
    const bytes = fixture();
    expect(() => receive(exchange, bytes)).not.toThrow();
    expect(observer).toHaveBeenCalledExactlyOnceWith(true);
    expect(laterObserver).toHaveBeenCalledExactlyOnceWith(true);
    expect(provider).toHaveBeenCalledTimes(1);
    expect(laterProvider).toHaveBeenCalledTimes(1);
    expect(ordinary).not.toHaveBeenCalled();
    expect(laterProvider.mock.invocationCallOrder[0]).toBeLessThan(forward.mock.invocationCallOrder[0]);
    expect(fills.map((fill) => fill.orderId)).toEqual(['order-1', 'order-1']);
    expect(accountAtFill).toEqual(Array(2).fill({ total: 42, orderId: 'order-1' }));
    expect(store.positions[0].contracts).toBe(0.5);
    expect(exchange.onFrame).toHaveBeenCalledExactlyOnceWith(bytes, false);
    expect(bytes.byteLength).toBe(0);
    if (forwarded === undefined) throw new Error('Expected the transferred frame');
    expect(new Frame(forwarded).balance()?.total).toBe(42);
    expect(snapshot).not.toHaveBeenCalled();
    expect(exchange.isDisposed).toBe(false);
    exchange.dispose();
  });

  it('keeps book listeners fed when the frame consumer is a frame-fed page engine', () => {
    const store = new DefaultStore();
    const exchange = new RustEngineExchange('binance', { key: '', secret: '' }, store);
    installHost(exchange, vi.fn());
    const ordinary = vi.fn();
    const internal = exchange as unknown as { bookListeners: Map<string, Map<number, () => void>> };
    internal.bookListeners.set('BTCUSDT', new Map([[1, ordinary]]));
    // forwardFrames: the page engine ingests account sections only.
    exchange.framesCarryMarketData = false;
    const forward = vi.fn();
    exchange.onFrame = forward;
    receive(exchange, fixture());
    expect(ordinary).toHaveBeenCalledTimes(1);
    expect(forward).toHaveBeenCalledTimes(1);
    exchange.dispose();
  });

  it('does not validate a shared catalog from JS loaded flags before native metadata is ready', () => {
    const store = new DefaultStore();
    store.update({ loaded: { ...store.loaded, markets: true } });
    const exchange = new RustEngineExchange('binance', { key: '', secret: '' }, store);
    installHost(exchange, vi.fn());
    const shared = { validateMarkets: vi.fn(), send: vi.fn(), dispose: vi.fn() };
    (exchange as unknown as { sharedPublicFeed: unknown }).sharedPublicFeed = shared;
    exchange.onFrame = vi.fn();
    const bytes = fixture();
    receive(exchange, bytes);
    expect(shared.validateMarkets).not.toHaveBeenCalled();
    expect(shared.send).not.toHaveBeenCalled();
    expect(exchange.onFrame).toHaveBeenCalledExactlyOnceWith(bytes, false);
    expect(exchange.isDisposed).toBe(false);
    exchange.dispose();
  });

  it('delivers original account fills and frame before shared catalog validation failure disposes the account', () => {
    vi.spyOn(console, 'trace').mockImplementation(() => {});
    const store = new DefaultStore();
    store.update({ loaded: { ...store.loaded, markets: true } });
    const exchange = new RustEngineExchange('binance', { key: '', secret: '' }, store);
    const snapshot = vi.fn();
    installHost(exchange, snapshot);
    const failure = new Error('shared catalog mismatch');
    const sequence: string[] = [];
    const shared = {
      validateMarkets: vi.fn(() => {
        sequence.push('validate');
        throw failure;
      }),
      send: vi.fn(),
      dispose: vi.fn(() => sequence.push('dispose')),
    };
    (exchange as unknown as { sharedPublicFeed: unknown }).sharedPublicFeed = shared;
    const fills: OrderFillEvent[] = [];
    const accountAtFill: unknown[] = [];
    exchange.on('fill', (fill: OrderFillEvent) => {
      sequence.push('fill');
      accountAtFill.push({
        disposed: exchange.isDisposed,
        total: store.balance.total,
        orderId: store.orders[0]?.id,
        contracts: store.positions[0]?.contracts,
      });
      fills.push(fill);
    });
    const errors: unknown[] = [];
    const error = vi.fn((message: string) => {
      sequence.push('error');
      errors.push({ message, disposed: exchange.isDisposed });
    });
    exchange.on('error', error);
    exchange.onFrame = vi.fn(() => sequence.push('frame'));
    const bytes = fixture({ loaded: LOADED_FLAG.MARKETS });
    expect(() => receive(exchange, bytes)).not.toThrow();
    expect(fills.map((fill) => fill.orderId)).toEqual(['order-1', 'order-1']);
    expect(accountAtFill).toEqual(Array(2).fill({ disposed: false, total: 42, orderId: 'order-1', contracts: 0.5 }));
    expect(exchange.onFrame).toHaveBeenCalledExactlyOnceWith(bytes, false);
    expect(shared.validateMarkets).toHaveBeenCalledExactlyOnceWith([expect.objectContaining({ symbol: 'BTCUSDT' })]);
    expect(error).toHaveBeenCalledTimes(1);
    expect(errors).toEqual([{ message: failure.message, disposed: false }]);
    expect(shared.send).not.toHaveBeenCalled();
    expect(shared.dispose).toHaveBeenCalledTimes(1);
    expect(sequence).toEqual(['validate', 'fill', 'fill', 'frame', 'error', 'dispose']);
    expect(exchange.isDisposed).toBe(true);
    expect(snapshot).not.toHaveBeenCalled();
    expect((exchange as unknown as { startupFailure: Error }).startupFailure).toBe(failure);
  });

  it('returns engine snapshot bundles through the typed worker method and rejects TS-only exchanges', async () => {
    const store = new DefaultStore();
    const worker = new RustEngineExchange('binance', { key: '', secret: '' }, store);
    const bytes = fixture({ fills: 0 });
    installHost(
      worker,
      vi.fn(() => bytes),
    );
    const main = new BaseExchange('binance', { key: '', secret: '' }, new DefaultStore());
    const postMessage = vi.fn((message) => main.emitter.emit(message.op, message.data));
    const handler = new CallMethodHandler(
      'account',
      'binance',
      worker,
      { postMessage } as unknown as Worker,
      {} as never,
      {} as never,
    );
    const method: CallableMethodsInWorker = 'fetchEngineSnapshot';
    main.on('callMethod', (request: { requestId: string; method: CallableMethodsInWorker; args: unknown[] }) => {
      void handler.handleCallMethod(request.requestId, request.method, request.args);
    });
    const result: EngineSnapshot = await main.callMethodInWorker(method);
    expect(result.frame).toBe(bytes);
    expect(result.fetchedLeverage).toEqual({});
    expect(postMessage).toHaveBeenCalledExactlyOnceWith({
      op: 'callMethod:done',
      data: { requestId: expect.any(String), result, success: true },
    });
    expect(postMessage.mock.calls[0][0].data.result).toBe(result);
    await expect(main.fetchEngineSnapshot()).rejects.toThrow('Engine snapshots are unavailable');
    handler.dispose();
    worker.dispose();
    main.dispose();
  });

  it('releases recovery guards after snapshot failure so later malformed frames can recover', async () => {
    const worker = new RustEngineExchange('binance', { key: '', secret: '' }, new DefaultStore());
    const snapshot = vi
      .fn()
      .mockImplementationOnce(() => {
        throw new Error('snapshot unavailable');
      })
      .mockReturnValue(fixture({ fills: 0 }));
    installHost(worker, snapshot);
    receive(worker, new ArrayBuffer(0));
    receive(worker, new ArrayBuffer(0));
    expect(snapshot).toHaveBeenCalledTimes(2);
    const recoveryClock = vi.spyOn(performance, 'now').mockReturnValue(1000);
    const main = new BaseExchange('binance', { key: '', secret: '' }, new DefaultStore());
    const rpc = vi
      .spyOn(main, 'callMethodInWorker')
      .mockRejectedValueOnce(new Error('worker unavailable'))
      .mockResolvedValue(bundle(fixture({ fills: 0 })) as never);
    main.applyWorkerFrame(new ArrayBuffer(0));
    await settle();
    recoveryClock.mockReturnValue(1500);
    main.applyWorkerFrame(new ArrayBuffer(0));
    await settle();
    expect(rpc).toHaveBeenCalledTimes(2);
    worker.dispose();
    main.dispose();
  });

  it('retains query-only leverage during worker-local book recovery without replaying original executions', () => {
    const store = new DefaultStore();
    const exchange = new RustEngineExchange('polymarket', { key: '', secret: '' }, store);
    receive(exchange, fixture({ fills: 0, settings: false }));
    const rails = { buyLeverage: 1, sellLeverage: 1 };
    (exchange as unknown as { applier: FrameApplier }).applier.applyLeverage('BTCUSDT', rails);
    const snapshot = vi.fn(() => fixture({ fills: 0, settings: false }));
    installHost(exchange, snapshot);
    const fills = vi.fn();
    exchange.on('fill', fills);
    receive(exchange, fixture({ brokenBook: true, omitSettings: true }));
    expect(snapshot).toHaveBeenCalledTimes(1);
    expect(store.fetchedLeverage).toEqual({ BTCUSDT: rails });
    expect(fills).toHaveBeenCalledTimes(2);
    exchange.dispose();
  });

  it.each([true, false])(
    'flushes pending authoritative SETTINGS before local recovery (settings present: %s)',
    (settings) => {
      const store = new DefaultStore();
      const exchange = new RustEngineExchange('binance', { key: '', secret: '' }, store);
      receive(exchange, fixture({ fills: 0 }));
      (exchange as unknown as { applier: FrameApplier }).applier.applyLeverage('DELISTED', {
        buyLeverage: 9,
        sellLeverage: 9,
      });
      const pending = fixture({ fills: 0, leverage: 12, settings });
      const flush = vi.fn(() => receive(exchange, pending));
      const snapshot = vi.fn(() => pending);
      (exchange as unknown as { host: unknown }).host = { flush, snapshot, dispose: vi.fn() };
      receive(exchange, new ArrayBuffer(0));
      expect(flush).toHaveBeenCalledTimes(1);
      expect(snapshot).toHaveBeenCalledTimes(1);
      expect(flush.mock.invocationCallOrder[0]).toBeLessThan(snapshot.mock.invocationCallOrder[0]);
      expect(store.fetchedLeverage.DELISTED).toBeUndefined();
      if (settings) {
        expect(store.fetchedLeverage.BTCUSDT).toMatchObject({ buyLeverage: 12, sellLeverage: 12 });
      } else expect(store.fetchedLeverage).toEqual({});
      exchange.dispose();
    },
  );

  it('restores forwarded query-only leverage from the RPC bundle when engine SETTINGS is empty', async () => {
    const workerStore = new DefaultStore();
    const worker = new RustEngineExchange('polymarket', { key: '', secret: '' }, workerStore);
    const frame = fixture({ fills: 0, settings: false });
    receive(worker, frame);
    const rails = { buyLeverage: 7, sellLeverage: 9 };
    (worker as unknown as { applier: FrameApplier }).applier.applyLeverage('BTCUSDT', rails);
    installHost(
      worker,
      vi.fn(() => frame),
    );
    const snapshot = await worker.fetchEngineSnapshot();
    const settings = new Frame(snapshot.frame).settings();
    if (settings === undefined) throw new Error('Expected the SETTINGS snapshot');
    expect(settings.count).toBe(0);
    expect(snapshot.fetchedLeverage).toEqual({ BTCUSDT: rails });
    expect(snapshot.fetchedLeverage).not.toBe(workerStore.fetchedLeverage);
    expect(snapshot.fetchedLeverage.BTCUSDT).not.toBe(workerStore.fetchedLeverage.BTCUSDT);

    const store = new DefaultStore();
    const main = new BaseExchange('polymarket', { key: '', secret: '' }, store);
    main.applyWorkerFrame(frame);
    const applier = (main as unknown as { workerFrameApplier: FrameApplier }).workerFrameApplier;
    applier.applyLeverageSnapshot({ BTCUSDT: rails });
    const writer = vi.spyOn(applier, 'applyLeverageSnapshot');
    const rpc = vi.spyOn(main, 'callMethodInWorker').mockResolvedValue(snapshot as never);
    main.applyWorkerFrame(new ArrayBuffer(0));
    await settle();
    expect(rpc).toHaveBeenCalledExactlyOnceWith('fetchEngineSnapshot');
    expect(writer).toHaveBeenCalledExactlyOnceWith(snapshot.fetchedLeverage);
    expect(store.fetchedLeverage).toEqual({ BTCUSDT: rails });
    worker.dispose();
    main.dispose();
  });

  it('flushes changed engine SETTINGS before capturing a coherent frame and leverage map', async () => {
    const store = new DefaultStore();
    const exchange = new RustEngineExchange('binance', { key: '', secret: '' }, store);
    receive(exchange, fixture({ fills: 0 }));
    (exchange as unknown as { applier: FrameApplier }).applier.applyLeverage('DELISTED', {
      buyLeverage: 9,
      sellLeverage: 9,
    });
    const frame = fixture({ fills: 0, leverage: 12 });
    const flush = vi.fn(() => receive(exchange, frame));
    const snapshot = vi.fn(() => frame);
    (exchange as unknown as { host: unknown }).host = { flush, snapshot, dispose: vi.fn() };
    const result = await exchange.fetchEngineSnapshot();
    expect(flush).toHaveBeenCalledTimes(1);
    expect(snapshot).toHaveBeenCalledTimes(1);
    expect(flush.mock.invocationCallOrder[0]).toBeLessThan(snapshot.mock.invocationCallOrder[0]);
    const buyLeverage = new Frame(result.frame).settings()?.buyLeverage;
    if (buyLeverage === undefined) throw new Error('Expected the buy leverage column');
    expect(buyLeverage[0]).toBe(12);
    expect(result.fetchedLeverage.BTCUSDT.buyLeverage).toBe(12);
    expect(result.fetchedLeverage.BTCUSDT.sellLeverage).toBe(12);
    expect(result.fetchedLeverage.DELISTED).toBeUndefined();
    exchange.dispose();
  });

  it('flushes late-client snapshots before the worker forwards derived leverage so stale rails cannot overwrite fresh engine SETTINGS', () => {
    const store = new DefaultStore();
    const exchange = new RustEngineExchange('binance', { key: '', secret: '' }, store);
    receive(exchange, fixture({ fills: 0 }));
    const main = new BaseExchange('binance', { key: '', secret: '' }, new DefaultStore());
    const postMessage = vi.fn((message) => {
      if (message.op === 'frame') main.applyWorkerFrame(message.data);
      else if (message.op === 'store') main.store.update(message.data, true);
    });
    const worker = new ExchangeWorker(
      { postMessage } as unknown as Worker,
      'account',
      'binance',
      { key: '', secret: '' },
      {} as never,
      {} as never,
      'session',
    );
    const internal = worker as unknown as {
      _exchange: RustEngineExchange;
      _store: DefaultStore;
      _forwardsFrames: boolean;
    };
    internal._exchange = exchange;
    internal._store = store;
    internal._forwardsFrames = true;
    exchange.onFrame = (buffer) => postMessage({ op: 'frame', data: buffer });
    const frame = fixture({ fills: 0, leverage: 12 });
    const flush = vi.fn(() => receive(exchange, frame));
    const snapshot = vi.fn(() => frame);
    (exchange as unknown as { host: unknown }).host = { flush, snapshot, dispose: vi.fn() };
    worker.sendSnapshot();
    expect(flush).toHaveBeenCalledTimes(1);
    expect(snapshot).toHaveBeenCalledTimes(1);
    expect(store.fetchedLeverage.BTCUSDT.buyLeverage).toBe(12);
    expect(main.store.fetchedLeverage.BTCUSDT.buyLeverage).toBe(12);
    const lastMessage = postMessage.mock.calls.at(-1);
    if (lastMessage === undefined) throw new Error('Expected the worker snapshot message');
    expect(lastMessage[0]).toMatchObject({
      op: 'store',
      data: { fetchedLeverage: { BTCUSDT: { buyLeverage: 12, sellLeverage: 12 } } },
    });
    worker.dispose();
    main.dispose();
  });

  it('publishes KuCoin Spot leverage acknowledgements from engine SETTINGS and keeps them authoritative', async () => {
    const store = new DefaultStore();
    const exchange = new RustEngineExchange(
      'kucoinspot',
      { key: 'key', secret: 'secret', passphrase: 'fixture-passphrase' },
      store,
    );
    receive(exchange, fixture({ fills: 0 }));
    const command = vi.fn().mockResolvedValue(undefined);
    (exchange as unknown as { command: unknown }).command = command;
    // The engine confirms KuCoin Spot's fixed 1x rails in the frame the flush publishes.
    const flush = vi.fn(() => receive(exchange, fixture({ fills: 0, leverage: 1 })));
    (exchange as unknown as { host: unknown }).host = { flush, dispose: vi.fn() };
    const applier = (exchange as unknown as { applier: FrameApplier }).applier;
    const writer = vi.spyOn(applier, 'applyLeverage');
    await exchange.setLeverage('BTCUSDT', 20);
    expect(command).toHaveBeenCalledExactlyOnceWith({ op: 'setLeverage', symbol: 'BTCUSDT', leverage: 20 });
    expect(flush).toHaveBeenCalledOnce();
    expect(writer).not.toHaveBeenCalled();
    expect(store.fetchedLeverage.BTCUSDT).toMatchObject({ buyLeverage: 1, sellLeverage: 1 });
    receive(exchange, fixture({ fills: 0 }));
    expect(store.fetchedLeverage.BTCUSDT.buyLeverage).toBe(5);
    command.mockRejectedValueOnce(new Error('command failed'));
    await expect(exchange.setLeverage('BTCUSDT', 20)).rejects.toThrow('command failed');
    expect(flush).toHaveBeenCalledOnce();
    expect(store.fetchedLeverage.BTCUSDT.buyLeverage).toBe(5);
    exchange.dispose();
  });

  it('returns confirmed Binance leverage without replacing it with the market maximum and leaves unknown leverage absent', async () => {
    const store = new DefaultStore();
    const exchange = new RustEngineExchange('binance', { key: 'key', secret: 'secret' }, store);
    adoptEngineCapabilities(exchange);
    receive(exchange, fixture({ fills: 0 }));
    const confirmed = store.fetchedLeverage.BTCUSDT;
    const maxLeverage = vi.spyOn(exchange, 'fetchSymbolMaxLeverage').mockResolvedValue(100);
    const applier = (exchange as unknown as { applier: FrameApplier }).applier;
    const writer = vi.spyOn(applier, 'applyLeverage');
    const command = vi.fn();
    (exchange as unknown as { command: unknown }).command = command;
    const flush = vi.fn();
    (exchange as unknown as { host: unknown }).host = { flush, dispose: vi.fn() };

    expect(await exchange.fetchLeverageForSymbol('BTCUSDT')).toBe(confirmed);
    expect(store.fetchedLeverage.BTCUSDT.buyLeverage).toBe(5);
    expect(store.fetchedLeverage.BTCUSDT.sellLeverage).toBe(5);
    expect(flush).toHaveBeenCalledTimes(1);
    store.update({ fetchedLeverage: {} });
    expect(await exchange.fetchLeverageForSymbol('BTCUSDT')).toBeNull();
    expect(store.fetchedLeverage.BTCUSDT).toBeUndefined();
    expect(flush).toHaveBeenCalledTimes(2);
    flush.mockImplementationOnce(() => receive(exchange, fixture({ fills: 0 })));
    expect(await exchange.fetchLeverageForSymbol('BTCUSDT')).toBe(store.fetchedLeverage.BTCUSDT);
    expect(store.fetchedLeverage.BTCUSDT.buyLeverage).toBe(5);
    expect(flush).toHaveBeenCalledTimes(3);
    expect(maxLeverage).not.toHaveBeenCalled();
    expect(command).not.toHaveBeenCalled();
    expect(writer).not.toHaveBeenCalled();
    exchange.dispose();
  });

  it.each([
    [NaN, NaN],
    [NaN, 5],
    [5, NaN],
    [Infinity, 5],
    [5, Infinity],
    [0, 5],
    [5, 0],
    [-1, 5],
    [5, -1],
  ])('does not return KuCoin cached leverage %s/%s as confirmed account rails', async (buyLeverage, sellLeverage) => {
    const store = new DefaultStore();
    const exchange = new RustEngineExchange('kucoin', { key: '', secret: '' }, store);
    adoptEngineCapabilities(exchange);
    receive(exchange, fixture({ fills: 0, leverage: NaN, buyLeverage, sellLeverage }));
    const cached = store.fetchedLeverage.BTCUSDT;
    const maximum = vi.spyOn(exchange, 'fetchSymbolMaxLeverage');
    const writer = vi.spyOn((exchange as unknown as { applier: FrameApplier }).applier, 'applyLeverage');
    expect(await exchange.fetchLeverageForSymbol('BTCUSDT')).toBeNull();
    expect(store.fetchedLeverage.BTCUSDT).toBe(cached);
    expect(maximum).not.toHaveBeenCalled();
    expect(writer).not.toHaveBeenCalled();
    exchange.dispose();
  });

  it.each(['polymarket', 'mexc'] as const)(
    'routes %s leverage refresh through the applier and keeps SETTINGS authoritative',
    async (venue) => {
      const store = new DefaultStore();
      const exchange = new RustEngineExchange(venue, { key: 'key', secret: 'secret' }, store);
      adoptEngineCapabilities(exchange);
      // The metadata refresh waits for the engine's own markets, which only a frame's status can say.
      receive(exchange, fixture({ fills: 0, loaded: LOADED_FLAG.MARKETS }));
      const rails = { buyLeverage: 7, sellLeverage: 9 };
      const command = vi.fn().mockResolvedValue(rails);
      (exchange as unknown as { command: unknown }).command = command;
      const applier = (exchange as unknown as { applier: FrameApplier }).applier;
      const writer = vi.spyOn(applier, 'applyLeverage');
      if (venue === 'mexc') await exchange.updateSymbolMetadata('BTCUSDT');
      else await exchange.fetchLeverageForSymbol('BTCUSDT');
      expect(writer).toHaveBeenCalledExactlyOnceWith('BTCUSDT', rails);
      expect(store.fetchedLeverage.BTCUSDT).toEqual(rails);
      expect(command).toHaveBeenCalledExactlyOnceWith({ op: 'fetchLeverage', symbol: 'BTCUSDT' }, { report: false });
      applier.applyLeverage('DELISTED', rails);
      receive(exchange, fixture({ fills: 0 }));
      expect(store.fetchedLeverage.BTCUSDT.buyLeverage).toBe(5);
      expect(store.fetchedLeverage.DELISTED).toBeUndefined();
      exchange.dispose();
    },
  );
});

describe('direct public-data rejection observers', () => {
  beforeEach(() => vi.spyOn(console, 'error').mockImplementation(() => {}));
  afterEach(() => vi.restoreAllMocks());

  it.each(['public', 'fill', 'book', 'candle'] as const)(
    'stops sibling native deliveries when a %s callback disposes the exchange',
    (kind) => {
      const exchange = new RustEngineExchange('bybit', { key: '', secret: '' }, new DefaultStore());
      const deliveredAfterDisposal: string[] = [];
      const deliver = (stream: string) => {
        if (exchange.isDisposed) deliveredAfterDisposal.push(stream);
        else if (stream === kind) exchange.dispose();
      };
      exchange.observePublicFeed({ updated: () => deliver('public'), closed: vi.fn() });
      exchange.on('fill', () => deliver('fill'));
      const stopBook = exchange.listenOrderBook('BTCUSDT', () => deliver('book'));
      const stopPeerBook = exchange.listenOrderBook('BTCUSDT', () => deliver('peer book'));
      const stopCandle = exchange.listenOHLCV({ symbol: 'BTCUSDT', interval: '1m' }, () => deliver('candle'));
      const stopPeerCandle = exchange.listenOHLCV({ symbol: 'BTCUSDT', interval: '1m' }, () => deliver('peer candle'));
      const forward = vi.fn();
      exchange.framesCarryMarketData = false;
      exchange.onFrame = forward;
      try {
        receive(exchange, fixture());
        expect(exchange.isDisposed).toBe(true);
        expect(deliveredAfterDisposal).toEqual([]);
        expect(forward).not.toHaveBeenCalled();
      } finally {
        stopBook();
        stopPeerBook();
        stopCandle();
        stopPeerCandle();
        exchange.dispose();
      }
    },
  );

  it.each(['book', 'candle', 'frame'] as const)(
    'stops the native frame when a %s rejection observer disposes the exchange',
    (kind) => {
      const exchange = new RustEngineExchange('bybit', { key: '', secret: '' }, new DefaultStore());
      const deliveredAfterDisposal: string[] = [];
      const frameDelivered = vi.fn();
      exchange.framesCarryMarketData = false;
      exchange.onFrame = frameDelivered;
      const record = (stream: string) => {
        if (exchange.isDisposed) deliveredAfterDisposal.push(stream);
      };
      const rejected = vi.fn(() => exchange.dispose());
      const stopCandle = exchange.listenOHLCV(
        { symbol: 'BTCUSDT', interval: '1m' },
        () => record('candle'),
        kind === 'candle' ? rejected : () => record('candle rejection'),
      );
      const stopBook = exchange.listenOrderBook(
        'BTCUSDT',
        () => record('book'),
        kind !== 'candle' ? rejected : () => record('book rejection'),
      );
      exchange.emitter.on('fill', () => record('fill'));
      try {
        receive(
          exchange,
          kind === 'frame'
            ? new ArrayBuffer(0)
            : fixture({ invalidBook: kind === 'book', invalidCandle: kind === 'candle' }),
        );
        expect(rejected).toHaveBeenCalledTimes(1);
        expect(exchange.isDisposed).toBe(true);
        expect(deliveredAfterDisposal).toEqual([]);
        expect(frameDelivered).not.toHaveBeenCalled();
      } finally {
        stopBook();
        stopCandle();
        exchange.dispose();
      }
    },
  );

  it.each([
    ['book', true],
    ['candle', true],
    ['book', false],
    ['candle', false],
  ] as const)('retires a %s unsubscribe during registration: %s', (kind, duringRegistration) => {
    const source = new RustEngineExchange('bybit', { key: '', secret: '' }, new DefaultStore());
    installHost(source, vi.fn());
    receive(source, fixture({ fills: 0 }));
    let handler: OrderBookSubscriptionsHandler | OHLCVSubscriptionsHandler;
    const messages: Array<{ op: string }> = [];
    const context = {
      postMessage(message: { op: string }) {
        messages.push(message);
        if (message.op.endsWith(':rejected')) {
          handler.handle({
            op: kind === 'book' ? 'unsubscribeOrderBook' : 'unsubscribeOHLCV',
            data: { subId: 'cancel-during-registration' },
          });
        }
      },
    } as unknown as Worker;
    const release = vi.fn();
    const register = (stop: () => void) => {
      if (duringRegistration) receive(source, new ArrayBuffer(0));
      return () => {
        release();
        // Cleanup itself may synchronously report a final rejection.
        receive(source, new ArrayBuffer(0));
        stop();
      };
    };
    if (kind === 'book') {
      const original = source.listenOrderBook;
      vi.spyOn(source, 'listenOrderBook').mockImplementation((...args) => register(original(...args)));
      handler = new OrderBookSubscriptionsHandler('synthetic', 'bybit', source, context, {} as never, {} as never);
    } else {
      const original = source.listenOHLCV;
      vi.spyOn(source, 'listenOHLCV').mockImplementation((...args) => register(original(...args)));
      handler = new OHLCVSubscriptionsHandler('synthetic', 'bybit', source, context, {} as never, {} as never);
    }
    try {
      handler.handle({
        op: kind === 'book' ? 'subscribeOrderBook' : 'subscribeOHLCV',
        data: { symbol: 'BTCUSDT', interval: '1m', subId: 'cancel-during-registration' },
      });
      if (!duringRegistration) receive(source, new ArrayBuffer(0));
      const afterCancellation = messages.length;
      const releasedAtCancellation = release.mock.calls.length;
      receive(source, new ArrayBuffer(0));
      receive(source, fixture({ fills: 0 }));
      expect({ released: releasedAtCancellation, extraMessages: messages.length - afterCancellation }).toEqual({
        released: 1,
        extraMessages: 0,
      });
      expect(messages).toHaveLength(1);
      handler.dispose();
      expect(release).toHaveBeenCalledTimes(1);
    } finally {
      handler.dispose();
      source.dispose();
    }
  });

  it.each(['book', 'candle'] as const)(
    'keeps a replacement %s subscription created inside rejection delivery',
    (kind) => {
      const source = new RustEngineExchange('bybit', { key: '', secret: '' }, new DefaultStore());
      installHost(source, vi.fn());
      receive(source, fixture({ fills: 0 }));
      let handler: OrderBookSubscriptionsHandler | OHLCVSubscriptionsHandler;
      let replaced = false;
      const subscribe = () =>
        handler.handle({
          op: kind === 'book' ? 'subscribeOrderBook' : 'subscribeOHLCV',
          data: { symbol: 'BTCUSDT', interval: '1m', subId: 'replacement-sub' },
        });
      const unsubscribe = () =>
        handler.handle({
          op: kind === 'book' ? 'unsubscribeOrderBook' : 'unsubscribeOHLCV',
          data: { subId: 'replacement-sub' },
        });
      const messages: Array<{ op: string }> = [];
      const context = {
        postMessage(message: { op: string }) {
          messages.push(message);
          if (!replaced && message.op.endsWith(':rejected')) {
            replaced = true;
            unsubscribe();
            subscribe();
          }
        },
      } as unknown as Worker;
      let registrations = 0;
      const releases = [vi.fn(), vi.fn()];
      const register = (stop: () => void) => {
        const index = registrations++;
        if (index === 0) receive(source, new ArrayBuffer(0));
        return () => {
          releases[index]();
          stop();
        };
      };
      if (kind === 'book') {
        const original = source.listenOrderBook;
        vi.spyOn(source, 'listenOrderBook').mockImplementation((...args) => register(original(...args)));
        handler = new OrderBookSubscriptionsHandler('synthetic', 'bybit', source, context, {} as never, {} as never);
      } else {
        const original = source.listenOHLCV;
        vi.spyOn(source, 'listenOHLCV').mockImplementation((...args) => register(original(...args)));
        handler = new OHLCVSubscriptionsHandler('synthetic', 'bybit', source, context, {} as never, {} as never);
      }
      try {
        subscribe();
        expect(registrations).toBe(2);
        expect(releases.map((release) => release.mock.calls.length)).toEqual([1, 0]);
        const beforeRejection = messages.length;
        receive(source, new ArrayBuffer(0));
        expect(messages.length - beforeRejection).toBe(1);
        unsubscribe();
        expect(releases.map((release) => release.mock.calls.length)).toEqual([1, 1]);
        const afterCancellation = messages.length;
        receive(source, new ArrayBuffer(0));
        receive(source, fixture({ fills: 0 }));
        expect(messages).toHaveLength(afterCancellation);
        handler.dispose();
        expect(releases.map((release) => release.mock.calls.length)).toEqual([1, 1]);
      } finally {
        handler.dispose();
        source.dispose();
      }
    },
  );

  it.each(['book', 'candle'] as const)('allows a %s retry after source registration throws', (kind) => {
    const source = new RustEngineExchange('bybit', { key: '', secret: '' }, new DefaultStore());
    installHost(source, vi.fn());
    receive(source, fixture({ fills: 0 }));
    const postMessage = vi.fn();
    const context = { postMessage } as unknown as Worker;
    const failure = new Error('source registration refused');
    const listen = kind === 'book' ? vi.spyOn(source, 'listenOrderBook') : vi.spyOn(source, 'listenOHLCV');
    listen.mockImplementationOnce(() => {
      throw failure;
    });
    const handler =
      kind === 'book'
        ? new OrderBookSubscriptionsHandler('synthetic', 'bybit', source, context, {} as never, {} as never)
        : new OHLCVSubscriptionsHandler('synthetic', 'bybit', source, context, {} as never, {} as never);
    const subscribe = () =>
      handler.handle({
        op: kind === 'book' ? 'subscribeOrderBook' : 'subscribeOHLCV',
        data: { symbol: 'BTCUSDT', interval: '1m', subId: 'retry-sub' },
      });
    try {
      expect(subscribe).toThrow(failure);
      expect(subscribe).not.toThrow();
      expect(listen).toHaveBeenCalledTimes(2);
      receive(source, new ArrayBuffer(0));
      expect(postMessage).toHaveBeenCalledTimes(1);
    } finally {
      handler.dispose();
      source.dispose();
    }
  });

  it.each([
    ['book', true],
    ['candle', true],
    ['book', false],
    ['candle', false],
  ] as const)('fences %s callbacks before registration returns after provider disposal: %s', (kind, dispose) => {
    const source = new RustEngineExchange('bybit', { key: '', secret: '' }, new DefaultStore());
    const parent = new BaseExchange('bybit', { key: '', secret: '' }, new DefaultStore());
    const peerData = vi.fn();
    const peerRejections = vi.fn();
    const stopPeer =
      kind === 'book'
        ? source.listenOrderBook('BTCUSDT', peerData, peerRejections)
        : source.listenOHLCV({ symbol: 'BTCUSDT', interval: '1m' }, peerData, peerRejections);
    const release = vi.fn();
    const register = (unsubscribe: () => void) => {
      receive(source, new ArrayBuffer(0));
      receive(source, fixture({ fills: 0 }));
      receive(source, new ArrayBuffer(0));
      return () => {
        release();
        unsubscribe();
      };
    };
    const provider = new InProcessPublicDataProvider(parent, {
      store: source.store,
      listenOrderBook: (...args: Parameters<typeof source.listenOrderBook>) =>
        register(source.listenOrderBook(...args)),
      listenOHLCV: (...args: Parameters<typeof source.listenOHLCV>) => register(source.listenOHLCV(...args)),
    });
    parent.externalPublicDataProvider = provider;
    const states: boolean[] = [];
    const observed = () => {
      states.push(parent.isDisposed);
      if (dispose) parent.dispose();
    };
    const data = vi.fn();
    let stop = () => {};
    try {
      stop =
        kind === 'book'
          ? parent.listenOrderBook('BTCUSDT', data, observed)
          : parent.listenOHLCV({ symbol: 'BTCUSDT', interval: '1m' }, data, observed);
      expect(states).toEqual(dispose ? [false] : [false, false]);
      expect(data).toHaveBeenCalledTimes(dispose ? 0 : 1);
      expect(peerRejections).toHaveBeenCalledTimes(2);
      expect(peerData).toHaveBeenCalledTimes(1);
      expect(release).toHaveBeenCalledTimes(dispose ? 1 : 0);
      stop();
      stop();
      provider.dispose();
      receive(source, new ArrayBuffer(0));
      receive(source, fixture({ fills: 0 }));
      expect(states).toEqual(dispose ? [false] : [false, false]);
      expect(data).toHaveBeenCalledTimes(dispose ? 0 : 1);
      expect(release).toHaveBeenCalledTimes(1);
    } finally {
      stop();
      stopPeer();
      parent.dispose();
      source.dispose();
    }
  });

  it.each([
    ['book', 'after registration'],
    ['candle', 'after registration'],
    ['book', 'during registration'],
    ['candle', 'during registration'],
    ['book', 'during release'],
    ['candle', 'during release'],
  ] as const)('retires the %s handler subscription after synchronous disposal (%s)', (kind, rejectionAt) => {
    const source = new RustEngineExchange('bybit', { key: '', secret: '' }, new DefaultStore());
    installHost(source, vi.fn());
    receive(source, fixture({ fills: 0 }));
    let handler: OrderBookSubscriptionsHandler | OHLCVSubscriptionsHandler;
    const messages: Array<{ op: string }> = [];
    const context = {
      postMessage(message: { op: string }) {
        messages.push(message);
        if (message.op.endsWith(':rejected')) handler.dispose();
      },
    } as unknown as Worker;
    const release = vi.fn();
    const rejectDuringRegistration = (stop: () => void) => {
      receive(source, new ArrayBuffer(0));
      if (rejectionAt === 'during registration') receive(source, new ArrayBuffer(0));
      return () => {
        release();
        if (rejectionAt === 'during release') receive(source, new ArrayBuffer(0));
        stop();
      };
    };
    if (kind === 'book') {
      const original = source.listenOrderBook;
      vi.spyOn(source, 'listenOrderBook').mockImplementation((...args) => rejectDuringRegistration(original(...args)));
      handler = new OrderBookSubscriptionsHandler('synthetic', 'bybit', source, context, {} as never, {} as never);
    } else {
      const original = source.listenOHLCV;
      vi.spyOn(source, 'listenOHLCV').mockImplementation((...args) => rejectDuringRegistration(original(...args)));
      handler = new OHLCVSubscriptionsHandler('synthetic', 'bybit', source, context, {} as never, {} as never);
    }
    try {
      handler.handle({
        op: kind === 'book' ? 'subscribeOrderBook' : 'subscribeOHLCV',
        data: { symbol: 'BTCUSDT', interval: '1m', subId: 'synthetic-sub' },
      });
      const afterDisposal = messages.length;
      const releasedAtDisposal = release.mock.calls.length;
      receive(source, new ArrayBuffer(0));
      expect({ released: releasedAtDisposal, extraMessages: messages.length - afterDisposal }).toEqual({
        released: 1,
        extraMessages: 0,
      });
      expect(messages.filter((message) => message.op.endsWith(':rejected'))).toHaveLength(1);
      handler.dispose();
      expect(release).toHaveBeenCalledTimes(1);
    } finally {
      handler.dispose();
      source.dispose();
    }
  });

  it.each(['book', 'candle'] as const)(
    'releases the %s source handle exactly once during registration disposal',
    (kind) => {
      const parent = new BaseExchange('bybit', { key: '', secret: '' }, new DefaultStore());
      const release = vi.fn();
      const source = {
        store: parent.store,
        listenOrderBook: (_symbol: string, _data: unknown, rejected?: (signal: OrderBookFrameRejection) => void) => {
          rejected?.({ stream: 'orderBook', symbol: 'BTCUSDT', error: new Error('synthetic book refusal') });
          return release;
        },
        listenOHLCV: (_opts: unknown, _data: unknown, rejected?: (signal: OHLCVFrameRejection) => void) => {
          rejected?.({
            stream: 'ohlcv',
            symbol: 'BTCUSDT',
            interval: '1m',
            error: new Error('synthetic candle refusal'),
          });
          return release;
        },
      };
      const provider = new InProcessPublicDataProvider(parent, source);
      parent.externalPublicDataProvider = provider;
      const observed = vi.fn(() => parent.dispose());
      try {
        const stop =
          kind === 'book'
            ? parent.listenOrderBook('BTCUSDT', () => {}, observed)
            : parent.listenOHLCV({ symbol: 'BTCUSDT', interval: '1m' }, () => {}, observed);
        expect(observed).toHaveBeenCalledTimes(1);
        expect(parent.isDisposed).toBe(true);
        expect(release).toHaveBeenCalledTimes(1);
        stop();
        stop();
        provider.dispose();
        expect(release).toHaveBeenCalledTimes(1);
      } finally {
        parent.dispose();
      }
    },
  );

  it.each(['book', 'candle'] as const)(
    'ends native %s data and rejection delivery after registration disposal',
    (kind) => {
      const source = new RustEngineExchange('bybit', { key: '', secret: '' }, new DefaultStore());
      const parent = new BaseExchange('bybit', { key: '', secret: '' }, new DefaultStore());
      const peerData = vi.fn();
      const peerRejections = vi.fn();
      const stopPeer =
        kind === 'book'
          ? source.listenOrderBook('BTCUSDT', peerData, peerRejections)
          : source.listenOHLCV({ symbol: 'BTCUSDT', interval: '1m' }, peerData, peerRejections);
      const provider = new InProcessPublicDataProvider(parent, {
        store: source.store,
        listenOrderBook: (...args: Parameters<typeof source.listenOrderBook>) => {
          const stop = source.listenOrderBook(...args);
          receive(source, new ArrayBuffer(0));
          return stop;
        },
        listenOHLCV: (...args: Parameters<typeof source.listenOHLCV>) => {
          const stop = source.listenOHLCV(...args);
          receive(source, new ArrayBuffer(0));
          return stop;
        },
      });
      parent.externalPublicDataProvider = provider;
      const observed = vi.fn(() => parent.dispose());
      const data = vi.fn();
      let stop = () => {};
      try {
        stop =
          kind === 'book'
            ? parent.listenOrderBook('BTCUSDT', data, observed)
            : parent.listenOHLCV({ symbol: 'BTCUSDT', interval: '1m' }, data, observed);
        expect(parent.isDisposed).toBe(true);
        expect(observed).toHaveBeenCalledTimes(1);
        receive(source, new ArrayBuffer(0));
        expect(observed).toHaveBeenCalledTimes(1);
        expect(peerRejections).toHaveBeenCalledTimes(2);
        receive(source, fixture({ fills: 0 }));
        expect(data).not.toHaveBeenCalled();
        expect(peerData).toHaveBeenCalledTimes(1);
        stop();
        stop();
        receive(source, new ArrayBuffer(0));
        expect(observed).toHaveBeenCalledTimes(1);
      } finally {
        stop();
        stopPeer();
        parent.dispose();
        source.dispose();
      }
    },
  );

  it.each(['book', 'candle', 'envelope', 'identity'] as const)(
    'scopes native %s failures and keeps two-argument data delivery',
    (failure) => {
      const exchange = new RustEngineExchange('bybit', { key: '', secret: '' }, new DefaultStore());
      installHost(exchange, vi.fn());
      const book = vi.fn<(signal: OrderBookFrameRejection) => void>();
      const candle = vi.fn<(signal: OHLCVFrameRejection) => void>();
      const otherBook = vi.fn();
      const otherCandle = vi.fn();
      const data = vi.fn();
      const candles = vi.fn();
      const stopBook = exchange.listenOrderBook('BTCUSDT', data, book);
      const stopCandle = exchange.listenOHLCV({ symbol: 'BTCUSDT', interval: '1m' }, candles, candle);
      exchange.listenOrderBook('ETHUSDT', () => {}, otherBook);
      exchange.listenOHLCV({ symbol: 'BTCUSDT', interval: '5m' }, () => {}, otherCandle);
      const legacyBook = vi.fn();
      const legacyCandle = vi.fn();
      exchange.listenOrderBook('BTCUSDT', legacyBook);
      exchange.listenOHLCV({ symbol: 'BTCUSDT', interval: '1m' }, legacyCandle);
      try {
        receive(exchange, fixture({ fills: 0, secondaryMarket: true }));
        expect(data).toHaveBeenCalledTimes(1);
        expect(legacyBook).toHaveBeenCalledWith(data.mock.calls[0][0]);
        expect(legacyCandle).toHaveBeenCalledWith(candles.mock.calls[0][0]);
        const malformed =
          failure === 'envelope'
            ? new ArrayBuffer(0)
            : fixture({
                fills: 0,
                secondaryMarket: true,
                invalidBook: failure === 'book',
                invalidCandle: failure === 'candle',
                unknownIdentity: failure === 'identity',
              });
        receive(exchange, malformed);
        expect(data).toHaveBeenCalledTimes(failure === 'candle' ? 2 : 1);
        expect(candles).toHaveBeenCalledTimes(failure === 'book' ? 2 : 1);
        if (failure !== 'candle')
          expect(book).toHaveBeenCalledExactlyOnceWith({
            stream: 'orderBook',
            symbol: 'BTCUSDT',
            error: expect.any(Error),
          });
        else expect(book).not.toHaveBeenCalled();
        if (failure !== 'book')
          expect(candle).toHaveBeenCalledExactlyOnceWith({
            stream: 'ohlcv',
            symbol: 'BTCUSDT',
            interval: '1m',
            error: expect.any(Error),
          });
        else expect(candle).not.toHaveBeenCalled();
        expect(otherBook).toHaveBeenCalledTimes(Number(failure === 'envelope' || failure === 'identity'));
        expect(otherCandle).toHaveBeenCalledTimes(Number(failure === 'envelope' || failure === 'identity'));
        stopBook();
        stopCandle();
        const counts = [book.mock.calls.length, candle.mock.calls.length];
        receive(exchange, new ArrayBuffer(0));
        expect([book.mock.calls.length, candle.mock.calls.length]).toEqual(counts);
        exchange.dispose();
        receive(exchange, new ArrayBuffer(0));
        expect([book.mock.calls.length, candle.mock.calls.length]).toEqual(counts);
      } finally {
        exchange.dispose();
      }
    },
  );

  it('isolates a throwing direct observer from the other subscriptions', () => {
    const exchange = new RustEngineExchange('bybit', { key: '', secret: '' }, new DefaultStore());
    exchange.listenOrderBook(
      'BTCUSDT',
      () => {},
      () => {
        throw new Error('synthetic observer failure');
      },
    );
    const observed = vi.fn();
    exchange.listenOrderBook('ETHUSDT', () => {}, observed);
    try {
      expect(() => receive(exchange, new ArrayBuffer(0))).not.toThrow();
      expect(observed).toHaveBeenCalledExactlyOnceWith({
        stream: 'orderBook',
        symbol: 'ETHUSDT',
        error: expect.any(Error),
      });
    } finally {
      exchange.dispose();
    }
  });

  it('carries native rejections across JSON worker messages to each subscription and releases them', () => {
    const source = new RustEngineExchange('bybit', { key: '', secret: '' }, new DefaultStore());
    const main = new BaseExchange('bybit', { key: '', secret: '' }, new DefaultStore());
    installHost(source, vi.fn());
    receive(source, fixture({ fills: 0 }));
    const messages: Array<{ op: string; data: unknown }> = [];
    const context = {
      postMessage(message: { op: string; data: unknown }) {
        const transported = JSON.parse(JSON.stringify(message));
        messages.push(transported);
        main.emitter.emit(transported.op, transported.data);
      },
    } as unknown as Worker;
    const bookHandler = new OrderBookSubscriptionsHandler(
      'synthetic',
      'bybit',
      source,
      context,
      {} as never,
      {} as never,
    );
    const candleHandler = new OHLCVSubscriptionsHandler(
      'synthetic',
      'bybit',
      source,
      context,
      {} as never,
      {} as never,
    );
    for (const op of ['subscribeOrderBook', 'unsubscribeOrderBook'] as const)
      main.on(op, (data: unknown) => bookHandler.handle({ op, data }));
    for (const op of ['subscribeOHLCV', 'unsubscribeOHLCV'] as const)
      main.on(op, (data: unknown) => candleHandler.handle({ op, data }));
    const book = vi.fn();
    const secondBook = vi.fn();
    const candle = vi.fn();
    const data = vi.fn();
    const candles = vi.fn();
    const stop = main.listenOrderBookWebworker('BTCUSDT', data, book);
    main.listenOrderBookWebworker('BTCUSDT', () => {}, secondBook);
    const stopCandle = main.listenOHLCVWebworker({ symbol: 'BTCUSDT', interval: '1m' }, candles, candle);
    try {
      receive(source, fixture({ fills: 0 }));
      expect(data).toHaveBeenCalledTimes(1);
      expect(candles).toHaveBeenCalledTimes(1);
      receive(source, fixture({ fills: 0, invalidBook: true }));
      for (const listener of [book, secondBook]) expect(listener).toHaveBeenCalledTimes(1);
      expect(candle).not.toHaveBeenCalled();
      receive(source, fixture({ fills: 0, invalidCandle: true }));
      expect(candle).toHaveBeenCalledTimes(1);
      for (const listener of [book, secondBook]) expect(listener).toHaveBeenCalledTimes(1);
      for (const listener of [book, secondBook, candle]) listener.mockClear();
      messages.length = 0;
      receive(source, new ArrayBuffer(0));
      for (const listener of [book, secondBook])
        expect(listener).toHaveBeenCalledExactlyOnceWith({
          stream: 'orderBook',
          symbol: 'BTCUSDT',
          error: expect.any(Error),
        });
      expect(candle).toHaveBeenCalledExactlyOnceWith({
        stream: 'ohlcv',
        symbol: 'BTCUSDT',
        interval: '1m',
        error: expect.any(Error),
      });
      expect(book.mock.calls[0][0].error.message).toBeTruthy();
      expect(messages.filter((m) => m.op === 'listenOrderBook:rejected')).toHaveLength(2);
      stop();
      receive(source, new ArrayBuffer(0));
      expect(book).toHaveBeenCalledTimes(1);
      expect(secondBook).toHaveBeenCalledTimes(2);
      expect(candle).toHaveBeenCalledTimes(2);
      stopCandle();
      stopCandle();
      receive(source, new ArrayBuffer(0));
      expect(candle).toHaveBeenCalledTimes(2);
      expect(secondBook).toHaveBeenCalledTimes(3);
      const queuedRejections = messages.filter((message) => message.op.endsWith(':rejected'));
      for (const message of queuedRejections) main.emitter.emit(message.op, message.data);
      expect(book).toHaveBeenCalledTimes(1);
      expect(candle).toHaveBeenCalledTimes(2);
      main.dispose();
      const beforeDisposal = secondBook.mock.calls.length;
      for (const message of queuedRejections) main.emitter.emit(message.op, message.data);
      expect(secondBook).toHaveBeenCalledTimes(beforeDisposal);
      bookHandler.dispose();
      candleHandler.dispose();
      const before = messages.length;
      receive(source, new ArrayBuffer(0));
      expect(messages).toHaveLength(before);
    } finally {
      bookHandler.dispose();
      candleHandler.dispose();
      source.dispose();
      main.dispose();
    }
  });
});

it('registers worker rejection observers before synchronous subscription replies', () => {
  const main = new BaseExchange('bybit', { key: '', secret: '' }, new DefaultStore());
  const observed = vi.fn();
  main.on('subscribeOrderBook', ({ symbol, subId }: { symbol: string; subId: string }) =>
    main.emitter.emit('listenOrderBook:rejected', {
      stream: 'orderBook',
      symbol,
      subId,
      error: { message: 'synthetic book refusal' },
    }),
  );
  main.on('subscribeOHLCV', ({ symbol, interval, subId }: { symbol: string; interval: string; subId: string }) =>
    main.emitter.emit('listenOHLCV:rejected', {
      stream: 'ohlcv',
      symbol,
      interval,
      subId,
      error: { message: 'synthetic candle refusal' },
    }),
  );
  try {
    main.listenOrderBookWebworker('BTCUSDT', () => {}, observed);
    main.listenOHLCVWebworker({ symbol: 'BTCUSDT', interval: '1m' }, () => {}, observed);
    expect(observed).toHaveBeenCalledTimes(2);
    expect(observed.mock.calls[0][0].error).toBeInstanceOf(Error);
    expect(observed.mock.calls[1][0].error.message).toBe('synthetic candle refusal');
  } finally {
    main.dispose();
  }
});

it('forwards PublicDataSource rejection observers and removes them when its provider is disposed', () => {
  const source = new RustEngineExchange('bybit', { key: '', secret: '' }, new DefaultStore());
  const parent = new BaseExchange('bybit', { key: '', secret: '' }, new DefaultStore());
  const provider = new InProcessPublicDataProvider(parent, source);
  parent.externalPublicDataProvider = provider;
  const book = vi.fn();
  const candle = vi.fn();
  vi.spyOn(console, 'error').mockImplementation(() => {});
  try {
    const stop = parent.listenOrderBook('BTCUSDT', () => {}, book);
    parent.listenOHLCV({ symbol: 'BTCUSDT', interval: '1m' }, () => {}, candle);
    receive(source, new ArrayBuffer(0));
    expect(book).toHaveBeenCalledTimes(1);
    expect(candle).toHaveBeenCalledTimes(1);
    stop();
    stop();
    receive(source, new ArrayBuffer(0));
    expect(book).toHaveBeenCalledTimes(1);
    expect(candle).toHaveBeenCalledTimes(2);
    parent.dispose();
    receive(source, new ArrayBuffer(0));
    expect(candle).toHaveBeenCalledTimes(2);
  } finally {
    parent.dispose();
    source.dispose();
    vi.restoreAllMocks();
  }
});

it('invalid subscription diagnostics preserve financial peers and book identity', () => {
  const store = new DefaultStore();
  const applier = new FrameApplier(store);
  applier.apply(new Frame(fixture()));
  const bad = new Frame(fixture({ invalidStats: true, total: 99 }));
  expect(() => applier.apply(bad)).toThrow('invalid message handler stats mode');
  expect(applier.hasMarketIdentity).toBe(true);
  expect(applier.hasAccountState).toBe(true);
  expect(applier.hasTickerState).toBe(true);
  expect(store.orders[0]).toMatchObject({ id: 'order-1', symbol: 'BTCUSDT' });
  expect(store.positions[0]).toMatchObject({ symbol: 'BTCUSDT' });
  expect(store.tickers[0]).toMatchObject({ symbol: 'BTCUSDT', bid: 99, ask: 101 });
  expect(store.balance.total).toBe(99);
  expect(applier.books(bad)[0].orderbook.bids[0].price).toBe(100);
  applier.apply(new Frame(fixture({ markets: false, total: 101 })));
  expect(applier.hasMarketIdentity).toBe(true);
  expect(store.balance.total).toBe(101);
});
