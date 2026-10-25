import type { EngineCapabilities, WasmEngineLike } from '@tealstreet/safe-cex-wasm/host';
import type { WasmEngineModule } from '../../exchanges/rust/rust.exchange';
import type { ExchangeName, Order, StoreData } from '../../types';

import { afterEach, describe, expect, it, vi } from 'vitest';

import { RustEngineExchange, setWasmEngineLoader } from '../../exchanges/rust/rust.exchange';
import { ExchangeHealthMonitor } from '../../health/ExchangeHealthMonitor';
import { DefaultStore } from '../../store/store.base';
import { OrderSide, OrderStatus, OrderType } from '../../types';
import { virtualClock } from '../../utils/virtualClock';
import { consumerFrame } from '../cutover/consumerFrame';
import { engineCapabilities } from '../fixtures/engineCapabilities';
import { installNativeStartupFrames } from './rust.native-startup.fixtures';

const running: RustEngineExchange[] = [];

/** A venue whose bridge behaviour comes only from the record its engine reports. */
const fixture = (capabilities: Partial<EngineCapabilities>, name: ExchangeName = 'woo', native?: () => Partial<StoreData>) => {
  const commands: Array<Record<string, unknown>> = [];
  const commandTimes: number[] = [];
  const started = vi.fn(() => '[]');
  const noActions = () => '[]';
  const replies: Record<string, unknown> = {
    fetchAccount: { userId: 'account-1' },
    fetchLeverage: { buyLeverage: 20, sellLeverage: 50 },
    findOrderByClientOrderId: { found: false, definitive: true },
    fetchOrderBook: name === 'weex'
      ? { bids: [{ price: 60000, amount: 0.001, total: 0.001 }], asks: [], lastUpdateId: '90071992547409931234567890' }
      : name === 'paradex'
      ? { bids: [{ price: 60000, amount: 0.001, total: 60 }], asks: [] }
      : name === 'grvt'
      ? { bids: [{ price: 60000, amount: 0.001, total: 0.001 }], asks: [] }
      : ['backpack', 'cryptocom'].includes(name)
      ? { bids: [{ id: '60000.00', price: 60000, amount: 0.001, total: 0 }], asks: [] }
      : { bids: [{ price: 60000, amount: 0.001, total: 0.001 }], asks: [], lastUpdateId: '9007199254740993' },
  };
  const engine: WasmEngineLike = {
    capabilities: () => engineCapabilities(name, capabilities),
    start: started,
    command(json, now) {
      const command = JSON.parse(json);
      if (command.op === 'setTickerSymbols') return '[]';
      commands.push(command);
      commandTimes.push(now);
      return JSON.stringify([
        { op: 'done', command: command.command, result: JSON.stringify(replies[command.op] ?? []) },
      ]);
    },
    takeFrame: () =>
      new Uint8Array(
        consumerFrame({ loaded: { markets: true, tickers: true, balance: true, orders: true, positions: true } }),
      ),
    snapshotFrame: () => new Uint8Array(),
    wsOpen: noActions,
    wsMessage: noActions,
    wsClose: noActions, wsRouteExhausted: noActions,
    httpStart: noActions,
    httpChunk: noActions,
    httpEnd: noActions,
    httpFailed: noActions,
    timer: noActions,
  };
  setWasmEngineLoader(async () => ({
    WasmEngine: vi.fn(function () {
      return engine;
    }) as unknown as WasmEngineModule['WasmEngine'],
  }));
  const store = new DefaultStore();
  store.update({ loaded: { markets: true, tickers: true, balance: true, orders: true, positions: true } });
  installNativeStartupFrames(engine, store, native);
  // WEEX trusted signer setup requires its private passphrase.
  const credentials = { key: 'key', secret: 'secret', ...(name === 'weex' ? { passphrase: 'pass' } : {}) };
  const exchange = new RustEngineExchange(name, credentials, store);
  running.push(exchange);
  return { exchange, commands, commandTimes, started, engine };
};

const order = (id: string, symbol: string): Order =>
  ({
    id,
    symbol,
    status: OrderStatus.Open,
    type: OrderType.Limit,
    side: OrderSide.Buy,
    price: 1,
    amount: 1,
    filled: 0,
    remaining: 1,
    reduceOnly: false,
  }) as Order;

