import type { EngineHost as EngineHostType, Transport, WasmEngineLike } from '@tealstreet/safe-cex-wasm/host';
import type { Store } from '../../store/store.interface';
import type {
  Candle,
  ExchangeName,
  ExchangeOptions,
  Execution,
  ExecutionsOptions,
  FetchedLeverage,
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
  ValidateAccountResult,
} from '../../types';
import type { ClientOrderIdRule } from '../clientOrderId';

import { EngineHost, nativeTransport } from '@tealstreet/safe-cex-wasm/host';
import { Frame } from '@tealstreet/safe-cex-wasm/wire';
import { exchangeMetadata } from '../../exchangeMetadata';
import { FrameApplier } from '../../store/frameApplier';
import { LogSeverity } from '../../types';
import { uuid } from '../../utils/uuid';
import { virtualClock } from '../../utils/virtualClock';
import { BaseExchange } from '../base';
import { BROKER_ID as BLOFIN_BROKER_ID } from '../blofin/blofin.types';
import { isRegularClientOrderShape, patternClientOrderIdRule } from '../clientOrderId';
import { BROKER_ID as OKX_BROKER_ID } from '../okx/okx.types';

/** Venues the Rust engine runs, by exchange name, and its name for each. */
export const RUST_VENUES: Partial<Record<ExchangeName, string>> = {
  binance: 'binance',
  backpack: 'backpack',
  binancepm: 'binancepm',
  bitget: 'bitget',
  bitgetusdc: 'bitgetusdc',
  bitgetinverse: 'bitgetinverse',
  blofin: 'blofin',
  mexc: 'mexc',
  bingx: 'bingx',
  bybit: 'bybit',
  cryptocom: 'cryptocom',
  okx: 'okx',
  okxeu: 'okxeu',
  okxdemo: 'okxdemo',
  okxeudemo: 'okxeudemo',
  weex: 'weex',
};

export const isRustVenue = (name: ExchangeName) => RUST_VENUES[name] !== undefined;

