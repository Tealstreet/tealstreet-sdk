import type { ExchangeLike } from '@tealstreet/cli-core/src/types';
import type { Transport, WasmEngineLike } from '@tealstreet/safe-cex-wasm/host';
import type { ActiveTWAP, ExchangeName, ExchangeOptions, StoreData } from '../../types';

import { afterAll, afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import { formatAccountMonitorOutput } from '@tealstreet/cli-core/src/commands/trading/MonitorCommand';
import { OrdersCommand } from '@tealstreet/cli-core/src/commands/trading/OrdersCommand';
import { PositionsCommand } from '@tealstreet/cli-core/src/commands/trading/PositionsCommand';
import { WorkerPlatformAdapter } from '@tealstreet/cli-core/src/worker-host/WorkerPlatformAdapter';
import { BaseExchange } from '../../exchanges/base';
import { loadNodeEngine } from '../../exchanges/rust/loadNodeEngine';
import {
  RUST_VENUES,
  RustEngineExchange,
  setRustEngineTransport,
  setWasmEngineLoader,
} from '../../exchanges/rust/rust.exchange';
import { initialStoreData } from '../../generated/StoreData';
import { createExchange } from '../../index';
import { FrameStore } from '../../store/frameStore';
import {
  MarginType,
  OrderSide,
  OrderStatus,
  OrderTimeInForce,
  OrderType,
  PositionMode,
  PositionSide,
} from '../../types';
import { virtualClock } from '../../utils/virtualClock';
import { ChaserOrderInstance } from '../../webworker/chaserOrder/ChaserOrderInstance';
import { ChaserOrderManager } from '../../webworker/chaserOrder/ChaserOrderManager';
import { ExchangeWorker } from '../../webworker/ExchangeWorker';
import { setMainAcceptsFrames } from '../../webworker/frameForwarding';
import { ChaserOrderEnrichmentPostprocessor } from '../../webworker/postProcessors/ChaserOrderEnrichmentPostprocessor';
import { DisplayedMarginPostprocessor } from '../../webworker/postProcessors/DisplayedMarginPostprocessor';
import { DisplayedPositionsPostprocessor } from '../../webworker/postProcessors/DisplayedPositionsPostprocessor';
import { OrdersRiskProfilePostprocessor } from '../../webworker/postProcessors/OrdersRiskProfilePostprocessor';
import { independentPostprocessors } from '../../webworker/postProcessors/registry';
import { TwapOrderEnrichmentPostprocessor } from '../../webworker/postProcessors/TwapOrderEnrichmentPostprocessor';
import { TwapInstance } from '../../webworker/twap/TwapInstance';
import { TwapStatus } from '../../webworker/twap/TwapStatus';
import { engineCapabilities, quantityUnitsCode } from '../fixtures/engineCapabilities';
import { consumerFrame } from './consumerFrame';
import { nativeFixture, withNativeSections } from './nativeFixture';

const hyroEnvironment = vi.hoisted(() => {
  const previous = process.env.NEXT_PUBLIC_HYROTRADER_SIMULATOR_REST_URL;
  process.env.NEXT_PUBLIC_HYROTRADER_SIMULATOR_REST_URL = 'https://hyro-fixture.invalid';
  return previous;
});
afterAll(() => {
  if (hyroEnvironment === undefined) delete process.env.NEXT_PUBLIC_HYROTRADER_SIMULATOR_REST_URL;
  else process.env.NEXT_PUBLIC_HYROTRADER_SIMULATOR_REST_URL = hyroEnvironment;
});

const venues = [...RUST_VENUES];
const native = ['mexc', 'blofin', 'kucoin', 'coinbase', 'coinbasespot', 'okx', 'okxeu', 'okxdemo', 'okxeudemo'];
const plain = <T>(value: T): T => JSON.parse(JSON.stringify(value));

function state(name: ExchangeName): Partial<StoreData> {
  const lots = native.includes(name);
  const marketId = name.startsWith('okx') ? 'BTC-USDT-SWAP' : 'native-market';
  const coinM = name === 'binanceinverse';
  const inverse = coinM || name === 'bybitinverse' || name === 'bitgetinverse';
  return {
    markets: [
      {
        id: marketId,
        symbol: 'BTCUSDT',
        base: 'BTC',
        quote: 'USDT',
        active: true,
        precision: { amount: coinM ? 1 : 0.1, price: 0.1 },
        contractSize: coinM ? 100 : lots ? 0.1 : 1,
        ...(inverse ? { isInverse: true } : {}),
        limits: { amount: { min: 0.1, max: 100 }, leverage: { min: 1, max: 50 } },
      },
    ],
    tickers: [
      {
        id: marketId,
        symbol: 'BTCUSDT',
        bid: coinM ? 9999 : 99,
        ask: coinM ? 10001 : 101,
        last: coinM ? 10000 : 100,
        mark: coinM ? 10000 : 100,
        index: coinM ? 10000 : 100,
        percentage: 1,
        openInterest: 20,
        fundingRate: 0.0001,
        fundingTime: 1_700_000_000_000,
        volume: 50,
        quoteVolume: coinM ? 500000 : 5000,
      },
    ],
    orders: [
      {
        id: 'order-1',
        clientOrderId: 'caller-1',
        symbol: 'BTCUSDT',
        status: OrderStatus.Open,
        side: OrderSide.Buy,
        type: OrderType.Limit,
        price: coinM ? 9999 : 99,
        amount: coinM ? 7 : inverse ? 70 : name.startsWith('okx') ? 7 : 0.7,
        filled: coinM ? 3 : inverse ? 30 : 0.3,
        remaining: coinM ? 4 : inverse ? 40 / 99 : 0.4,
        reduceOnly: false,
        lastUpdate: 1_700_000_000_000,
        timeInForce: OrderTimeInForce.PostOnly,
        ...(lots ? { extra: { remainingNative: 4, filledInPlacementUnits: 3 } } : {}),
        ...(inverse ? { extra: { remainingNative: coinM ? 4 : 40, remainingNotional: coinM ? 400 : 40 } } : {}),
      },
    ],
    positions: [
      {
        id: 'venue-position-1',
        symbol: 'BTCUSDT',
        side: PositionSide.Long,
        contracts: coinM ? 0.07 : 0.7,
        entryPrice: coinM ? 9900 : 90,
        notional: coinM ? 700 : 70,
        leverage: 10,
        unrealizedPnl: 7,
        liquidationPrice: coinM ? 5000 : 50,
        marginType: MarginType.Cross,
        positionMode: PositionMode.OneWay,
        ...(lots ? { extra: { contractsNative: 7 } } : {}),
        ...(inverse ? { extra: { contractsNative: coinM ? 7 : 70, unrealizedPnlNative: 7 } } : {}),
      },
    ],
    balance: { total: 1000, free: 900, used: 100, upnl: 7 },
    loaded: { balance: true, orders: true, positions: true, markets: true, tickers: true },
    socketHealth: { publicHealthy: true, privateHealthy: true, publicInitialized: true, privateInitialized: true },
    options: { isHedged: false },
    latency: 12,
    fetchedLeverage: {
      BTCUSDT: { buyLeverage: 10, sellLeverage: 15, marginType: MarginType.Cross, positionMode: PositionMode.OneWay },
    },
  };
}

// This mock bypasses WasmEngine::store_frames. Its historical TS oracle supplies
// explicit derived-section fixture values; production projection is covered by
// realVenue.wasm.test and safe-cex-store-frames' real Rust -> JS regressions.
function fixtureProjection(name: ExchangeName, data: Partial<StoreData>): StoreData {
  const decoded = new FrameStore();
  decoded.apply(nativeFixture(data));
  const result = structuredClone(decoded.data);
  for (const Cls of [
    DisplayedPositionsPostprocessor,
    DisplayedMarginPostprocessor,
    ChaserOrderEnrichmentPostprocessor,
    TwapOrderEnrichmentPostprocessor,
    OrdersRiskProfilePostprocessor,
  ])
    new Cls('conformance', name, { key: '', secret: '' }).process(result);
  return result;
}

// A deterministic binary producer for shared consumer edge cases, NOT a venue
// emulator. realVenue.wasm.test separately starts every actual Rust engine.
class FrameSource implements WasmEngineLike {
  pending = true;
  current: Partial<StoreData>;
  private readonly name: ExchangeName;
  constructor(
    readonly venue: string,
    _options: string,
  ) {
    const name = [...RUST_VENUES].find((name) => name === venue);
    if (!name) throw new Error('Unknown fixture venue');
    this.name = name;
    this.current = state(name);
    sources.push(this);
  }
  capabilities = () => engineCapabilities(this.venue);
  start = () => '[]';
  command = () => '[]';
  wsOpen = this.start;
  wsMessage = this.start;
  wsClose = this.start;
  wsRouteExhausted = this.start;
  httpStart = this.start;
  httpChunk = this.start;
  httpEnd = this.start;
  httpFailed = this.start;
  timer = this.start;
  takeFrame = (_now: number, marketOnly: boolean) => {
    if (!this.pending || marketOnly) return undefined;
    this.pending = false;
    return new Uint8Array(this.frame());
  };
  snapshotFrame = () => new Uint8Array(this.frame());
  accountSnapshotFrame = () => new Uint8Array(this.frame(true));
  private frame = (accountRead = false) => {
    const projected = fixtureProjection(this.name, this.current);
    return withNativeSections(
      consumerFrame(projected, undefined, false, undefined, quantityUnitsCode(this.venue), accountRead),
      projected,
    );
  };
}
const sources: FrameSource[] = [];
const workers: ExchangeWorker[] = [];
const exchanges: BaseExchange[] = [];
const managers: Array<{ manager: ChaserOrderManager; exchange: RustEngineExchange }> = [];
const twaps: TwapInstance[] = [];
const transport: Transport = {
  now: () => Date.now(),
  connect: () => {
    throw new Error('Consumer fixtures must not connect');
  },
  request: async () => {
    throw new Error('Consumer fixtures must not request');
  },
  setTimeout,
  clearTimeout: (handle) => clearTimeout(handle as ReturnType<typeof setTimeout>),
};

beforeEach(() => {
  vi.useFakeTimers();
  vi.spyOn(virtualClock, 'start').mockResolvedValue();
  setMainAcceptsFrames(true);
  setRustEngineTransport(() => transport);
  setWasmEngineLoader(async () => ({ WasmEngine: FrameSource }));
});
afterEach(async () => {
  await Promise.all(twaps.splice(0).map((twap) => twap.terminate()));
  managers.splice(0).forEach(({ manager, exchange }) => manager.cleanupExchange(exchange));
  workers.splice(0).forEach((worker) => worker.dispose());
  exchanges.splice(0).forEach((exchange) => exchange.dispose());
  sources.length = 0;
  setMainAcceptsFrames(false);
  setWasmEngineLoader(loadNodeEngine);
  vi.useRealTimers();
});

function consumerOptions(name: ExchangeName): ExchangeOptions {
  return {
    engine: 'rust',
    key: '',
    secret: '',
    publicDataOnly: true,
    ...(name === 'hyrotrader' ? { extra: { hyrotraderMode: 'demo' } } : {}),
  };
}

async function consumer(name: ExchangeName) {
  const options = consumerOptions(name);
  const main = new BaseExchange(name, options, new FrameStore());
  exchanges.push(main);
  const messages: Array<{ op: string; data: any }> = [];
  const ctx = {
    postMessage(message: { op: string; data: any }, transfer?: Transferable[]) {
      // Match a real postMessage: neither worker state nor its ArrayBuffer stays
      // shared with the receiving realm.
      const copy = structuredClone(message, { transfer });
      messages.push(copy);
      if (copy.op === 'frame') main.applyWorkerFrame(copy.data);
    },
  } as unknown as Worker;
  const worker = new ExchangeWorker(ctx, 'conformance', name, options, {} as never, {} as never, 'fixture');
  workers.push(worker);
  const started = worker.start();
  await vi.advanceTimersByTimeAsync(250);
  expect(await started).toBe(true);
  expect(worker.exchange).toBeInstanceOf(RustEngineExchange);
  const exchange = worker.exchange as RustEngineExchange;
  const deliver = (data: Partial<StoreData>, delta?: number[], events = false) => {
    // Exercise the same callback EngineHost invokes, with an independently
    // encoded lifecycle frame; no store mutation is substituted for decoding.
    (exchange as unknown as { receiveFrame(buffer: ArrayBuffer): void }).receiveFrame(
      withNativeSections(consumerFrame(data, delta, events, undefined, quantityUnitsCode(name)), data),
    );
  };
  return { main, worker, exchange, messages, ctx, deliver };
}

describe.each(venues)('%s consumer conformance', (name) => {
  it('honors explicit Rust selection in the public SDK factory', async () => {
    const store = new FrameStore();
    const exchange = await createExchange(name, consumerOptions(name), store);
    exchanges.push(exchange);
    expect(exchange).toBeInstanceOf(RustEngineExchange);
    expect(exchange.store).toBe(store);
  });

  it('preserves TS store reads, postprocessors, CLI output, transfer and late snapshots', async () => {
    const { worker, main, exchange, messages } = await consumer(name);
    const reference = new BaseExchange(name, { key: '', secret: '' }, new FrameStore());
    exchanges.push(reference);
    const expected = state(name);
    reference.store.apply(
      nativeFixture(
        expected,
        reference.store.markets.map((row) => row.symbol),
      ),
    );
    const derived = structuredClone(reference.store.data);
    for (const Cls of independentPostprocessors)
      new (Cls as typeof DisplayedPositionsPostprocessor)('conformance', name, exchange.options).process(derived);
    reference.store.apply(
      nativeFixture(
        derived,
        reference.store.markets.map((row) => row.symbol),
      ),
    );

    for (const key of [
      'markets',
      'tickers',
      'orders',
      'positions',
      'balance',
      'loaded',
      'fetchedLeverage',
      'displayedPositions',
      'displayedMargin',
      'ordersRisk',
    ] as const) {
      expect(plain(main.store[key]), key).toEqual(plain(reference.store[key]));
    }
    expect(formatAccountMonitorOutput(main as unknown as ExchangeLike)).toBe(
      formatAccountMonitorOutput(reference as unknown as ExchangeLike),
    );
    expect(formatAccountMonitorOutput(main as unknown as ExchangeLike)).toContain('BTCUSDT');
    const cliReads = async (reader: BaseExchange) => {
      const output: string[] = [];
      const capture = ({ message }: { message: string }) => output.push(message);
      reader.on('exec-log', capture);
      try {
        const exchange = reader as unknown as ExchangeLike;
        const adapter = new WorkerPlatformAdapter(exchange, 'readiness');
        for (const command of [new PositionsCommand(), new OrdersCommand()]) {
          const result = await command.execute({
            raw: command.name,
            args: { _: [command.name] },
            symbol: 'BTCUSDT',
            accountName: 'conformance',
            exchange,
            adapter,
          });
          expect(result.success).toBe(true);
        }
      } finally {
        reader.off('exec-log', capture);
      }
      return output;
    };
    const cliOutput = await cliReads(main);
    expect(cliOutput).toEqual(await cliReads(reference));
    expect(cliOutput).toHaveLength(2);
    expect(cliOutput.every((line) => line.includes('BTCUSDT'))).toBe(true);
    expect(Object.keys(main.store.data)).toEqual(expect.arrayContaining(Object.keys(initialStoreData)));
    const roundtrip = new FrameStore();
    roundtrip.apply(
      nativeFixture(
        structuredClone(main.store.data),
        roundtrip.markets.map((row) => row.symbol),
      ),
    );
    expect(plain(roundtrip.data)).toEqual(plain(main.store.data));
    const counts = { ...main.store.updateIds };
    worker.sendSnapshot();
    expect(main.store.updateIds.orders).toBe(counts.orders);
    expect(main.store.updateIds.positions).toBe(counts.positions);
    expect(plain(main.store.orders)).toEqual(plain(reference.store.orders));
    expect(messages.every((message) => message.op !== 'store')).toBe(true);
  });

  it('keeps update IDs, terminal opt-in, split deltas and stream events correct', async () => {
    const { worker, main, exchange, deliver } = await consumer(name);
    const ticker = main.store.tickers[0];
    const markets = main.store.markets;
    const orders = main.store.orders;
    const balance = main.store.balance;
    const counts = { ...worker.store.updateIds };
    const fills = vi.fn();
    const workerFills = vi.fn();
    const books = vi.fn();
    const candles = vi.fn();
    main.on('fill', fills);
    exchange.on('fill', workerFills);
    main.on('listenOrderBook', books);
    main.on('listenOHLCV', candles);
    deliver({}, [0, 4, 105], true);
    // FrameStore shares untouched sections and replaces the changed ticker row.
    expect(main.store.markets).toBe(markets);
    expect(main.store.orders).toBe(orders);
    expect(main.store.balance).toBe(balance);
    expect(main.store.tickers[0]).not.toBe(ticker);
    expect(ticker.last).toBe(state(name).tickers![0].last);
    expect(main.store.tickers[0].last).toBe(105);
    expect(worker.store.updateIds).toEqual(counts);
    expect(fills).toHaveBeenCalledExactlyOnceWith({
      id: 'order-1',
      orderId: 'order-1',
      symbol: 'BTCUSDT',
      side: OrderSide.Buy,
      price: 100,
      amount: 0.3,
    });
    expect(workerFills).toHaveBeenCalledOnce();
    expect(books).toHaveBeenCalledExactlyOnceWith({
      symbol: 'BTCUSDT',
      orderbook: { bids: [{ price: 99, amount: 2, total: 2 }], asks: [{ price: 101, amount: 3, total: 3 }] },
    });
    expect(candles).toHaveBeenCalledExactlyOnceWith({
      symbol: 'BTCUSDT',
      interval: '1m',
      candle: { timestamp: 1_700_000_000, open: 100, high: 102, low: 99, close: 101, volume: 5 },
    });
    (() => {
      const targets: Array<Pick<StoreData['orders'][number], 'id'>> = [{ id: 'order-1' }];
      const status: StoreData['orders'][number]['status'] = OrderStatus.Canceled;
      return worker.store.apply(
        nativeFixture(
          {
            orders: worker.store.orders.map((row) =>
              targets.some((target) => target.id === row.id) ? { ...row, status } : row,
            ),
          },
          worker.store.markets.map((row) => row.symbol),
        ),
      );
    })();
    (() => {
      const targets: Array<Pick<StoreData['orders'][number], 'id'>> = [{ id: 'order-1' }];
      const status: StoreData['orders'][number]['status'] = OrderStatus.Canceled;
      return main.store.apply(
        nativeFixture(
          {
            orders: main.store.orders.map((row) =>
              targets.some((target) => target.id === row.id) ? { ...row, status } : row,
            ),
          },
          main.store.markets.map((row) => row.symbol),
        ),
      );
    })();
    deliver({ orders: state(name).orders });
    expect(worker.store.orders[0].status).toBe(OrderStatus.Open);
    expect(main.store.orders[0].status).toBe(OrderStatus.Open);
    // The engine owns private reconciliation. Explicit store terminal memory
    // is local; bridge snapshots still decode on main with its own counters.
    expect(worker.store.updateIds.orders).toBe(counts.orders + 2);
    const unmarked = { ...state(name).orders![0], id: 'triggered', status: OrderStatus.Closed };
    deliver({ orders: [unmarked] });
    deliver({ orders: [{ ...unmarked, status: OrderStatus.Open }] });
    expect(worker.store.orders[0].status).toBe(OrderStatus.Open);
    deliver({ orders: [], positions: [] });
    expect(worker.store.orders).toEqual([]);
    expect(main.store.orders).toEqual([]);
    expect(main.store.positions).toEqual([]);
    expect(worker.store.updateIds.positions).toBe(counts.positions + 1);
    worker.sendSnapshot();
    expect(fills).toHaveBeenCalledOnce();
  });

  it('drives native chaser fill sizing and TWAP limit prices from decoded stores', async () => {
    const { exchange, main, ctx, deliver } = await consumer(name);
    const coinM = name === 'binanceinverse';
    const inverse = name === 'bybitinverse' || name === 'bitgetinverse';
    const size = coinM ? 7 : inverse ? 70 : native.includes(name) ? 7 : 0.7;
    for (const reader of [exchange, main]) {
      const chaser = new ChaserOrderInstance(reader, {
        symbol: 'BTCUSDT',
        side: OrderSide.Buy,
        size,
        maxAttempts: 5,
        delay: 1000,
      });
      chaser.subtractFilledSize(reader.store.orders[0]);
      if (native.includes(name)) expect(reader.store.orders[0].extra?.filledInPlacementUnits).toBe(3);
      expect(chaser.remainingSize).toBeCloseTo(coinM ? 4 : inverse ? 40 : native.includes(name) ? 4 : 0.4, 12);
      chaser.subtractFilledSize(reader.store.orders[0]);
      expect(chaser.remainingSize).toBeCloseTo(coinM ? 4 : inverse ? 40 : native.includes(name) ? 4 : 0.4, 12);
    }
    const place = vi.spyOn(exchange, 'placeOrder').mockResolvedValue(['twap-order']);
    for (const side of [OrderSide.Buy, OrderSide.Sell]) {
      const twap = new TwapInstance(
        exchange,
        ctx,
        {
          accountName: 'conformance',
          activeMarket: exchange.store.markets[0],
          size,
          side,
          duration: 1,
          ordersCount: 1,
          chaserDelay: 1000,
          reduceOnly: false,
          randomnessPercentage: 0,
          useLimitOrders: true,
          useChaserOrders: false,
        },
        () => {},
      );
      await twap.start();
      // A slice carries the venue's minted client id where it mints one (#2514).
      const placed = place.mock.lastCall![0] as { clientOrderId?: string };
      expect(placed).toEqual({
        symbol: 'BTCUSDT',
        side,
        type: OrderType.Limit,
        price: side === OrderSide.Buy ? state(name).tickers![0].bid : state(name).tickers![0].ask,
        amount: size,
        reduceOnly: false,
        ...(placed.clientOrderId === undefined ? {} : { clientOrderId: expect.stringMatching(/.+/) }),
      });
    }
    // The supervisor observes order updates immediately through BaseExchange's
    // store subscription; no direct WS event is necessary to finish a chase.
    place.mockResolvedValue(['supervised']);
    vi.spyOn(exchange, 'cancelOrders').mockResolvedValue();
    const manager = new ChaserOrderManager();
    managers.push({ manager, exchange });
    exchange.store.apply(
      nativeFixture(
        {
          socketHealth: {
            publicHealthy: true,
            privateHealthy: true,
            publicInitialized: true,
            privateInitialized: true,
          },
        },
        exchange.store.markets.map((row) => row.symbol),
      ),
    );
    const supervised = new ChaserOrderInstance(exchange, {
      symbol: 'BTCUSDT',
      side: OrderSide.Buy,
      size: 1,
      maxAttempts: 5,
      delay: 1000,
    });
    const completion = vi.fn();
    const errors = vi.fn();
    exchange.on('chaser:complete', completion);
    exchange.on('error', errors);
    manager.add(supervised);
    await vi.advanceTimersByTimeAsync(0);
    expect(supervised.currentOrderIds).toEqual(['supervised']);
    deliver({
      orders: [{ ...state(name).orders![0], id: 'supervised', filled: 0, remaining: 1, extra: { remainingNative: 1 } }],
    });
    await vi.advanceTimersByTimeAsync(1000);
    deliver({
      orders: [
        {
          ...state(name).orders![0],
          id: 'supervised',
          status: OrderStatus.Closed,
          amount: 1,
          filled: 1,
          remaining: 0,
          ...(native.includes(name) ? { filled: 0.1, extra: { remainingNative: 0, filledInPlacementUnits: 1 } } : {}),
        },
      ],
    });
    await vi.advanceTimersByTimeAsync(0);
    expect(supervised.isStopped).toBe(true);
    expect(supervised.remainingSize).toBe(0);
    expect(completion).toHaveBeenCalledExactlyOnceWith(
      expect.objectContaining({ size: 1, filled: 1, stopReason: 'Completed' }),
    );
    expect(errors).not.toHaveBeenCalled();
    manager.cleanupExchange(exchange);
  });

  it('ships explicit native algo, display and ownership sections through snapshots', async () => {
    const { worker, main, exchange, deliver } = await consumer(name);
    // The rows the engine's Rust algos post: this fixture engine runs no algos.
    const chaser = {
      id: 'chaser-1',
      symbol: 'BTCUSDT',
      side: OrderSide.Buy,
      size: 0.7,
      maxAttempts: 5,
      attempts: 1,
      isStopped: false,
      stopReason: '',
      currentOrderIds: ['order-1'],
    };
    const twap = {
      id: 'twap-1',
      symbol: 'BTCUSDT',
      accountName: 'conformance',
      activeMarket: exchange.store.markets[0],
      side: OrderSide.Buy,
      size: 2,
      ordersExecuted: 1,
      status: TwapStatus.Running,
    } as unknown as ActiveTWAP;
    const source = sources.find((candidate) => candidate.venue === name);
    if (!source) throw new Error('Missing fixture engine');
    source.current = { ...source.current, activeChasers: [chaser], activeTWAPs: [twap] };
    deliver(fixtureProjection(name, source.current));
    await vi.advanceTimersByTimeAsync(1000);
    worker.sendSnapshot();
    expect(main.store.activeChasers).toEqual([
      expect.objectContaining({ id: chaser.id, currentOrderIds: ['order-1'], isStopped: false }),
    ]);
    expect(main.store.activeTWAPs).toEqual([
      expect.objectContaining({ id: twap.id, ordersExecuted: 1, symbol: 'BTCUSDT' }),
    ]);
    expect(main.store.displayedPositions[0]).toMatchObject({ hasActiveChaser: true, hasActiveTWAP: true });
    expect(worker.store.orders[0].extra).toMatchObject({ isFromChaser: true, chaserId: chaser.id });
    expect(main.store.orders[0].extra).toMatchObject({ isFromChaser: true, chaserId: chaser.id });
    const beforeTags = { ...main.store.updateIds };
    const notices: number[] = [];
    const unsubscribe = main.store.subscribe((data) => notices.push(data.updateIds.orders));
    const exchangeUpdates = vi.fn();
    main.on('update', exchangeUpdates);
    const activeChasers = main.store.activeChasers.map((row) => ({ ...row, attempts: row.attempts + 1 }));
    main.store.apply(
      nativeFixture(
        {
          activeChasers,
          orders: main.store.orders.map((row) => ({
            ...row,
            extra: { ...row.extra, chaserAttempts: activeChasers[0].attempts },
          })),
        },
        main.store.markets.map((row) => row.symbol),
      ),
    );
    expect(main.store.updateIds).toEqual({ orders: beforeTags.orders + 1, positions: beforeTags.positions });
    expect(main.store.orders[0].extra?.chaserAttempts).toBe(activeChasers[0].attempts);
    expect(notices.length).toBeGreaterThan(0);
    expect(notices.every((id) => id === beforeTags.orders + 1)).toBe(true);
    expect(exchangeUpdates).toHaveBeenCalledOnce();
    main.store.apply(
      nativeFixture(
        {
          activeChasers,
          orders: main.store.orders.map((row) => ({
            ...row,
            extra: { ...row.extra, chaserAttempts: activeChasers[0].attempts },
          })),
        },
        main.store.markets.map((row) => row.symbol),
      ),
    );
    (() => {
      const target = { id: state(name).markets![0].id };
      const changes: Partial<StoreData['tickers'][number]> = { last: 106 };
      return main.store.apply(
        nativeFixture(
          { tickers: main.store.tickers.map((row) => (row.id === target.id ? { ...row, ...changes } : row)) },
          main.store.markets.map((row) => row.symbol),
        ),
      );
    })();
    expect(main.store.updateIds.orders).toBe(beforeTags.orders + 1);
    unsubscribe();
    main.off('update', exchangeUpdates);
    deliver({ orders: state(name).orders });
    expect(main.store.orders[0].extra).toMatchObject({ isFromChaser: true, chaserId: chaser.id });
    main.dispose();
    const disposedOrders = main.store.updateIds.orders;
    main.store.apply(
      nativeFixture(
        { activeChasers: activeChasers.map((row) => ({ ...row, attempts: row.attempts + 1 })) },
        main.store.markets.map((row) => row.symbol),
      ),
    );
    expect(main.store.updateIds.orders).toBe(disposedOrders);
  });
});
