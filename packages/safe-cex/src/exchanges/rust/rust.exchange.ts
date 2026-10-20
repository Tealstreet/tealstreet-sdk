import type { EngineHost as EngineHostType, Transport, WasmEngineLike } from '@tealstreet/safe-cex-wasm/host';
import type { Store } from '../../store/store.interface';
import type {
  Candle,
  ExchangeAccount,
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
  PublicTrade,
  UpdateOrderOpts,
  ValidateAccountResult,
} from '../../types';
import type { AsterDEXPlaceOrderOpts, AsterDEXStpMode } from '../asterdex/asterdex.types';
import type { ClientOrderIdRule, ClientOrderLookup } from '../clientOrderId';

import bs58 from 'bs58';

import { EngineHost, nativeTransport } from '@tealstreet/safe-cex-wasm/host';
import { Frame } from '@tealstreet/safe-cex-wasm/wire';
import { exchangeMetadata } from '../../exchangeMetadata';
import { wooOrdersToPositionRecords } from '../../journal/woo.position-adapter';
import { FrameApplier } from '../../store/frameApplier';
import { assertEngineAccountRouting, createEngineTransport } from '../../transport/engine';
import { LogSeverity } from '../../types';
import { subscribeTickIntervalMultipliers } from '../../utils/tickIntervalOverride';
import { uuid } from '../../utils/uuid';
import { virtualClock } from '../../utils/virtualClock';
import {
  ASTERDEX_MAIN_WALLET_REQUIRED_MESSAGE,
  hasAsterDEXV3AuthContext,
  isAsterDEXAddress,
  isAsterDEXStpMode,
  isAsterDEXV3PrivateKey,
  resolveAsterDEXApiVersion,
  resolveAsterDEXV3AuthContext,
} from '../asterdex/asterdex.types';
import { BaseExchange, markBaseCapabilityFallback } from '../base';
import { BROKER_ID as BLOFIN_BROKER_ID } from '../blofin/blofin.types';
import {
  HYROTRADER_PROD_URL,
  HYROTRADER_PROD_WS_URL,
  HYROTRADER_SIMULATOR_URL,
  HYROTRADER_SIMULATOR_WS_URL,
} from '../bybit/bybit.types';
import { isRegularClientOrderShape, patternClientOrderIdRule, withReplacementClientOrderId } from '../clientOrderId';
import { hyperliquidClientOrderIdRule } from '../hyperliquid/hyperliquid.cloid';
import { parseLighterApiKeyIndex } from '../lighter/lighter.utils';
import { BROKER_ID as OKX_BROKER_ID } from '../okx/okx.types';
import { BROKER_ID as PHEMEX_BROKER_ID } from '../phemex/phemex.types';
import { txflowClientOrderIdRule } from '../txflow/txflow.client-id';
import { hasSharedTickerValues, RustPublicFeedClient, rustPublicFeedIdentity } from './publicFeed';

/** Venues the Rust engine runs, by exchange name, and its name for each. */
export const RUST_VENUES: Partial<Record<ExchangeName, string>> = {
  coinbase: 'coinbase',
  coinbasespot: 'coinbasespot',
  apexomni: 'apexomni',
  asterdex: 'asterdex',
  breakoutprop: 'breakoutprop',
  binance: 'binance',
  binancespot: 'binancespot',
  binanceinverse: 'binanceinverse',
  backpack: 'backpack',
  'tealstreet-dex': 'tealstreet-dex',
  kodiakfi: 'kodiakfi',
  mode: 'mode',
  salsadex: 'salsadex',
  what: 'what',
  woofi: 'woofi',
  binancepm: 'binancepm',
  bitget: 'bitget',
  bitgetusdc: 'bitgetusdc',
  bitgetinverse: 'bitgetinverse',
  blofin: 'blofin',
  bitrue: 'bitrue',
  grvt: 'grvt',
  mexc: 'mexc',
  woo: 'woo',
  bingx: 'bingx',
  bybit: 'bybit',
  bybitinverse: 'bybitinverse',
  bybitspot: 'bybitspot',
  bybitdemo: 'bybitdemo',
  hyrotrader: 'hyrotrader',
  cryptocom: 'cryptocom',
  kucoin: 'kucoin',
  polymarket: 'polymarket',
  hyperliquid: 'hyperliquid',
  hyperliquidspot: 'hyperliquidspot',
  paradex: 'paradex',
  txflow: 'txflow',
  kraken: 'kraken',
  krakenspot: 'krakenspot',
  lighter: 'lighter',
  'lighter-robinhood': 'lighter-robinhood',
  okx: 'okx',
  okxeu: 'okxeu',
  okxdemo: 'okxdemo',
  okxeudemo: 'okxeudemo',
  weex: 'weex',
  phemex: 'phemex',
};

export const isRustVenue = (name: ExchangeName) => RUST_VENUES[name] !== undefined;

