import type { EngineHost as EngineHostType, Transport, WasmEngineLike } from '@tealstreet/safe-cex-wasm/host';
import type { Store } from '../../store/store.interface';
import type {
  Candle,
  ExchangeName,
  ExchangeOptions,
  Execution,
  HistoricalOrder,
  IncomeHistoryOptions,
  IncomeRecord,
  OHLCVOptions,
  Order,
  OrderBook,
  OrderHistoryOptions,
  PlaceOrderOpts,
  PublicTrade,
  UpdateOrderOpts,
  ExecutionsOptions,
} from '../../types';

import { EngineHost, nativeTransport } from '@tealstreet/safe-cex-wasm/host';
import { Frame } from '@tealstreet/safe-cex-wasm/wire';
import { exchangeMetadata } from '../../exchangeMetadata';
import { FrameApplier } from '../../store/frameApplier';
import { LogSeverity } from '../../types';
import { BaseExchange } from '../base';

/** Venues the Rust engine runs, by exchange name, and its name for each. */
export const RUST_VENUES: Partial<Record<ExchangeName, string>> = {
  binance: 'binance',
  binancepm: 'binancepm',
};

export const isRustVenue = (name: ExchangeName) => RUST_VENUES[name] !== undefined;

export type WasmEngineModule = {
  WasmEngine: new (venue: string, optionsJson: string) => WasmEngineLike;
};

/**
 * How this realm loads the wasm engine: the web build in browsers and
 * Workers, the Node build on servers and in tests. Registered once by the
 * host, like the transport.
 */
let loadWasmEngine: (() => Promise<WasmEngineModule>) | undefined;
export const setWasmEngineLoader = (loader: () => Promise<WasmEngineModule>) => {
  loadWasmEngine = loader;
};

let engineTransport: () => Transport = nativeTransport;
/** Where engine sockets and requests go; tests replace it with a recording. */
export const setRustEngineTransport = (factory: () => Transport) => {
  engineTransport = factory;
};

/** Store sections flush at the worker's store cadence. */
const STORE_FLUSH_MS = 250;

/**
 * An exchange account run by the Rust engine. Venue I/O, parsing and state
 * live in wasm; each flush frame updates this exchange's store through
 * `FrameApplier`, so the worker shell (chasers, TWAP, CLI, post-processors)
 * reads the same store it always has. Trading methods are engine commands.
 */
export class RustEngineExchange extends BaseExchange {
  private host?: EngineHostType;
  private flushTimers: Array<ReturnType<typeof setInterval>> = [];
  private readonly applier: FrameApplier;
  private readonly bookListeners = new Map<string, Set<(book: OrderBook) => void>>();
  private readonly candleListeners = new Map<string, Set<(candle: Candle) => void>>();

  /**
   * Receives each frame after this exchange has applied it. When the worker
   * forwards frames to a main thread that reads them, book and candle
   * listeners here stay silent: main decodes those from the frame itself.
   */
  onFrame?: (frame: ArrayBuffer) => void;

  constructor(exchangeName: ExchangeName, options: ExchangeOptions, store: Store) {
    super(exchangeName, options, store);
    this.applier = new FrameApplier(store);
  }

  start = async () => {
    if (!loadWasmEngine) throw new Error('No wasm engine loader registered (setWasmEngineLoader)');
    const { WasmEngine } = await loadWasmEngine();
    if (this.isDisposed) return;
    const { key, secret, testnet, publicDataOnly } = this.options;
    const engine = new WasmEngine(
      RUST_VENUES[this.name]!,
      JSON.stringify({ key, secret, testnet: Boolean(testnet), publicDataOnly: Boolean(publicDataOnly) }),
    );
    this.host = new EngineHost(engine, engineTransport(), {
      onFrame: this.receiveFrame,
      onLog: this.receiveLog,
    });
    this.host.start();
    // Books, candles and fills go out at the venue's book cadence, the rest
    // at the store's, matching what the TS worker posts today.
    const multiplier = this.options.tuning?.dataUpdateThrottleMultiplier || 1;
    const bookMs = (exchangeMetadata[this.name]?.orderbookTickDelay || STORE_FLUSH_MS) * multiplier;
    this.flushTimers.push(setInterval(() => this.host?.flush(), STORE_FLUSH_MS * multiplier));
    if (bookMs < STORE_FLUSH_MS * multiplier) {
      this.flushTimers.push(setInterval(() => this.host?.flush(true), bookMs));
    }
    await this.untilLoaded();
  };

  /** The whole current state as one frame, for a consumer that starts late. */
  snapshotFrame = () => this.host?.snapshot();

  private receiveFrame = (buffer: ArrayBuffer) => {
    const frame = new Frame(buffer);
    this.applier.apply(frame);
    for (const fill of this.applier.fills(frame)) this.emitter.emit('fill', fill);
    if (!this.onFrame) {
      for (const { symbol, orderbook } of this.applier.books(frame)) {
        this.bookListeners.get(symbol)?.forEach((listener) => listener(orderbook));
      }
      for (const { symbol, interval, candle } of this.applier.candles(frame)) {
        this.candleListeners.get(`${symbol}:${interval}`)?.forEach((listener) => listener(candle));
      }
    }
    this.onFrame?.(buffer);
  };

