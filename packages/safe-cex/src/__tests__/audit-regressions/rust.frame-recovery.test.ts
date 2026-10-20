import type { CallableMethodsInWorker } from '../../exchanges/base';
import type { OrderFillEvent } from '../../types';

import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import { SECTION } from '@tealstreet/safe-cex-wasm/wire';
import { BaseExchange } from '../../exchanges/base';
import { RustEngineExchange } from '../../exchanges/rust/rust.exchange';
import { FrameApplier } from '../../store/frameApplier';
import { DefaultStore } from '../../store/store.base';
import { CallMethodHandler } from '../../webworker/handlers/CallMethodHandler';

const align8 = (value: number) => Math.ceil(value / 8) * 8;

// Independent revision-zero bytes exercise the real readers and caller recovery.
const fixture = ({ brokenBook = false, fills = 2, book = true, total = 42, omitStrings = false } = {}) => {
  const strings = ['BTCUSDT', 'BTC', 'USDT', 'order-1', '1m'];
  const sections: Array<{ kind: number; bytes: Uint8Array }> = [];
  const table = (kind: number, words: number[], numbers: number[]) => {
    const bytes = new Uint8Array(align8(words.length * 4) + numbers.length * 8);
    const view = new DataView(bytes.buffer);
    words.forEach((word, i) => view.setUint32(i * 4, word, true));
    numbers.forEach((number, i) => view.setFloat64(align8(words.length * 4) + i * 8, number, true));
    sections.push({ kind, bytes });
  };
  table(SECTION.MARKETS, [1, 0, 0, 1, 2, 1], [0.001, 1, 0.001, 100, 1, 100, NaN, 1, NaN, NaN]);
  table(SECTION.ORDERS, [1, 3, 0xffffffff, 0xffffffff, 0, 256], [100, 1, 0, 1, 1234, NaN, NaN]);
  table(SECTION.POSITIONS, [1, 0, 2 << 16], [100, 50, 5, 0, 0, 0.5, 80, 10, 1234]);
  table(SECTION.BALANCE, [], [total - 30, 30, total, 0]);
  table(SECTION.SETTINGS, [1, 0, 2], [5, 5, 5]);
  table(
    SECTION.FILLS,
    [fills, ...Array(fills).fill(3), ...Array(fills).fill(0), ...Array(fills).fill(0)],
    Array.from({ length: fills }, () => [100, 0.25]).flat(),
  );
  table(SECTION.CANDLES, [1, 0, 4], [1234, 100, 101, 99, 100, 5]);
  if (book) table(SECTION.BOOK, [0, brokenBook ? 2 : 1, 0, 0], [100, 1, 1]);
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
    view.setUint32(offset + 4, section.bytes.length, true);
    new Uint8Array(buffer, offset + 8, section.bytes.length).set(section.bytes);
    offset += 8 + section.bytes.length;
  }
  return buffer;
};

const receive = (exchange: RustEngineExchange, buffer: ArrayBuffer) =>
  (exchange as unknown as { receiveFrame: (buffer: ArrayBuffer) => void }).receiveFrame(buffer);

const installHost = (exchange: RustEngineExchange, snapshot: ReturnType<typeof vi.fn>) => {
  (exchange as unknown as { host: unknown }).host = { snapshot, dispose: vi.fn() };
};

const settle = async () => {
  for (let i = 0; i < 8; i++) await Promise.resolve();
};

describe('Rust frame recovery callers', () => {
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

  it('coalesces main-thread snapshot RPCs and applies a recovery without replaying original fills', async () => {
    const store = new DefaultStore();
    const exchange = new BaseExchange('binance', { key: '', secret: '' }, store);
    let resolve!: (buffer: ArrayBuffer) => void;
    const pending = new Promise<ArrayBuffer>((done) => {
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
    resolve(fixture({ fills: 0, total: 100 }));
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
    const rpc = vi.spyOn(main, 'callMethodInWorker').mockResolvedValue(new ArrayBuffer(0) as never);
    main.applyWorkerFrame(new ArrayBuffer(0));
    await settle();
    expect(rpc).toHaveBeenCalledTimes(1);
    worker.dispose();
    main.dispose();
  });

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
      else vi.spyOn(exchange, 'callMethodInWorker').mockImplementation(async () => snapshot() as never);
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
      else vi.spyOn(exchange, 'callMethodInWorker').mockImplementation(async () => snapshot() as never);
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
      else vi.spyOn(exchange, 'callMethodInWorker').mockImplementation(async () => snapshot() as never);
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

  it('returns raw engine snapshots through the typed worker method and rejects TS-only exchanges', async () => {
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
    const result: ArrayBuffer = await main.callMethodInWorker(method);
    expect(result).toBe(bytes);
    expect(postMessage).toHaveBeenCalledExactlyOnceWith({
      op: 'callMethod:done',
      data: { requestId: expect.any(String), result: bytes, success: true },
    });
    expect(postMessage.mock.calls[0][0].data.result).toBe(bytes);
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
      .mockResolvedValue(fixture({ fills: 0 }) as never);
    main.applyWorkerFrame(new ArrayBuffer(0));
    await settle();
    main.applyWorkerFrame(new ArrayBuffer(0));
    await settle();
    expect(rpc).toHaveBeenCalledTimes(2);
    worker.dispose();
    main.dispose();
  });

  it.each(['polymarket', 'mexc', 'binance'] as const)(
    'routes %s leverage refresh through the applier and keeps SETTINGS authoritative',
    async (venue) => {
      const store = new DefaultStore();
      const exchange = new RustEngineExchange(venue, { key: 'key', secret: 'secret' }, store);
      receive(exchange, fixture({ fills: 0 }));
      const rails = { buyLeverage: 7, sellLeverage: 9 };
      const command = vi.fn().mockResolvedValue(rails);
      (exchange as unknown as { command: unknown }).command = command;
      const applier = (exchange as unknown as { applier: FrameApplier }).applier;
      const writer = vi.spyOn(applier, 'applyLeverage');
      if (venue === 'mexc') await exchange.updateSymbolMetadata('BTCUSDT');
      else await exchange.fetchLeverageForSymbol('BTCUSDT');
      const expected = venue === 'binance' ? { buyLeverage: 100, sellLeverage: 100 } : rails;
      expect(writer).toHaveBeenCalledExactlyOnceWith('BTCUSDT', expected);
      expect(store.fetchedLeverage.BTCUSDT).toEqual(expected);
      if (venue === 'binance') expect(command).not.toHaveBeenCalled();
      else expect(command).toHaveBeenCalledExactlyOnceWith({ op: 'fetchLeverage', symbol: 'BTCUSDT' });
      applier.applyLeverage('DELISTED', rails);
      receive(exchange, fixture({ fills: 0 }));
      expect(store.fetchedLeverage.BTCUSDT.buyLeverage).toBe(5);
      expect(store.fetchedLeverage.DELISTED).toBeUndefined();
      exchange.dispose();
    },
  );
});