const okxClientOrderIdRule = patternClientOrderIdRule('OKX', /^[A-Za-z0-9]{1,32}$/, {
  mint: () => `${OKX_BROKER_ID}${uuid()}`.slice(0, 32),
});
const clientOrderIdRules: Partial<Record<ExchangeName, ClientOrderIdRule>> = {
  backpack: {
    label: 'Backpack',
    requirement: 'be a canonical decimal uint32 between 1 and 4294967295',
    isValid: (id) => /^[1-9]\d{0,9}$/.test(id) && Number(id) <= 0xffff_ffff,
  },
  blofin: patternClientOrderIdRule('BloFin', /^[A-Za-z0-9]{1,32}$/, {
    mint: () => `${BLOFIN_BROKER_ID}${uuid()}`.slice(0, 32),
  }),
  bingx: {
    label: 'BingX',
    isHonorableShape: (opts) => isRegularClientOrderShape(opts) && opts.close !== true,
    mint: () => `ts${uuid()}`.toLowerCase().slice(0, 40),
    requirement: 'be 1-40 characters with no uppercase letters',
    isValid: (id) => id.length >= 1 && id.length <= 40 && id === id.toLowerCase(),
  },
  bybit: patternClientOrderIdRule('Bybit', /^[A-Za-z0-9_-]{1,36}$/),
  okx: okxClientOrderIdRule,
  okxeu: okxClientOrderIdRule,
  okxdemo: okxClientOrderIdRule,
  okxeudemo: okxClientOrderIdRule,
};

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
  protected readonly clientOrderIdRule = clientOrderIdRules[this.name];
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
    this.applier = new FrameApplier(store, this.name === 'mexc');
    this.on('activeTickerSymbols:changed', this.updateTickerSymbols);
  }

  private updateTickerSymbols = () => {
    if (this.host) void this.host.command({ op: 'setTickerSymbols', symbols: this.getActiveTickerSymbols() });
  };

  start = async () => {
    if (!this.isPublicDataOnly && (!this.options.key || !this.options.secret)) {
      throw new Error('Rust engine private accounts require a local API key and secret; remote signing is unsupported');
    }
    if (!loadWasmEngine) throw new Error('No wasm engine loader registered (setWasmEngineLoader)');
    const { WasmEngine } = await loadWasmEngine();
    if (this.isDisposed) return;
    const { key, secret, passphrase, testnet } = this.options;
    const publicDataOnly = this.isPublicDataOnly;
    const transport = engineTransport();
    const engine = new WasmEngine(
      RUST_VENUES[this.name]!,
      JSON.stringify({
        key,
        secret,
        passphrase,
        testnet: Boolean(testnet),
        publicDataOnly: Boolean(publicDataOnly),
        recvWindow: this.options.extra?.recvWindow,
        privateWsHeaders: Boolean(transport.webSocketHeaders),
      }),
    );
    this.host = new EngineHost(
      engine,
      this.name === 'backpack'
        ? {
            webSocketHeaders: transport.webSocketHeaders,
            now: virtualClock.getCurrentTimeMs,
            connect: transport.connect.bind(transport),
            request: transport.request.bind(transport),
            setTimeout: transport.setTimeout.bind(transport),
            clearTimeout: transport.clearTimeout.bind(transport),
          }
        : transport,
      {
        onFrame: this.receiveFrame,
        onLog: this.receiveLog,
      },
    );
    this.host.start();
    this.updateTickerSymbols();
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

  updateSymbolMetadata = async (symbol: string) => {
    if (this.name !== 'mexc' || this.options.publicDataOnly) return;
    const rails = await this.command<FetchedLeverage>({ op: 'fetchLeverage', symbol });
    this.store.update({ fetchedLeverage: { ...this.store.fetchedLeverage, [symbol]: rails } });
  };

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
        const account = this.isPublicDataOnly || (loaded.balance && loaded.orders && loaded.positions);
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

  /** Valid once the account's balance loads; the engine's first error says why not. */
  validateAccount = (): Promise<ValidateAccountResult> =>
    new Promise((resolve) => {
      let settled = false;
      const finish = (result: ValidateAccountResult) => {
        if (settled) return;
        settled = true;
        this.emitter.off('error', onError);
        unsubscribe();
        clearTimeout(timeout);
        resolve(result);
      };
      const onError = (message: unknown) =>
        finish({ error: typeof message === 'string' && message ? message : 'Invalid API key or secret' });
      const loaded = () => {
        if (this.store.loaded.balance) finish({ error: '', balance: this.store.balance });
      };
      this.emitter.on('error', onError);
      const unsubscribe = this.store.subscribe(loaded);
      const timeout = setTimeout(() => finish({ error: 'Timed out validating the account' }), 15_000);
      loaded();
      if (!this.host) this.start().catch((error) => onError(String(error?.message ?? error)));
    });

  private command = <T>(command: { op: string; [field: string]: unknown }): Promise<T> => {
    if (!this.host) return Promise.reject(new Error(`${this.name} engine is not started`));
    return this.host.command(command) as Promise<T>;
  };

  placeOrders = async (orders: PlaceOrderOpts[]) => {
    if (this.name === 'bybit' || this.name === 'backpack' || this.name === 'bingx')
      this.assertCallerClientOrderIds(orders);
    return this.command<string[]>({ op: 'placeOrders', orders });
  };

  placeOrder = (opts: PlaceOrderOpts) => this.placeOrders([opts]);

  updateOrder = ({ order, update }: UpdateOrderOpts): Promise<string[]> => {
    const price = 'price' in update ? update.price : undefined;
    const amount = 'amount' in update ? update.amount : undefined;
    if (price === undefined && amount === undefined) {
      return Promise.reject(new Error('Order amendment requires a price or amount'));
    }
    if ([price, amount].some((value) => value !== undefined && !Number.isFinite(value))) {
      return Promise.reject(new Error('Order amendment price and amount must be finite numbers'));
    }
    return this.command<string[]>({
      op: 'amendOrder',
      id: order.id,
      ...(price !== undefined && { price }),
      ...(amount !== undefined && { amount }),
    });
  };

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
    if (!symbol && this.name !== 'blofin') throw new Error('Symbol is required');
    await this.command({ op: 'setMarginType', symbol: symbol ?? '', isolated });
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

  fetchHistoricalFills = (opts: ExecutionsOptions) => {
    const milliseconds = (value?: number) =>
      value === undefined || !Number.isFinite(value)
        ? undefined
        : Math.floor(value > 1_000_000_000_000 ? value : value * 1000);
    return this.command<Execution[]>({
      op: 'fetchFills',
      ...opts,
      ...(this.name === 'blofin' ? { from: milliseconds(opts.from), to: milliseconds(opts.to) } : {}),
    });
  };

  fetchLeverageForSymbol = async (symbol: string): Promise<FetchedLeverage | null> => {
    try {
      let leverage: FetchedLeverage;
      if (this.name === 'blofin') leverage = await this.command<FetchedLeverage>({ op: 'fetchLeverage', symbol });
      else {
        const value = await this.fetchSymbolMaxLeverage(symbol);
        leverage = { buyLeverage: value, sellLeverage: value };
      }
      this.host?.flush();
      this.store.update({ fetchedLeverage: { ...this.store.fetchedLeverage, [symbol]: leverage } });
      return leverage;
    } catch (error) {
      this.log(`Error fetching leverage for ${symbol}: ${error}`, LogSeverity.Warning);
      return null;
    }
  };

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