  private receiveLog = (level: 'info' | 'warning' | 'error', message: string) => {
    if (level === 'error') this.emitAndLogError(message);
    else this.log(message, level === 'warning' ? LogSeverity.Warning : LogSeverity.Info);
  };

  /** Resolves once markets and tickers, and the account unless public-only, have loaded. */
  private untilLoaded = () =>
    new Promise<void>((resolve) => {
      const ready = () => {
        const loaded = this.store.loaded;
        const account = this.options.publicDataOnly || (loaded.balance && loaded.orders && loaded.positions);
        return loaded.markets && loaded.tickers && account;
      };
      if (ready()) return resolve();
      const unsubscribe = this.store.subscribe(() => {
        if (ready() || this.isDisposed) {
          unsubscribe();
          resolve();
        }
      });
    });

  private command = <T>(command: { op: string; [field: string]: unknown }): Promise<T> => {
    if (!this.host) return Promise.reject(new Error(`${this.name} engine is not started`));
    return this.host.command(command) as Promise<T>;
  };

  placeOrders = (orders: PlaceOrderOpts[]) => this.command<string[]>({ op: 'placeOrders', orders });

  placeOrder = (opts: PlaceOrderOpts) => this.placeOrders([opts]);

  updateOrder = ({ order, update }: UpdateOrderOpts) =>
    this.command<string[]>({
      op: 'amendOrder',
      id: order.id,
      ...('price' in update ? { price: update.price } : { amount: update.amount }),
    });

  cancelOrders = async (orders: Order[]) => {
    await this.command({ op: 'cancelOrders', ids: orders.map((order) => order.id) });
  };

  cancelSymbolOrders = async (symbol: string) => {
    await this.command({ op: 'cancelSymbolOrders', symbol });
  };

  setLeverage = async (symbol: string, leverage: number) => {
    await this.command({ op: 'setLeverage', symbol, leverage });
  };

  changeMarginType = async (isolated: boolean, symbol?: string) => {
    if (!symbol) throw new Error('Symbol is required');
    await this.command({ op: 'setMarginType', symbol, isolated });
  };

  changePositionMode = async (hedged: boolean) => {
    await this.command({ op: 'setPositionMode', hedged });
  };

  // The engine reconciles orders and positions itself; reads serve its state.
  fetchOrders = async () => this.store.orders;

  fetchPositions = async () => this.store.positions;

  fetchOHLCV = (opts: OHLCVOptions) =>
    this.command<Candle[]>({
      op: 'fetchCandles',
      symbol: opts.symbol,
      interval: opts.interval,
      limit: opts.limit,
      from: opts.from,
      to: opts.to,
    });

  fetchHistoricalFills = (opts: ExecutionsOptions) =>
    this.command<Execution[]>({ op: 'fetchFills', ...opts });

  fetchHistoricalOrders = (opts?: OrderHistoryOptions) =>
    this.command<HistoricalOrder[]>({
      op: 'fetchOrderHistory',
      symbol: opts?.symbol,
      from: opts?.from,
      to: opts?.to,
      limit: opts?.limit,
    });

  fetchIncomeHistory = (opts?: IncomeHistoryOptions) =>
    this.command<IncomeRecord[]>({
      op: 'fetchIncome',
      symbol: opts?.symbol,
      incomeType: opts?.incomeType,
      from: opts?.startTime,
      to: opts?.endTime,
      limit: opts?.limit,
    });

  fetchIncomeHistoryRange = (startTime: number, endTime: number, incomeType?: string) =>
    this.command<IncomeRecord[]>({ op: 'fetchIncome', incomeType, from: startTime, to: endTime });

  fetchPublicTrades = (symbol: string, limit?: number) =>
    this.command<PublicTrade[]>({ op: 'fetchPublicTrades', symbol, limit });

  listenOrderBook = (symbol: string, callback: (book: OrderBook) => void) => {
    let listeners = this.bookListeners.get(symbol);
    if (!listeners) {
      listeners = new Set();
      this.bookListeners.set(symbol, listeners);
      void this.command({ op: 'subscribeOrderBook', symbol });
    }
    listeners.add(callback);
    return () => {
      listeners!.delete(callback);
      if (listeners!.size === 0 && this.bookListeners.get(symbol) === listeners) {
        this.bookListeners.delete(symbol);
        void this.command({ op: 'unsubscribeOrderBook', symbol });
      }
    };
  };

  listenOHLCV = (opts: OHLCVOptions, callback: (candle: Candle) => void) => {
    const key = `${opts.symbol}:${opts.interval}`;
    let listeners = this.candleListeners.get(key);
    if (!listeners) {
      listeners = new Set();
      this.candleListeners.set(key, listeners);
      void this.command({ op: 'subscribeCandles', symbol: opts.symbol, interval: opts.interval });
    }
    listeners.add(callback);
    return () => {
      listeners!.delete(callback);
      if (listeners!.size === 0 && this.candleListeners.get(key) === listeners) {
        this.candleListeners.delete(key);
        void this.command({ op: 'unsubscribeCandles', symbol: opts.symbol, interval: opts.interval });
      }
    };
  };

  dispose() {
    this.flushTimers.forEach(clearInterval);
    this.flushTimers = [];
    this.host?.dispose();
    this.host = undefined;
    super.dispose();
  }
}