describe('Rust bridge routes from the engine capability record', () => {
  afterEach(() => {
    for (const exchange of running.splice(0)) exchange.dispose();
    vi.restoreAllMocks();
    vi.useRealTimers();
  });

  it('waits for pending native startup before history and placement, even after capabilities arrive', async () => {
    vi.useFakeTimers();
    let ready = false;
    const { exchange, commands, started } = fixture({}, 'txflow', () => ({
      loaded: { markets: ready, tickers: ready, orders: ready, positions: ready, balance: ready },
    }));
    const startup = exchange.start();
    await vi.advanceTimersByTimeAsync(0);
    expect(started).toHaveBeenCalledOnce();
    expect(exchange.engineCapabilities).toBeDefined();
    const history = exchange.fetchHistoricalOrders({ from: 1, to: 2 });
    const placement = exchange.placeOrders([{ symbol: 'BTCUSDT', type: OrderType.Limit, side: OrderSide.Buy, amount: 1, price: 100 }]);
    await vi.advanceTimersByTimeAsync(0);
    expect(commands.filter((command) => ['fetchOrderHistory', 'placeOrders'].includes(String(command.op)))).toEqual([]);
    ready = true;
    await vi.advanceTimersByTimeAsync(250);
    await startup;
    await expect(history).resolves.toEqual([]);
    await expect(placement).resolves.toEqual([]);
    expect(commands.filter((command) => ['fetchOrderHistory', 'placeOrders'].includes(String(command.op)))).toHaveLength(2);
  });

  it('settles queued calls through the existing startup disposal lifecycle without dispatching writes', async () => {
    vi.useFakeTimers();
    const { exchange, commands } = fixture({}, 'txflow', () => ({
      loaded: { markets: false, tickers: false, orders: false, positions: false, balance: false },
    }));
    const startup = exchange.start();
    const failedStartup = expect(startup).rejects.toThrow('disposed');
    await vi.advanceTimersByTimeAsync(0);
    const placement = exchange.placeOrders([{ symbol: 'BTCUSDT', type: OrderType.Limit, side: OrderSide.Buy, amount: 1, price: 100 }]);
    const failedPlacement = expect(placement).rejects.toMatchObject({
      kind: 'invalidInput', outcome: 'notSent', message: expect.stringContaining('placeOrders: exchange has not started because startup failed'),
    });
    exchange.dispose();
    await failedStartup;
    await failedPlacement;
    expect(commands.some((command) => command.op === 'placeOrders')).toBe(false);
  });

  it('keeps an undispatched queued write notSent when native startup traps', async () => {
    vi.spyOn(console, 'trace').mockImplementation(() => {});
    const { exchange, commands, engine } = fixture({}, 'txflow');
    engine.start = () => { throw new WebAssembly.RuntimeError('fixture startup trap'); };
    const errors = vi.fn();
    exchange.on('error', errors);
    const startup = exchange.start();
    const startupFailure = expect(startup).rejects.toMatchObject({ code: 'ENGINE_DEAD' });
    const placement = exchange.placeOrders([{ symbol: 'BTCUSDT', type: OrderType.Limit, side: OrderSide.Buy, amount: 1, price: 100 }]);
    await expect(placement).rejects.toMatchObject({
      kind: 'invalidInput', outcome: 'notSent', cause: { code: 'ENGINE_DEAD' },
      message: expect.stringContaining('placeOrders: exchange has not started because startup failed'),
    });
    await startupFailure;
    expect(commands.some((command) => command.op === 'placeOrders')).toBe(false);
    expect(errors).toHaveBeenCalledOnce();
  });

  it('validates scheduleCancel before JSON and preserves arm/clear through the native command', async () => {
    const now = 1_800_000_000_000;
    vi.spyOn(virtualClock, 'getCurrentTimeMs').mockReturnValue(now);
    const { exchange, commands, commandTimes } = fixture({}, 'hyperliquid');
    await exchange.start();
    for (const invalid of [NaN, Infinity, -Infinity, now + 4_999, now + 5_000.5, Number.MAX_SAFE_INTEGER + 1, '1800000030000', false]) {
      await expect(exchange.scheduleCancel(invalid as number)).rejects.toThrow(/safe integer|5 seconds/);
    }
    expect(commands.some((c) => c.op === 'scheduleCancel')).toBe(false);
    await expect(exchange.scheduleCancel(now + 5_000)).resolves.toBeUndefined();
    await expect(exchange.scheduleCancel(null)).resolves.toBeUndefined();
    await expect(exchange.scheduleCancel()).resolves.toBeUndefined();
    expect(commands.flatMap((c, i) => c.op === 'scheduleCancel' ? [commandTimes[i]] : [])).toEqual([now, now, now]);
    expect(commands.filter((c) => c.op === 'scheduleCancel')).toEqual([
      expect.objectContaining({ time: now + 5_000 }),
      expect.not.objectContaining({ time: expect.anything() }),
      expect.not.objectContaining({ time: expect.anything() }),
    ]);
  });

  it('propagates native schedule write uncertainty without retry or clear', async () => {
    const { exchange, commands, engine } = fixture({}, 'hyperliquid');
    await exchange.start();
    engine.command = (json) => {
      const c = JSON.parse(json);
      commands.push(c);
      return JSON.stringify([{ op: 'done', command: c.command, error: 'lost schedule reply', detail: { message: 'lost schedule reply', kind: 'network', outcome: 'unknown' } }]);
    };
    await expect(exchange.scheduleCancel()).rejects.toMatchObject({ kind: 'network', outcome: 'unknown' });
    expect(commands.filter((c) => c.op === 'scheduleCancel')).toHaveLength(1);
  });

  it('uses native scheduleCancel capability instead of venue-name inference', async () => {
    const { exchange, commands } = fixture({ scheduleCancel: false }, 'hyperliquid');
    await exchange.start();
    await expect(exchange.scheduleCancel()).rejects.toThrow('unavailable');
    expect(commands.some((c) => c.op === 'scheduleCancel')).toBe(false);
  });

  it('forwards scheduleCancel to the worker without creating a main-thread engine', async () => {
    const now = 1_800_000_000_000;
    vi.spyOn(virtualClock, 'getCurrentTimeMs').mockReturnValue(now);
    const { exchange, commands, started } = fixture({}, 'hyperliquid');
    const worker = vi.fn(({ requestId }: { requestId: string }) => {
      exchange.emitter.emit('callMethod:done', { requestId, success: true, result: undefined });
    });
    exchange.on('callMethod', worker);
    await expect(exchange.scheduleCancel(NaN)).rejects.toThrow('safe integer');
    expect(worker).not.toHaveBeenCalled();
    await expect(exchange.scheduleCancel(now + 30_000)).resolves.toBeUndefined();
    expect(worker).toHaveBeenCalledOnce();
    expect(worker.mock.calls[0][0]).toMatchObject({ method: 'scheduleCancel', args: [now + 30_000] });
    expect(started).not.toHaveBeenCalled();
    expect(commands).toEqual([]);
  });

  it('routes Backpack REST books from the native capability without subscribing', async () => {
    const { exchange, commands } = fixture({}, 'backpack');
    await exchange.start();
    const result = await exchange.fetchOrderBook('SOL_USDC_PERP', 1);
    expect(result).toEqual({
      bids: [{ id: '60000.00', price: 60000, amount: 0.001, total: 0 }], asks: [],
    });
    expect(commands.filter((c) => c.op === 'fetchOrderBook')).toEqual([
      expect.objectContaining({ symbol: 'SOL_USDC_PERP', limit: 1 }),
    ]);
    expect(commands.some((c) => c.op === 'subscribeOrderBook')).toBe(false);
  });

  it('routes WEEX REST books from native capability with an exact version', async () => {
    const { exchange, commands } = fixture({}, 'weex');
    await exchange.start();
    await expect(exchange.fetchOrderBook('BTCUSDT', 1)).resolves.toEqual({
      bids: [{ price: 60000, amount: 0.001, total: 0.001 }], asks: [], lastUpdateId: '90071992547409931234567890',
    });
    expect(commands.filter((c) => c.op === 'fetchOrderBook')).toEqual([
      expect.objectContaining({ symbol: 'BTCUSDT', limit: 1 }),
    ]);
    expect(commands.some((c) => c.op === 'subscribeOrderBook')).toBe(false);
  });

  it('routes Paradex REST books from the native capability without subscribing', async () => {
    const { exchange, commands } = fixture({}, 'paradex');
    await exchange.start();
    await expect(exchange.fetchOrderBook('BTC-USD-PERP', 5)).resolves.toEqual({
      bids: [{ price: 60000, amount: 0.001, total: 60 }], asks: [],
    });
    expect(commands.filter((c) => c.op === 'fetchOrderBook')).toEqual([
      expect.objectContaining({ symbol: 'BTC-USD-PERP', limit: 5 }),
    ]);
    expect(commands.some((c) => c.op === 'subscribeOrderBook')).toBe(false);
  });

  it('routes GRVT REST books from the native capability without subscribing', async () => {
    const { exchange, commands } = fixture({}, 'grvt');
    await exchange.start();
    await expect(exchange.fetchOrderBook('BTC_USDT_PERP', 1)).resolves.toEqual({
      bids: [{ price: 60000, amount: 0.001, total: 0.001 }], asks: [],
    });
    expect(commands.filter((c) => c.op === 'fetchOrderBook')).toEqual([
      expect.objectContaining({ symbol: 'BTC_USDT_PERP', limit: 1 }),
    ]);
    expect(commands.some((c) => c.op === 'subscribeOrderBook')).toBe(false);
  });

  it('routes Crypto.com REST books from the native capability without subscribing', async () => {
    const { exchange, commands } = fixture({}, 'cryptocom');
    await exchange.start();
    await expect(exchange.fetchOrderBook('BTCUSD-PERP', 1)).resolves.toEqual({
      bids: [{ id: '60000.00', price: 60000, amount: 0.001, total: 0 }], asks: [],
    });
    expect(commands.filter((c) => c.op === 'fetchOrderBook')).toEqual([
      expect.objectContaining({ symbol: 'BTCUSD-PERP', limit: 1 }),
    ]);
    expect(commands.some((c) => c.op === 'subscribeOrderBook')).toBe(false);
  });

  it('routes MEXC snapshot queries without a book subscription', async () => {
    const { exchange, commands } = fixture({}, 'mexc');
    await exchange.start();
    await expect(exchange.fetchOrderBook('BTCUSDT', 20)).resolves.toEqual({
      bids: [{ price: 60000, amount: 0.001, total: 0.001 }], asks: [], lastUpdateId: '9007199254740993',
    });
    expect(commands.filter((c) => c.op === 'fetchOrderBook')).toEqual([
      expect.objectContaining({ symbol: 'BTCUSDT', limit: 20 }),
    ]);
    expect(commands.some((c) => c.op === 'subscribeOrderBook')).toBe(false);
  });

  it('keeps public MEXC depth refusals out of terminal account auth health', () => {
    const monitor = new ExchangeHealthMonitor('mexc');
    try {
      for (const status of [401, 403, 200]) {
        expect(monitor.processMessage(
          `MEXC depth query HTTP ${status}: invalid api key: permission denied`,
          true,
          { kind: 'venueUnavailable', outcome: 'notSent' },
        )).toBe('exchange_down');
        expect(monitor.hasAuthError()).toBe(false);
      }
    } finally {
      monitor.dispose();
    }
  });

  it('is unknown until the engine exists, then brands the missing history axes', async () => {
    const { exchange } = fixture({ fillHistory: false, orderHistory: false, incomeHistory: false });
    expect(exchange.engineCapabilities).toBeUndefined();
    expect(exchange.capabilities.orderHistory).toBe(true);
    await exchange.start();
    expect(exchange.engineCapabilities?.cancelAll).toBe('native');
    expect(exchange.capabilities).toMatchObject({
      fillHistory: false,
      orderHistory: false,
      incomeHistory: false,
      incomeRange: false,
    });
  });

  it('loads the record by creating the engine without starting it', async () => {
    const { exchange, started } = fixture({ fillHistory: false, orderHistory: false, incomeHistory: false });
    await expect(exchange.loadCapabilities()).resolves.toMatchObject({ fillHistory: false, orderHistory: false });
    expect(started).not.toHaveBeenCalled();
  });

  it('answers a client order lookup from open orders before an engine exists', async () => {
    const { exchange, commands } = fixture({ clientOrderLookup: true });
    await expect(exchange.findOrderByClientOrderId('BTC', 'id-1')).resolves.toEqual({ found: false, definitive: false });
    // The native open-order read may start the engine; no lookup command is sent.
    expect(commands.filter((command) => command.op !== 'setPollScale')).toEqual([]);
  });

  it('keeps reported history axes as engine queries', async () => {
    const { exchange } = fixture({});
    await exchange.start();
    expect(exchange.capabilities).toMatchObject({
      fillHistory: true,
      orderHistory: true,
      incomeHistory: true,
      incomeRange: true,
    });
    expect(exchange.fetchFillsForOrderIds).toBeUndefined();
  });

  it('cancels everything in one native command only where the engine supports it', async () => {
    const native = fixture({ cancelAll: 'native' });
    await native.exchange.start();
    await native.exchange.cancelAllOrders();
    expect(native.commands.filter((c) => c.op === 'cancelSymbolOrders')).toEqual([
      expect.objectContaining({ symbol: '' }),
    ]);

    const perSymbol = fixture({ cancelAll: 'eachOpenSymbol' });
    await perSymbol.exchange.start();
    // Cancel-all reads the native open orders; the fixture's native state mirrors the store.
    perSymbol.exchange.store.update({
      markets: ['BTC', 'ETH'].map((symbol) => ({
        id: symbol,
        symbol,
        base: symbol,
        quote: 'USDT',
        active: true,
        precision: { amount: 0.1, price: 0.1 },
        limits: { amount: { min: 0.1, max: 100 }, leverage: { min: 1, max: 100 } },
      })),
      orders: [order('a', 'BTC'), order('b', 'ETH'), order('c', 'BTC')],
    });
    await perSymbol.exchange.cancelAllOrders();
    expect(perSymbol.commands.filter((c) => c.op === 'cancelSymbolOrders').map((c) => c.symbol)).toEqual([
      'BTC',
      'ETH',
    ]);
  });

  it('reads account identity as the record says', async () => {
    const unsupported = fixture({ account: 'unsupported' });
    await expect(unsupported.exchange.getAccount()).rejects.toThrow('Not implemented');
    expect(unsupported.commands).toEqual([]);
    expect(unsupported.started).not.toHaveBeenCalled();

    const query = fixture({ account: 'query' });
    await expect(query.exchange.getAccount()).resolves.toEqual({ userId: 'account-1' });
    expect(query.started).not.toHaveBeenCalled();

    const afterLoad = fixture({ account: 'afterLoad' });
    await expect(afterLoad.exchange.getAccount()).resolves.toEqual({ userId: 'account-1' });
    expect(afterLoad.started).toHaveBeenCalledTimes(1);
  });

  it('asks the venue for a client order id only where it can answer', async () => {
    const venue = fixture({ clientOrderLookup: true });
    await venue.exchange.start();
    await expect(venue.exchange.findOrderByClientOrderId('BTC', 'id-1')).resolves.toEqual({
      found: false,
      definitive: true,
    });
    expect(venue.commands.map((c) => c.op)).toContain('findOrderByClientOrderId');

    const store = fixture({ clientOrderLookup: false });
    await store.exchange.start();
    await store.exchange.findOrderByClientOrderId('BTC', 'id-1');
    expect(store.commands.map((c) => c.op)).not.toContain('findOrderByClientOrderId');
  });

  it('reads leverage rails on demand only where the venue answers', async () => {
    const onRead = fixture({ leverageQuery: 'onRead' });
    await onRead.exchange.start();
    await expect(onRead.exchange.fetchLeverageForSymbol('BTC')).resolves.toEqual({
      buyLeverage: 20,
      sellLeverage: 50,
    });

    const settings = fixture({ leverageQuery: 'settings' });
    await settings.exchange.start();
    await expect(settings.exchange.fetchLeverageForSymbol('BTC')).resolves.toBeNull();
    expect(settings.commands.map((c) => c.op)).not.toContain('fetchLeverage');

    // A metadata-refreshed venue reads a flat symbol it has not refreshed yet,
    // then serves the published rails without another venue read.
    const onMetadata = fixture({ leverageQuery: 'onMetadata' });
    await onMetadata.exchange.start();
    await expect(onMetadata.exchange.fetchLeverageForSymbol('BTC')).resolves.toEqual({
      buyLeverage: 20,
      sellLeverage: 50,
    });
    expect(onMetadata.commands.filter((c) => c.op === 'fetchLeverage')).toHaveLength(1);
    await expect(onMetadata.exchange.fetchLeverageForSymbol('BTC')).resolves.toMatchObject({
      buyLeverage: 20,
      sellLeverage: 50,
    });
    expect(onMetadata.commands.filter((c) => c.op === 'fetchLeverage')).toHaveLength(1);
    await onMetadata.exchange.updateSymbolMetadata('BTC');
    expect(onMetadata.commands.filter((c) => c.op === 'fetchLeverage')).toHaveLength(2);
  });

  it('reads BingX settings for a flat symbol during metadata refresh', async () => {
    const { exchange, commands } = fixture({}, 'bingx');
    await exchange.start();
    await exchange.updateSymbolMetadata('BTCUSDT');
    expect(commands.filter((c) => c.op === 'fetchLeverage')).toEqual([
      expect.objectContaining({ symbol: 'BTCUSDT' }),
    ]);
    expect(exchange.store.fetchedLeverage.BTCUSDT).toEqual({ buyLeverage: 20, sellLeverage: 50 });
  });

  it('keeps combined metadata refreshes and explicit reads fresh even with cached rails', async () => {
    // The generic record controls dispatch; no BingX-name branch is involved.
    const { exchange, commands } = fixture({ leverageQuery: 'onReadAndMetadata' });
    await exchange.start();
    for (let read = 0; read < 2; read++) {
      await expect(exchange.fetchLeverageForSymbol('BTCUSDT')).resolves.toEqual({
        buyLeverage: 20,
        sellLeverage: 50,
      });
    }
    expect(commands.filter((command) => command.op === 'fetchLeverage')).toHaveLength(2);
    await exchange.updateSymbolMetadata('BTCUSDT');
    expect(commands.filter((command) => command.op === 'fetchLeverage')).toHaveLength(3);
  });

  it('keeps existing onRead venues fresh only on explicit reads, without new metadata dispatch', async () => {
    const { exchange, commands } = fixture({}, 'grvt');
    await exchange.start();
    await exchange.updateSymbolMetadata('BTCUSDT');
    expect(commands.filter((command) => command.op === 'fetchLeverage')).toHaveLength(0);
    await exchange.fetchLeverageForSymbol('BTCUSDT');
    await exchange.fetchLeverageForSymbol('BTCUSDT');
    expect(commands.filter((command) => command.op === 'fetchLeverage')).toHaveLength(2);
  });

  it('takes maximum leverage from the reported source', async () => {
    const rails = fixture({ maxLeverage: 'rails' });
    await rails.exchange.start();
    await expect(rails.exchange.fetchSymbolMaxLeverage('BTC')).resolves.toBe(50);

    const unsupported = fixture({ maxLeverage: 'unsupported' });
    await unsupported.exchange.start();
    await expect(unsupported.exchange.fetchSymbolMaxLeverage('BTC')).rejects.toThrow('Not implemented');
  });

  it('exposes fills by order id only where the engine filters by it', async () => {
    const { exchange } = fixture({ fillsByOrderId: true });
    await exchange.start();
    expect(exchange.fetchFillsForOrderIds).toBeTypeOf('function');
  });

  it('rejects option orders unless the engine reports options', async () => {
    const { exchange, commands } = fixture({ options: false });
    await exchange.start();
    await expect(
      exchange.placeOrders([
        { symbol: 'BTC', type: OrderType.Limit, side: OrderSide.Buy, amount: 1, price: 1, impliedVolatility: 0.5 },
      ]),
    ).rejects.toThrow('Options are not enabled on this account');
    expect(commands.map((c) => c.op)).not.toContain('placeOrders');
  });
});

