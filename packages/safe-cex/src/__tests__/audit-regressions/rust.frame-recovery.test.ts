vi.mock('../../exchanges/rust/betaVenues', async (importOriginal) => {
  const { retainedRustPolicy } = await import('../reference/retainedRustPolicy');
  return retainedRustPolicy(await importOriginal<typeof import('../../exchanges/rust/betaVenues')>());
});

import type { CallableMethodsInWorker, EngineSnapshot } from '../../exchanges/base';
import type { OrderFillEvent } from '../../types';

import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import { Frame, LOADED_FLAG, SECTION } from '@tealstreet/safe-cex-wasm/wire';
import { BaseExchange } from '../../exchanges/base';
import { RustEngineExchange } from '../../exchanges/rust/rust.exchange';
import { FrameApplier } from '../../store/frameApplier';
import { DefaultStore } from '../../store/store.base';
import { ExchangeWorker } from '../../webworker/ExchangeWorker';
import { CallMethodHandler } from '../../webworker/handlers/CallMethodHandler';
import { adoptEngineCapabilities } from '../fixtures/engineCapabilities';

const align8 = (value: number) => Math.ceil(value / 8) * 8;
type FixtureOptions = {
  brokenBook?: boolean;
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
  brokenBook = false,
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
  fillSymbol,
  loaded,
}: FixtureOptions = {}) => {
  const strings = ['BTCUSDT', 'BTC', 'USDT', 'order-1', '1m', 'ETHUSDT', 'ETH'];
  const sections: Array<{ kind: number; bytes: Uint8Array; revision?: number }> = [];
  const table = (kind: number, words: number[], numbers: number[], tail: number[] = []) => {
    const numericOffset = align8(words.length * 4);
    const bytes = new Uint8Array(align8(numericOffset + numbers.length * 8 + tail.length * 4));
    const view = new DataView(bytes.buffer);
    words.forEach((word, i) => view.setUint32(i * 4, word, true));
    numbers.forEach((number, i) => view.setFloat64(numericOffset + i * 8, number, true));
    tail.forEach((word, i) => view.setUint32(numericOffset + numbers.length * 8 + i * 4, word, true));
    sections.push({ kind, bytes, revision: tail.length ? 1 : 0 });
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
  table(SECTION.ORDERS, [1, 3, 0xffffffff, 0xffffffff, 0, 256], [100, 1, 0, 1, 1234, NaN, NaN]);
  table(SECTION.POSITIONS, [1, 0, 2 << 16], [100, 50, 5, 0, 0, 0.5, 80, 10, 1234]);
  table(SECTION.BALANCE, [], [total - 30, 30, total, 0]);
  if (!omitSettings)
    table(SECTION.SETTINGS, settings ? [1, 0, 2] : [0], settings ? [leverage, buyLeverage, sellLeverage] : []);
  table(
    SECTION.FILLS,
    [fills, ...Array(fills).fill(3), ...Array(fills).fill(0), ...Array(fills).fill(0)],
    Array.from({ length: fills }, () => [100, 0.25]).flat(),
    fillSymbol === undefined ? [] : Array(fills).fill(strings.indexOf(fillSymbol)),
  );
  table(SECTION.CANDLES, [1, 0, 4], [1234, 100, 101, 99, 100, 5]);
  if (book) table(SECTION.BOOK, [0, brokenBook ? 2 : 1, 0, 0], [100, 1, 1]);
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
    exchange.listenOrderBook('BTCUSDT', (book) => { book.bids[0].price = 999; });
    exchange.listenOrderBook('BTCUSDT', (book) => { observed.push(book.bids[0].price); });
    try {
      receive(exchange, fixture());
      expect(observed).toEqual([100]);
    } finally { exchange.dispose(); }
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
    expect(fills.map((fill) => fill.orderId)).toEqual(['order-1', 'order-1']);
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
    expect(fills.map((fill) => fill.orderId)).toEqual(kind === SECTION.ORDERS ? ['order-1'] : []);
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
      expect(fills.map((fill) => fill.symbol)).toEqual(['ETHUSDT']);

      const recovered = fixture({ fills: 0, secondaryMarket: true, reversedMarkets: true });
      if (exchange instanceof RustEngineExchange) {
        expect(snapshot).toHaveBeenCalled();
        send(recovered);
      } else {
        expect(rpc).toHaveBeenCalledExactlyOnceWith('fetchEngineSnapshot');
        resolve(bundle(recovered));
        await settle();
      }
      send(fixture({ markets: false, fills: 1, book: false }));
      expect(fills.map((fill) => fill.symbol)).toEqual(['ETHUSDT', 'ETHUSDT']);
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
    internal.bookListeners.set('BTCUSDT', new Map([[1, provider], [2, laterProvider], [3, ordinary]]));
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
    expect(new Frame(forwarded!).balance()?.total).toBe(42);
    expect(snapshot).not.toHaveBeenCalled();
    expect(exchange.isDisposed).toBe(false);
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
    const main = new BaseExchange('binance', { key: '', secret: '' }, new DefaultStore());
    const rpc = vi
      .spyOn(main, 'callMethodInWorker')
      .mockRejectedValueOnce(new Error('worker unavailable'))
      .mockResolvedValue(bundle(fixture({ fills: 0 })) as never);
    main.applyWorkerFrame(new ArrayBuffer(0));
    await settle();
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

  it.each([true, false])('flushes pending authoritative SETTINGS before local recovery (settings present: %s)', (settings) => {
    const store = new DefaultStore();
    const exchange = new RustEngineExchange('binance', { key: '', secret: '' }, store);
    receive(exchange, fixture({ fills: 0 }));
    (exchange as unknown as { applier: FrameApplier }).applier.applyLeverage('DELISTED', {
      buyLeverage: 9, sellLeverage: 9,
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
  });

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
    expect(new Frame(snapshot.frame).settings()!.count).toBe(0);
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
    expect(new Frame(result.frame).settings()!.buyLeverage![0]).toBe(12);
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
    expect(postMessage.mock.calls.at(-1)![0]).toMatchObject({
      op: 'store',
      data: { fetchedLeverage: { BTCUSDT: { buyLeverage: 12, sellLeverage: 12 } } },
    });
    worker.dispose();
    main.dispose();
  });

  it('publishes KuCoin Spot leverage acknowledgements from engine SETTINGS and keeps them authoritative', async () => {
    const store = new DefaultStore();
    const exchange = new RustEngineExchange('kucoinspot', { key: 'key', secret: 'secret' }, store);
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
      receive(exchange, fixture({ fills: 0 }));
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