const okxClientOrderIdRule = patternClientOrderIdRule('OKX', /^[A-Za-z0-9]{1,32}$/, {
  mint: () => `${OKX_BROKER_ID}${uuid()}`.slice(0, 32),
});
const bybitClientOrderIdRule = patternClientOrderIdRule('Bybit', /^[A-Za-z0-9_-]{1,36}$/);
const orderlyNames = new Set<ExchangeName>(['tealstreet-dex', 'kodiakfi', 'mode', 'salsadex', 'what', 'woofi']);
const orderlyClientOrderIdRule = patternClientOrderIdRule('Orderly', /^(?!-)[\x00-\x09\x0b\x0c\x0e-\x7f]{1,36}$/);
const bitgetClientOrderIdRule = patternClientOrderIdRule('Bitget', /^[.A-Z:/a-z0-9_-]{1,32}$/);
const coinbaseClientOrderIdRule = patternClientOrderIdRule('Coinbase', /^\S{1,128}$/u, {
  mint: () => `tealstreet-${uuid()}`,
});
const clientOrderIdRules: Partial<Record<ExchangeName, ClientOrderIdRule>> = {
  coinbase: coinbaseClientOrderIdRule,
  coinbasespot: coinbaseClientOrderIdRule,
  phemex: patternClientOrderIdRule('Phemex', /^.{1,40}$/, { mint: () => `${PHEMEX_BROKER_ID}_${uuid()}` }),
  apexomni: patternClientOrderIdRule('ApeX Omni', /^\S+$/),
  binance: patternClientOrderIdRule('Binance', /^[.A-Z:/a-z0-9_-]{1,36}$/, { mint: () => `x-s3vFFRQp${uuid()}` }),
  binancepm: patternClientOrderIdRule('Binance PM', /^[.A-Z:/a-z0-9_-]{1,32}$/, {
    mint: () => `x-s3vFFRQp${uuid()}`.slice(0, 32),
  }),
  binancespot: patternClientOrderIdRule('Binance Spot', /^[A-Za-z0-9_-]{1,36}$/, { mint: () => `x-s3vFFRQp${uuid()}` }),
  'tealstreet-dex': orderlyClientOrderIdRule,
  kodiakfi: orderlyClientOrderIdRule,
  mode: orderlyClientOrderIdRule,
  salsadex: orderlyClientOrderIdRule,
  what: orderlyClientOrderIdRule,
  woofi: orderlyClientOrderIdRule,
  woo: {
    label: 'WOO',
    mint: () => String(100_000_000_000_000 + Math.floor(Math.random() * 900_000_000_000_000)),
    requirement: 'be a canonical positive decimal integer within the JavaScript safe-integer range',
    isValid: (id) => /^[1-9]\d*$/.test(id) && Number.isSafeInteger(Number(id)),
  },
  bitget: bitgetClientOrderIdRule,
  bitgetusdc: bitgetClientOrderIdRule,
  bitgetinverse: bitgetClientOrderIdRule,
  backpack: {
    label: 'Backpack',
    isHonorableShape: (opts) => isRegularClientOrderShape(opts) && opts.close !== true,
    requirement: 'be a canonical decimal uint32 between 1 and 4294967295',
    isValid: (id) => /^[1-9]\d{0,9}$/.test(id) && Number(id) <= 0xffff_ffff,
  },
  hyperliquid: hyperliquidClientOrderIdRule,
  hyperliquidspot: hyperliquidClientOrderIdRule,
  paradex: patternClientOrderIdRule('Paradex', /^.{1,64}$/, {
    mint: () => `ts_${uuid()}`,
    isHonorableShape: (opts) => isRegularClientOrderShape(opts) && opts.close !== true,
  }),
  txflow: txflowClientOrderIdRule,
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
  bitrue: patternClientOrderIdRule('Bitrue', /^.{1,31}$/),
  grvt: {
    label: 'GRVT',
    requirement: 'be a canonical decimal integer between 1 and 9223372036854775807',
    isValid: (id) => /^[1-9]\d{0,18}$/.test(id) && BigInt(id) <= 9223372036854775807n,
  },
  kucoin: patternClientOrderIdRule('KuCoin', /^[A-Za-z0-9_-]{1,40}$/),
  cryptocom: patternClientOrderIdRule('Crypto.com', /^.{1,36}$/),
  weex: patternClientOrderIdRule('WEEX', /^[.A-Z:/a-z0-9_-]{1,36}$/, {
    mint: () => `ts${uuid()}`.slice(0, 36),
  }),
  bybit: bybitClientOrderIdRule,
  bybitinverse: bybitClientOrderIdRule,
  bybitspot: bybitClientOrderIdRule,
  bybitdemo: bybitClientOrderIdRule,
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
let defaultWasmInitialization: Promise<WasmEngineModule> | undefined;
const defaultWasmLoader = (): Promise<WasmEngineModule> =>
  (defaultWasmInitialization ??= (async () => {
    // wasm-bindgen initialization replaces module-wide memory, including the
    // pointers held by existing accounts. Share pending and completed startup.
    const { loadEngine } = await import('@tealstreet/safe-cex-wasm/engine');
    return loadEngine();
  })().catch((error) => {
    defaultWasmInitialization = undefined;
    throw error;
  }));
let loadWasmEngine: () => Promise<WasmEngineModule> = defaultWasmLoader;
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
  private publicFeedDisabled = false;

  get publicFeedMode(): 'direct' | 'disabled' | 'shared' {
    if (this.options.publicFeed?.mode === 'shared') return 'shared';
    return this.publicFeedDisabled || this.isPrivateOnly ? 'disabled' : (this.options.publicFeed?.mode ?? 'direct');
  }

  override disablePublicWebsockets(): void {
    if (this.hasStarted) throw new Error('Disable Rust public sockets before startup');
    this.publicFeedDisabled = true;
  }
  private sharedPublicFeed?: RustPublicFeedClient;
  private requestSharedSnapshot = false;
  private sharedPublicReady = false;
  private readonly publicFeedObservers = new Set<{ updated(catalogChanged: boolean): void; closed(): void }>();

  /** Public providers observe applied native ticker frames, without replacing onFrame. */
  observePublicFeed(observer: { updated(catalogChanged: boolean): void; closed(): void }): () => void {
    this.publicFeedObservers.add(observer);
    return () => {
      this.publicFeedObservers.delete(observer);
    };
  }
  private hostInitialization?: Promise<void>;
  private startup?: Promise<void>;
  private startupFailure?: Error;
  private readonly startupWaiters = new Set<() => void>();
  private flushTimers: Array<ReturnType<typeof setInterval>> = [];
  private readonly applier: FrameApplier;
  private readonly bookListeners = new Map<string, Map<number, (book: OrderBook) => void>>();
  private nextBookListenerId = 0;
  private readonly candleListeners = new Map<string, Set<(candle: Candle) => void>>();
  private readonly feedBookListeners = new Set<(book: OrderBook) => void>();
  private readonly feedCandleListeners = new Set<(candle: Candle) => void>();

  /**
   * Receives each frame after this exchange has applied it. When the worker
   * forwards frames to a main thread that reads them, book and candle
   * listeners here stay silent: main decodes those from the frame itself.
   */
  onFrame?: (frame: ArrayBuffer) => void;
  private stopPollingConfig?: () => void;
  fetchFillsForOrderIds?: (ids: string[]) => Promise<Map<string, number>>;

  constructor(exchangeName: ExchangeName, options: ExchangeOptions, store: Store) {
    super(exchangeName, options, store);
    this.isSpot = ['bybitspot', 'binancespot', 'coinbasespot', 'hyperliquidspot', 'krakenspot'].includes(exchangeName);
    this.isDemo =
      exchangeName === 'bybitdemo' || (exchangeName === 'hyrotrader' && options.extra?.hyrotraderMode === 'demo');
    if (exchangeName === 'polymarket') {
      this.cancelAllOrders = async () => {
        await this.command({ op: 'cancelSymbolOrders', symbol: '' });
      };
      this.fetchSymbolMaxLeverage = async (symbol: string) => {
        const rails = await this.command<FetchedLeverage>({ op: 'fetchLeverage', symbol });
        return Math.max(rails.buyLeverage, rails.sellLeverage);
      };
    }
    if (exchangeName === 'apexomni') {
      this.getAccount = async () => {
        await this.ensureHost();
        return this.command<ExchangeAccount>({ op: 'getAccount' });
      };
      this.fetchHistoricalOrders = markBaseCapabilityFallback('fetchHistoricalOrders', async () => []);
      this.fetchIncomeHistory = markBaseCapabilityFallback('fetchIncomeHistory', async () => []);
      this.fetchIncomeHistoryRange = markBaseCapabilityFallback('fetchIncomeHistoryRange', async () => []);
    }
    if (['hyperliquid', 'grvt', 'paradex'].includes(exchangeName)) {
      this.findOrderByClientOrderId = (symbol: string, clientOrderId: string) =>
        this.command<ClientOrderLookup>({ op: 'findOrderByClientOrderId', symbol, clientOrderId });
    }
    if (exchangeName === 'grvt') {
      this.fetchFillsForOrderIds = async (ids) => {
        const quantities = new Map<string, number>();
        for (const id of new Set(ids)) {
          const fills = await this.command<Execution[]>({ op: 'fetchFills', orderId: id, from: 0, limit: 1000 });
          for (const fill of fills) {
            if (fill.orderId === id) quantities.set(id, (quantities.get(id) ?? 0) + fill.qty);
          }
        }
        return quantities;
      };
    }
    this.applier = new FrameApplier(
      store,
      ['mexc', 'blofin', 'kucoin', 'coinbase', 'coinbasespot', 'okx', 'okxeu', 'okxdemo', 'okxeudemo'].includes(
        this.name,
      ),
      this.name === 'bitgetinverse',
      this.name === 'binanceinverse',
    );
    this.on('activeTickerSymbols:changed', this.updateTickerSymbols);
    if (this.name === 'breakoutprop') {
      this.stopPollingConfig = subscribeTickIntervalMultipliers(() => {
        if (this.host && !this.isDisposed)
          void this.host.command({ op: 'setTickInterval', interval: this.getTickInterval() });
      });
      this.fetchHistoricalFills = markBaseCapabilityFallback('fetchHistoricalFills', async () => []);
      this.fetchHistoricalOrders = markBaseCapabilityFallback('fetchHistoricalOrders', async () => []);
      this.fetchIncomeHistory = markBaseCapabilityFallback('fetchIncomeHistory', async () => []);
      this.fetchIncomeHistoryRange = markBaseCapabilityFallback('fetchIncomeHistoryRange', async () => []);
      this.cancelAllOrders = async () => {
        await this.command({ op: 'cancelSymbolOrders', symbol: '' });
      };
    }
    if (this.name === 'kraken') {
      this.fetchHistoricalPositions = (opts) => this.foldClosedPositionRecords(opts, { symbolScoped: false });
    }
    if (this.name === 'hyperliquidspot' || this.name === 'krakenspot') {
      this.fetchHistoricalFills = markBaseCapabilityFallback('fetchHistoricalFills', async () => []);
      this.fetchHistoricalOrders = markBaseCapabilityFallback('fetchHistoricalOrders', async () => []);
      this.fetchIncomeHistory = markBaseCapabilityFallback('fetchIncomeHistory', async () => []);
      this.fetchIncomeHistoryRange = markBaseCapabilityFallback('fetchIncomeHistoryRange', async () => []);
    }
    if (this.name === 'woo' || this.name === 'grvt') {
      this.cancelAllOrders = async () => {
        await this.command({ op: 'cancelSymbolOrders', symbol: '' });
      };
    }
    if (this.name === 'woo') {
      this.fetchHistoricalPositions = async (opts) => {
        const to = opts.to ?? Date.now();
        const from = opts.from ?? to - 30 * 24 * 60 * 60 * 1000;
        const orders = await this.fetchHistoricalOrders({ from, to });
        return wooOrdersToPositionRecords(orders, {
          account: opts.account,
          from,
          resolveSettleCurrency: (symbol) => this.store.marketsBySymbol[symbol]?.quote || 'USDT',
        });
      };
    }
  }

  get isPublicDataOnly(): boolean {
    if (this.name === 'polymarket') return this.options.publicDataOnly === true || !this.options.key;
    if (this.name === 'paradex') {
      return (
        this.options.publicDataOnly === true ||
        !Boolean(this.options.key && this.options.paradexAddress && this.options.subkeyAddress)
      );
    }
    if (this.name === 'txflow') return this.options.publicDataOnly === true;
    if (['lighter', 'lighter-robinhood'].includes(this.name)) {
      return (
        this.options.publicDataOnly === true ||
        (!this.options.secret && !this.options.remoteSigner) ||
        !this.options.apiKeyIndex
      );
    }
    if (this.name !== 'hyperliquid') return super.isPublicDataOnly;
    return (
      this.options.publicDataOnly === true ||
      !Boolean(this.options.vault || this.options.applicationId || this.options.key)
    );
  }

  protected override get canSign(): boolean {
    if (this.name === 'txflow') return Boolean(this.options.secret && this.options.applicationId);
    if (['coinbase', 'coinbasespot'].includes(this.name)) return Boolean(this.options.key);
    if (this.name !== 'asterdex') return super.canSign;
    if (!this.options.key) return false;
    if (this.options.remoteSigner) return true;
    return resolveAsterDEXApiVersion(this.options) === 'v3'
      ? hasAsterDEXV3AuthContext(this.options)
      : Boolean(this.options.secret);
  }
  private updateTickerSymbols = () => {
    this.sharedPublicFeed?.send({ op: 'tickerSymbols', symbols: this.getActiveTickerSymbols() });
    if (this.host) void this.host.command({ op: 'setTickerSymbols', symbols: this.getActiveTickerSymbols() });
  };

  /** Startup is single-use; metadata factories can replace an already-started instance. */
  get hasStarted(): boolean {
    return this.startup !== undefined;
  }

  // Startup is single-use, including failures; a retry needs a new account instance.
  start = (): Promise<void> => {
    if (this.isDisposed) return Promise.reject(new Error('Exchange disposed during startup'));
    return (this.startup ??= this.startEngine());
  };

  private ensureHost = (): Promise<void> => {
    if (this.isDisposed) return Promise.reject(new Error('Exchange disposed during startup'));
    return (this.hostInitialization ??= this.createHost());
  };

  private createHost = async () => {
    if (this.options.externalPublicData) {
      throw new Error(
        'Rust externalPublicData is unsupported; use publicFeed: { mode: "shared", port } with RustPublicFeedProvider',
      );
    }
    if (this.options.privateOnly && this.options.publicFeed?.mode === 'direct') {
      throw new Error('privateOnly conflicts with direct publicFeed policy');
    }
    assertEngineAccountRouting(this.options, this.name);
    if (this.name === 'breakoutprop' && (!this.options.key || !this.options.secret)) {
      throw new Error('BreakoutProp market data requires a legacy DXtrade username and password');
    }
    if (this.name === 'paradex') {
      if (this.options.payloadSigner || this.options.remoteSigner) {
        throw new Error('Rust Paradex remote signing is unsupported');
      }
      if (this.options.publicDataOnly === false && this.isPublicDataOnly) {
        throw new Error('Rust Paradex private accounts require a local subkey, Paradex address and subkey address');
      }
    }
    if (this.name === 'asterdex' && !this.options.publicDataOnly) {
      const auth = resolveAsterDEXV3AuthContext(this.options);
      if (
        resolveAsterDEXApiVersion(this.options) === 'v1' &&
        isAsterDEXAddress(this.options.key) &&
        isAsterDEXV3PrivateKey(this.options.secret) &&
        !auth.userAddress
      ) {
        throw new Error(ASTERDEX_MAIN_WALLET_REQUIRED_MESSAGE);
      }
    }
    if (this.name === 'hyperliquid' && this.options.enableSpotMarkets) {
      throw new Error('Rust Hyperliquid currently supports perpetual markets only');
    }
    if (
      ![
        'hyperliquid',
        'paradex',
        'lighter',
        'lighter-robinhood',
        'txflow',
        'coinbase',
        'coinbasespot',
        'polymarket',
      ].includes(this.name) &&
      !this.isPublicDataOnly &&
      (!this.options.key ||
        !(this.name === 'asterdex' ? resolveAsterDEXV3AuthContext(this.options).signerPrivateKey : this.options.secret))
    ) {
      throw new Error('Rust engine private accounts require a local API key and secret; remote signing is unsupported');
    }
    if (this.name === 'polymarket' && !this.isPublicDataOnly) {
      if (!/^(?:0x)?[a-fA-F0-9]{64}$/.test(this.options.key)) {
        throw new Error('Polymarket requires a local 32-byte wallet private key');
      }
      const { apiKey, apiSecret, apiPassphrase, signerType = 0, funderAddress } = this.options.polymarket ?? {};
      const credentials = [apiKey, apiSecret, apiPassphrase];
      if (
        credentials.some((value) => value !== undefined) &&
        !credentials.every((value) => typeof value === 'string' && value.trim())
      ) {
        throw new Error('Polymarket requires all three L2 API credentials together');
      }
      if (![0, 1, 2, 3].includes(signerType)) throw new Error('Invalid Polymarket signature type');
      if ((signerType !== 0 || funderAddress !== undefined) && !/^0x[a-fA-F0-9]{40}$/.test(funderAddress ?? '')) {
        throw new Error('Polymarket requires a valid funder address for non-EOA signing');
      }
    }
    if (orderlyNames.has(this.name) && !this.isPublicDataOnly) {
      if (!this.options.applicationId || !/^[A-Za-z0-9_-]+$/.test(this.options.applicationId)) {
        throw new Error('Orderly private applicationId is required');
      }
      for (const key of [this.options.key, this.options.secret]) {
        const encoded = key?.replace(/^ed25519:/, '') ?? '';
        try {
          if (bs58.decode(encoded).length !== 32) throw new Error();
        } catch {
          throw new Error('Orderly requires base58-encoded 32-byte key and secret');
        }
      }
    }
    const { WasmEngine } = await new Promise<WasmEngineModule>((resolve, reject) => {
      const disposed = () => {
        if (!this.isDisposed) return;
        this.startupWaiters.delete(disposed);
        reject(new Error('Exchange disposed during startup'));
      };
      this.startupWaiters.add(disposed);
      try {
        void loadWasmEngine()
          .then(resolve, reject)
          .finally(() => this.startupWaiters.delete(disposed));
      } catch (error) {
        this.startupWaiters.delete(disposed);
        reject(error);
      }
    });
    if (this.isDisposed) throw new Error('Exchange disposed during startup');
    const {
      key,
      secret,
      passphrase,
      omniKeySeed,
      paradexAddress,
      subkeyAddress,
      testnet,
      applicationId,
      vault,
      hyperliquidBuilder,
      enableSpotMarkets,
    } = this.options;
    const publicDataOnly = this.isPublicDataOnly;
    const endpoints = this.name === 'hyrotrader' ? this.hyroEndpoints() : undefined;
    const transport = createEngineTransport(this.name, this.options, engineTransport());
    const engine = new WasmEngine(
      RUST_VENUES[this.name]!,
      JSON.stringify({
        key,
        secret,
        passphrase,
        ...(this.name === 'apexomni' ? { omniKeySeed } : {}),
        testnet: Boolean(testnet),
        publicDataOnly: Boolean(publicDataOnly),
        ...(['coinbase', 'coinbasespot'].includes(this.name) ? { spotOnly: this.name === 'coinbasespot' } : {}),
        extra: this.name === 'kucoin' ? { kucoinBroker: this.options.extra?.kucoinBroker } : undefined,
        endpoints,
        ...(['backpack', 'bitrue', 'breakoutprop'].includes(this.name)
          ? { recvWindow: this.options.extra?.recvWindow }
          : {}),
        applicationId,
        ...(orderlyNames.has(this.name) || this.name === 'breakoutprop'
          ? { tickInterval: this.options.extra?.tickInterval }
          : {}),
        ...(this.name === 'breakoutprop'
          ? {
              subaccount: this.options.subaccount,
              tickInterval: this.getTickInterval(),
              pollingEnabled: this.getTickInterval() !== Number.POSITIVE_INFINITY,
            }
          : {}),
        ...(['lighter', 'lighter-robinhood'].includes(this.name)
          ? {
              apiKeyIndex: parseLighterApiKeyIndex(this.options.apiKeyIndex)?.toString() ?? this.options.apiKeyIndex,
              lighterRemoteSigner: Boolean(this.options.remoteSigner),
              lighterSignerProvider: (this.options.extra as { lighterSignerProvider?: string } | undefined)
                ?.lighterSignerProvider,
              lighterIntegratorAccountIndex: this.options.extra?.lighterIntegratorAccountIndex,
              lighterStopSlippagePct: this.options.extra?.lighterStopSlippagePct,
            }
          : {}),
        vault,
        hyperliquidBuilder,
        enableSpotMarkets: Boolean(enableSpotMarkets),
        ...(this.name === 'grvt'
          ? {
              grvtTradingAccountId: this.options.grvtTradingAccountId,
              grvtSignerAddress: this.options.grvtSignerAddress,
            }
          : {}),
        ...(this.name === 'polymarket' ? { polymarket: this.options.polymarket } : {}),
        ...(this.name === 'paradex' ? { paradexAddress, subkeyAddress } : {}),
        ...(this.name === 'asterdex'
          ? {
              asterdex: {
                apiVersion: resolveAsterDEXApiVersion(this.options),
                userAddress: this.options.auth?.userAddress,
                signerAddress: this.options.auth?.signerAddress,
                privateKey: this.options.auth?.privateKey,
                builderAddress: this.options.extra?.asterBuilderAddress,
                builderFeeRateMaker: this.options.extra?.asterBuilderFeeRateMaker,
                builderFeeRateTaker: this.options.extra?.asterBuilderFeeRateTaker,
                readKey: this.options.secondary_key,
                readSecret: this.options.secondary_secret,
              },
            }
          : {}),
        ...transport.engineOptions,
        publicFeed: { mode: this.publicFeedMode },
        ...(this.name === 'weex' ? { privateWsHeaders: Boolean(transport.webSocketHeaders) } : {}),
      }),
    );
    this.host = new EngineHost(
      engine,
      ['backpack', 'coinbase', 'coinbasespot'].includes(this.name) || orderlyNames.has(this.name)
        ? { ...transport, now: virtualClock.getCurrentTimeMs }
        : transport,
      {
        onFrame: this.receiveFrame,
        onLog: this.receiveLog,
        onDeactivate: () => {
          this.startupFailure = new Error(`${this.name} credentials were rejected`);
          for (const wake of this.startupWaiters) wake();
          this.emitter.emit('deactivate');
        },
      },
    );
  };

  private startEngine = async () => {
    await this.ensureHost();
    if (this.isDisposed || !this.host) throw new Error('Exchange disposed during startup');
    if (this.options.publicFeed?.mode === 'shared') {
      this.sharedPublicFeed = new RustPublicFeedClient(
        this.options.publicFeed.port,
        rustPublicFeedIdentity(this.name, this.options),
        async (update) => {
          await this.host?.command(update);
          const wasReady = this.sharedPublicReady;
          if (
            this.requestSharedSnapshot &&
            update.tickers?.some((ticker) => this.store.marketsBySymbol[ticker.symbol] && hasSharedTickerValues(ticker))
          )
            this.sharedPublicReady = true;
          this.host?.flush();
          // An unchanged provider snapshot produces no frame or store notification.
          if (!wasReady && this.sharedPublicReady) for (const wake of this.startupWaiters) wake();
        },
        (error) => {
          this.startupFailure = error;
          for (const wake of this.startupWaiters) wake();
          this.emitAndLogError(error);
          this.dispose();
        },
      );
      try {
        await this.sharedPublicFeed.connect();
      } catch (error) {
        this.dispose();
        throw error;
      }
      if (this.isDisposed || !this.host) throw new Error('Exchange disposed during startup');
    }
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
    try {
      await this.untilLoaded();
    } catch (error) {
      this.dispose();
      throw error;
    }
  };

  private hyroEndpoints = () => {
    const simulator = this.options.extra?.hyrotraderMode === 'demo' || !HYROTRADER_PROD_URL.livenet;
    const rest = simulator ? HYROTRADER_SIMULATOR_URL : HYROTRADER_PROD_URL;
    const ws = simulator ? HYROTRADER_SIMULATOR_WS_URL : HYROTRADER_PROD_WS_URL;
    const network = this.options.testnet ? 'testnet' : 'livenet';
    if (!rest[network] || !ws.public.linear[network] || !ws.private[network]) {
      throw new Error('HyroTrader REST/public/private URLs are not configured');
    }
    return { rest: rest[network], publicWs: ws.public.linear[network], privateWs: ws.private[network] };
  };

  /** The whole current state as one frame, for a consumer that starts late. */
  snapshotFrame = () => this.host?.snapshot();

  /** A late shared subscriber receives the current native book without waiting for a trade. */
  snapshotOrderBook = (symbol: string): OrderBook | undefined => {
    const snapshot = this.snapshotFrame();
    return snapshot
      ? this.applier.books(new Frame(snapshot)).find((entry) => entry.symbol === symbol)?.orderbook
      : undefined;
  };

  updateSymbolMetadata = async (symbol: string) => {
    if (!['mexc', 'woo', 'kraken'].includes(this.name) || this.isPublicDataOnly) return;
    const rails = await this.command<FetchedLeverage>({ op: 'fetchLeverage', symbol });
    this.store.update({ fetchedLeverage: { ...this.store.fetchedLeverage, [symbol]: rails } });
  };

  private receiveFrame = (buffer: ArrayBuffer) => {
    const frame = new Frame(buffer);
    this.applier.apply(frame, !this.onFrame);
    if (frame.markets() || frame.tickers() || frame.tickerDeltas()) {
      for (const observer of this.publicFeedObservers) observer.updated(Boolean(frame.markets()));
    }
    if (this.sharedPublicFeed && this.store.loaded.markets && (frame.markets() || !this.requestSharedSnapshot)) {
      try {
        this.sharedPublicFeed.validateMarkets(this.store.markets);
      } catch (error) {
        this.startupFailure = error instanceof Error ? error : new Error(String(error));
        this.emitAndLogError(this.startupFailure);
        this.dispose();
        return;
      }
      if (!this.requestSharedSnapshot) {
        this.requestSharedSnapshot = true;
        this.sharedPublicFeed.send({ op: 'snapshot' });
      }
    }
    for (const fill of this.applier.fills(frame)) this.emitter.emit('fill', fill);
    if (!this.onFrame || this.feedBookListeners.size) {
      for (const { symbol, orderbook } of this.applier.books(frame)) {
        const listeners = this.bookListeners.get(symbol);
        if (this.name !== 'asterdex')
          listeners?.forEach((listener) => {
            if (!this.onFrame || this.feedBookListeners.has(listener)) listener(orderbook);
          });
        else if (listeners)
          for (const [id, listener] of [...listeners]) {
            if (listeners.has(id) && (!this.onFrame || this.feedBookListeners.has(listener)))
              listener(
                this.name === 'asterdex'
                  ? {
                      bids: orderbook.bids.map((level) => ({ ...level })),
                      asks: orderbook.asks.map((level) => ({ ...level })),
                    }
                  : orderbook,
              );
          }
      }
    }
    if (!this.onFrame || this.feedCandleListeners.size) {
      for (const { symbol, interval, candle } of this.applier.candles(frame)) {
        this.candleListeners.get(`${symbol}:${interval}`)?.forEach((listener) => {
          if (!this.onFrame || this.feedCandleListeners.has(listener)) listener(candle);
        });
      }
    }
    this.onFrame?.(buffer);
  };

  private receiveLog = (level: 'info' | 'warning' | 'error', message: string) => {
    if (this.name === 'breakoutprop' && level === 'error' && message.startsWith('DXtrade startup failed:')) {
      this.startupFailure = new Error(message);
      for (const wake of this.startupWaiters) wake();
    }
    if (level === 'error') this.emitAndLogError(message);
    else this.log(message, level === 'warning' ? LogSeverity.Warning : LogSeverity.Info);
  };

  /** Resolves once markets and tickers, and the account unless public-only, have loaded. */
  private untilLoaded = () =>
    new Promise<void>((resolve, reject) => {
      const ready = () => {
        const loaded = this.store.loaded;
        const account = this.isPublicDataOnly || (loaded.balance && loaded.orders && loaded.positions);
        return loaded.markets && loaded.tickers && account && (!this.sharedPublicFeed || this.sharedPublicReady);
      };
      let unsubscribe = () => {};
      const wake = () => {
        if (!this.startupFailure && !this.isDisposed && !ready()) return;
        unsubscribe();
        this.startupWaiters.delete(wake);
        if (this.startupFailure || this.isDisposed)
          reject(this.startupFailure ?? new Error('Exchange disposed during startup'));
        else resolve();
      };
      this.startupWaiters.add(wake);
      unsubscribe = this.store.subscribe(wake);
      wake();
    });

  /** Valid once the account's balance loads; the engine's first error says why not. */
  validateAccount = (): Promise<ValidateAccountResult> => {
    if (['lighter', 'lighter-robinhood'].includes(this.name)) {
      const index = parseLighterApiKeyIndex(this.options.apiKeyIndex);
      if (index === undefined || index < 0 || index > 255) {
        return Promise.resolve({ error: 'Invalid API key index. Expected an integer from 0 to 255.' });
      }
    }
    return new Promise((resolve) => {
      let settled = false;
      let started = false;
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
        if (this.startupFailure) return finish({ error: this.startupFailure.message });
        if (this.isDisposed) return finish({ error: 'Exchange disposed during account validation' });
        if (
          started &&
          (this.store.loaded.balance ||
            (this.name === 'breakoutprop' &&
              this.isPublicDataOnly &&
              this.store.loaded.markets &&
              this.store.loaded.tickers))
        )
          finish({ error: '', balance: this.store.balance });
      };
      this.emitter.on('error', onError);
      const unsubscribe = this.store.subscribe(loaded);
      const timeout = setTimeout(() => finish({ error: 'Timed out validating the account' }), 15_000);
      loaded();
      if (!settled)
        this.start().then(
          () => {
            started = true;
            loaded();
          },
          (error) => onError(String(error?.message ?? error)),
        );
    });
  };

  private command = <T>(command: { op: string; [field: string]: unknown }): Promise<T> => {
    if (this.isDisposed) return Promise.reject(new Error(`${this.name} engine disposed`));
    if (!this.host) return Promise.reject(new Error(`${this.name} engine is not started`));
    if (['coinbase', 'coinbasespot'].includes(this.name)) {
      const invalid = (value: unknown): boolean =>
        typeof value === 'number'
          ? !Number.isFinite(value)
          : Boolean(value && typeof value === 'object' && Object.values(value).some(invalid));
      if (invalid(command)) return Promise.reject(new Error('Coinbase command numbers must be finite'));
    }
    return this.host.command(command) as Promise<T>;
  };

  placeOrders = async (orders: AsterDEXPlaceOrderOpts[]) => {
    if (this.name !== 'asterdex' && orders.some((order) => order.stpMode !== undefined)) {
      this.rejectPreflight(`${this.name} does not support AsterDEX stpMode`);
    }
    const isBybit = ['bybit', 'bybitinverse', 'bybitspot', 'bybitdemo', 'hyrotrader'].includes(this.name);
    if (!isBybit && orders.some((order) => 'bybitBbo' in order || 'bybitSlippageToleranceTicks' in order)) {
      this.rejectPreflight(`${this.name} does not support Bybit BBO or TickSize controls`);
    }
    if (!isBybit) {
      for (const order of orders) {
        for (const field of ['amount', 'price', 'triggerPrice', 'stopLoss', 'takeProfit', 'maxSlippagePct'] as const) {
          const value = order[field];
          if (value !== undefined && (typeof value !== 'number' || !Number.isFinite(value))) {
            this.rejectPreflight(`${this.name} ${field} must be finite`);
          }
        }
      }
    }
    this.assertCallerClientOrderIds(orders);
    if (isBybit) {
      for (const order of orders) {
        for (const field of ['amount', 'price', 'triggerPrice', 'stopLoss', 'takeProfit'] as const) {
          const value = order[field];
          if (value !== undefined && (typeof value !== 'number' || !Number.isFinite(value))) {
            this.rejectPreflight(`Bybit ${field} must be finite`);
          }
        }
        if (
          this.name === 'hyrotrader' &&
          (order.maxSlippagePct !== undefined || 'bybitBbo' in order || 'bybitSlippageToleranceTicks' in order)
        ) {
          this.rejectPreflight('HyroTrader does not support client-defined slippage or Bybit execution controls');
        }
        const ticks = 'bybitSlippageToleranceTicks' in order ? order.bybitSlippageToleranceTicks : undefined;
        if (ticks !== undefined && (typeof ticks !== 'number' || !Number.isFinite(ticks))) {
          this.rejectPreflight('Bybit bybitSlippageToleranceTicks must be finite');
        }
        if (order.maxSlippagePct !== undefined && !Number.isFinite(order.maxSlippagePct)) {
          this.rejectPreflight('Bybit maxSlippagePct must be finite');
        }
      }
    }
    if (this.name === 'asterdex') for (const order of orders) this.validateAsterStp(order.stpMode);
    return this.command<string[]>({ op: 'placeOrders', orders });
  };

  placeOrder = async (opts: AsterDEXPlaceOrderOpts) => {
    this.resolveCallerClientOrderId(opts);
    return this.placeOrders([opts]);
  };

  updateOrder = async ({ order, update }: UpdateOrderOpts) => {
    const price = 'price' in update ? update.price : undefined;
    const amount = 'amount' in update ? update.amount : undefined;
    if (price === undefined && amount === undefined) throw new Error('Order amendment requires a price or amount');
    if ([price, amount].some((value) => value !== undefined && !Number.isFinite(value))) {
      throw new Error('Order amendment price and amount must be finite numbers');
    }
    const result = await this.command<string[] | { orderIds: string[]; clientOrderId: string }>({
      op: 'amendOrder',
      id: order.id,
      ...('price' in update ? { price: update.price } : {}),
      ...('amount' in update ? { amount: update.amount } : {}),
    });
    return Array.isArray(result) ? result : withReplacementClientOrderId(result.orderIds, result.clientOrderId);
  };

  cancelOrders = async (orders: Order[]) => {
    if (this.name === 'asterdex') {
      await this.command({ op: 'cancelOrdersWithSymbols', orders: orders.map(({ id, symbol }) => ({ id, symbol })) });
    } else await this.command({ op: 'cancelOrders', ids: orders.map((order) => order.id) });
  };

  cancelSymbolOrders = async (symbol: string) => {
    await this.command({ op: 'cancelSymbolOrders', symbol });
  };

  getAccount = async (): Promise<ExchangeAccount> => {
    if (['coinbase', 'coinbasespot'].includes(this.name)) {
      if (!this.host) await this.start();
      await this.untilLoaded();
      const userId = this.host?.accountId();
      if (!userId || this.isPublicDataOnly) throw new Error('Coinbase private portfolio ID is unavailable');
      return { userId };
    }
    if (this.name === 'asterdex' || this.name === 'polymarket')
      return this.command<ExchangeAccount>({ op: 'fetchAccount' });
    if (!orderlyNames.has(this.name)) throw new Error('Not implemented');
    if (!this.host) await this.start();
    await this.untilLoaded();
    const accountId = this.options.applicationId;
    if (!accountId || this.isPublicDataOnly) throw new Error('Orderly private account ID is required');
    return { userId: accountId, affiliateId: accountId };
  };

  setAllLeverage = async (leverage: number) => {
    if (orderlyNames.has(this.name)) await this.command({ op: 'setLeverage', symbol: '', leverage });
    else {
      for (const position of this.store.positions) {
        if (!this.isDisposed) await this.setLeverage(position.symbol, leverage);
      }
    }
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

  private validateAsterStp = (mode?: AsterDEXStpMode): void => {
    if (mode === undefined) return;
    if (resolveAsterDEXApiVersion(this.options) !== 'v3')
      throw new Error('AsterDEX self-trade prevention requires API v3');
    if (!isAsterDEXStpMode(mode)) throw new Error(`Invalid AsterDEX self-trade-prevention mode: ${String(mode)}`);
  };

  fetchStpMode = async (): Promise<AsterDEXStpMode> => {
    if (this.name !== 'asterdex') throw new Error('Self-trade prevention is unavailable for this venue');
    this.validateAsterStp('EXPIRE_TAKER');
    return this.command<AsterDEXStpMode>({ op: 'fetchStpMode' });
  };

  changeStpMode = async (mode: AsterDEXStpMode): Promise<void> => {
    if (this.name !== 'asterdex') throw new Error('Self-trade prevention is unavailable for this venue');
    this.validateAsterStp('EXPIRE_TAKER');
    if (!isAsterDEXStpMode(mode)) throw new Error(`Invalid AsterDEX self-trade-prevention mode: ${String(mode)}`);
    await this.command({ op: 'setStpMode', mode });
  };

  // The engine reconciles orders and positions itself; reads serve its state.
  fetchMarkets = async () => this.store.markets;

  fetchTickers = async () => this.store.tickers;

  /** Native reconciliation owns refresh; account consumers read the current balance. */
  fetchBalance = async () => this.store.balance;

  fetchOrders = async (symbols?: string[]) =>
    symbols ? this.store.orders.filter((order) => symbols.includes(order.symbol)) : this.store.orders;

  fetchPositions = async (symbols?: string[]) =>
    symbols ? this.store.positions.filter((position) => symbols.includes(position.symbol)) : this.store.positions;

  fetchPositionMode = async () => Boolean(this.store.options.isHedged);

  fetchSymbolMaxLeverage = async (symbol: string): Promise<number> => {
    if (this.name === 'breakoutprop') throw new Error('Not implemented');
    const market = this.store.marketsBySymbol[symbol];
    if (!market) throw new Error(`Market not found for symbol: ${symbol}`);
    return market.limits.leverage.max;
  };

  fetchOrderBook = async (symbol: string, limit?: number): Promise<OrderBook> => {
    if (this.name !== 'asterdex' && this.name !== 'polymarket')
      throw new Error('Orderbook snapshot queries are unavailable for this venue');
    return this.command<OrderBook>({
      op: 'fetchOrderBook',
      symbol,
      limit: this.name === 'asterdex' ? (limit ?? 100) : limit,
    });
  };

  private candleInterval = (interval: OHLCVOptions['interval']): OHLCVOptions['interval'] =>
    this.name === 'asterdex' &&
    !['1m', '3m', '5m', '15m', '30m', '1h', '2h', '4h', '6h', '12h', '1d', '1w'].includes(interval)
      ? '1h'
      : interval;

  fetchOHLCV = (opts: OHLCVOptions) =>
    this.command<Candle[]>({
      op: 'fetchCandles',
      symbol: opts.symbol,
      interval: this.candleInterval(opts.interval),
      limit: opts.limit,
      from: opts.from,
      to: opts.to,
    });

  fetchHistoricalFills = (opts: ExecutionsOptions) => {
    if (
      ['coinbase', 'coinbasespot'].includes(this.name) &&
      [opts.from, opts.to].some((value) => value !== undefined && !Number.isFinite(value))
    ) {
      return Promise.reject(new Error('Coinbase history bounds must be finite'));
    }
    const milliseconds = (value?: number) => {
      if (value === undefined || !Number.isFinite(value) || (this.name === 'woo' && value < 0)) return undefined;
      const alreadyMilliseconds = this.name === 'woo' ? value >= 1_000_000_000_000 : value > 1_000_000_000_000;
      return Math.floor(alreadyMilliseconds ? value : value * 1000);
    };
    return this.command<Execution[]>({
      op: 'fetchFills',
      ...opts,
      ...([
        'apexomni',
        'blofin',
        'bitrue',
        'grvt',
        'hyperliquid',
        'paradex',
        'lighter',
        'lighter-robinhood',
        'kraken',
        'woo',
        'coinbase',
        'coinbasespot',
      ].includes(this.name)
        ? { from: milliseconds(opts.from), to: milliseconds(opts.to) }
        : {}),
    });
  };

  fetchLeverageForSymbol = async (symbol: string): Promise<FetchedLeverage | null> => {
    try {
      let leverage: FetchedLeverage;
      if (
        [
          'asterdex',
          'blofin',
          'bitrue',
          'grvt',
          'paradex',
          'txflow',
          'coinbase',
          'coinbasespot',
          'polymarket',
        ].includes(this.name) ||
        orderlyNames.has(this.name)
      )
        leverage = await this.command<FetchedLeverage>({ op: 'fetchLeverage', symbol });
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

  fetchHistoricalOrders = (opts?: OrderHistoryOptions) => {
    if (this.name === 'txflow' && (opts?.orderId !== undefined || opts?.status !== undefined)) {
      return Promise.reject(new Error('TxFlow order history does not support orderId or status filters'));
    }
    if (
      ['coinbase', 'coinbasespot'].includes(this.name) &&
      (opts?.orderId !== undefined || opts?.status !== undefined)
    ) {
      return Promise.reject(new Error('Coinbase order history does not support orderId or status filters'));
    }
    if (orderlyNames.has(this.name) && (opts?.orderId !== undefined || opts?.status !== undefined)) {
      return Promise.reject(new Error('Orderly order history does not support orderId or status filters'));
    }
    return this.command<HistoricalOrder[]>({
      op: 'fetchOrderHistory',
      symbol: opts?.symbol,
      from: opts?.from,
      to: opts?.to,
      limit: opts?.limit,
    });
  };

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
      listeners = new Map();
      this.bookListeners.set(symbol, listeners);
      if (this.sharedPublicFeed) this.sharedPublicFeed.send({ op: 'book', symbol, subscribe: true });
      void this.command({ op: 'subscribeOrderBook', symbol });
    }
    const listenerId =
      (this.name !== 'asterdex' ? [...listeners].find(([, listener]) => listener === callback)?.[0] : undefined) ??
      ++this.nextBookListenerId;
    listeners.set(listenerId, callback);
    return () => {
      if (!listeners!.delete(listenerId)) return;
      if (listeners!.size === 0 && this.bookListeners.get(symbol) === listeners) {
        this.bookListeners.delete(symbol);
        if (this.sharedPublicFeed) this.sharedPublicFeed.send({ op: 'book', symbol, subscribe: false });
        void this.command({ op: 'unsubscribeOrderBook', symbol });
      }
    };
  };

  /** Provider listeners still receive data when normal callbacks are handled by the frame consumer. */
  listenOrderBookForFeed = (symbol: string, callback: (book: OrderBook) => void) => {
    const listener = (book: OrderBook) => callback(book);
    this.feedBookListeners.add(listener);
    const stop = this.listenOrderBook(symbol, listener);
    return () => {
      this.feedBookListeners.delete(listener);
      stop();
    };
  };

  listenOHLCV = (opts: OHLCVOptions, callback: (candle: Candle) => void) => {
    const interval = this.candleInterval(opts.interval);
    const key = `${opts.symbol}:${interval}`;
    let listeners = this.candleListeners.get(key);
    if (!listeners) {
      listeners = new Set();
      this.candleListeners.set(key, listeners);
      if (this.sharedPublicFeed)
        this.sharedPublicFeed.send({ op: 'candle', symbol: opts.symbol, interval, subscribe: true });
      void this.command({ op: 'subscribeCandles', symbol: opts.symbol, interval });
    }
    listeners.add(callback);
    return () => {
      listeners!.delete(callback);
      if (listeners!.size === 0 && this.candleListeners.get(key) === listeners) {
        this.candleListeners.delete(key);
        if (this.sharedPublicFeed)
          this.sharedPublicFeed.send({ op: 'candle', symbol: opts.symbol, interval, subscribe: false });
        void this.command({ op: 'unsubscribeCandles', symbol: opts.symbol, interval });
      }
    };
  };

  candleIntervalForFeed = (interval: OHLCVOptions['interval']) => this.candleInterval(interval);

  listenOHLCVForFeed = (opts: OHLCVOptions, callback: (candle: Candle) => void) => {
    const listener = (candle: Candle) => callback(candle);
    this.feedCandleListeners.add(listener);
    const stop = this.listenOHLCV(opts, listener);
    return () => {
      this.feedCandleListeners.delete(listener);
      stop();
    };
  };

  dispose() {
    this.sharedPublicFeed?.dispose();
    this.sharedPublicFeed = undefined;
    for (const observer of this.publicFeedObservers) observer.closed();
    this.publicFeedObservers.clear();
    this.feedBookListeners.clear();
    this.feedCandleListeners.clear();
    this.stopPollingConfig?.();
    this.stopPollingConfig = undefined;
    this.flushTimers.forEach(clearInterval);
    this.flushTimers = [];
    this.host?.dispose();
    this.host = undefined;
    super.dispose();
    for (const wake of this.startupWaiters) wake();
  }
}