/** A frame holding only a CAPABILITIES section: a flag word, then four-bit enum fields. */
const capabilityFrame = (flags: number, enums: number) => {
  const buffer = new ArrayBuffer(16 + 8 + 8);
  const view = new DataView(buffer);
  view.setUint32(0, 0x46584353, true);
  view.setUint16(4, 1, true);
  view.setUint16(6, 1, true);
  view.setUint16(16, 19, true);
  view.setUint32(20, 8, true);
  view.setUint32(24, flags, true);
  view.setUint32(28, enums, true);
  return buffer;
};

describe('main-thread mirrors learn the record from snapshot frames', () => {
  it('adopts the record a worker snapshot carries, without an engine', () => {
    const mirror = new RustEngineExchange('binancespot', { key: '', secret: '', publicDataOnly: true }, new DefaultStore());
    try {
      expect(mirror.engineCapabilities).toBeUndefined();
      expect(mirror.capabilities.orderHistory).toBe(true);
      // No history axes, native cancel-all, base units.
      mirror.applyWorkerFrame(capabilityFrame(0, 2 << 4));
      expect(mirror.engineCapabilities).toMatchObject({ cancelAll: 'native', fillHistory: false });
      expect(mirror.capabilities).toMatchObject({
        fillHistory: false,
        orderHistory: false,
        incomeHistory: false,
        incomeRange: false,
      });
      // The record is fixed for the engine's lifetime: later snapshots do not replace it.
      mirror.applyWorkerFrame(capabilityFrame(0b1111, 0));
      expect(mirror.engineCapabilities?.cancelAll).toBe('native');
    } finally {
      mirror.dispose();
    }
  });
});
