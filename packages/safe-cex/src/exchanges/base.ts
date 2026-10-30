import type { EngineEvent } from '@tealstreet/safe-cex-wasm/host';
import type { EngineCapabilities } from '@tealstreet/safe-cex-wasm/wire';
import type { Axios, AxiosInstance } from 'axios';
import type { TinyEmitter } from 'tiny-emitter';
import type { PositionRecord } from '../journal/position.types';
import type { Store } from '../store/store.interface';
import type { EnhancedAxiosError } from '../utils/axiosErrorEnhancer';
import type { HttpFailureRecord } from '../utils/httpFailureContext';

import { sha256 } from '@noble/hashes/sha2.js';
import { bytesToHex, utf8ToBytes } from '@noble/hashes/utils.js';
import createHmac from 'create-hmac';
import uniq from 'lodash/uniq';
import { nanoid } from 'nanoid';
import { forEachSeries, mapSeries } from 'p-iteration';

import { sleep } from '@tealstreet/sleep/src/sleep';
import { classifiedError, deserializeCommandError, exchangeErrorTags } from '../commandOutcome';
import { createApiWithSecondaryKey } from '../createApiWithSecondaryKey';
import { exchangeMetadata, getExchangeMetadata } from '../exchangeMetadata';
import { chaserOrderUpdates } from '../webworker/postProcessors/ChaserOrderEnrichmentPostprocessor';
import { buildFoldedPositionRecords } from '../journal/buildRecords';
import { withHistoryCompleteness } from '../historyCompleteness';
import {
  AccountGreeks,
  Candle,
  ClosedPnlEntry,
  ExchangeAccount,
  ExchangeName,
  ExchangeOptions,
  Execution,
  ExecutionsOptions,
  FetchedLeverage,
  HistoricalOrder,
  HistoricalTradeOptions,
  IncomeHistoryOptions,
  IncomeRecord,
  LogSeverity,
  Market,
  OHLCVOptions,
  OHLCVSOptions,
  OptionChainQuery,
  OptionChainRow,
  Order,
  OrderBook,
  OrderHistoryOptions,
  OrderSide,
  OrderType,
  PlaceOrderOpts,
  Position,
  PositionSide,
  PublicTrade,
  RemoteSignerRequest,
  RemoteSignerResponse,
  StoreData,
  Ticker,
  Timeframe,
  UpdateOrderOpts,
  ValidateAccountResult,
} from '../types';
import { formatErrorForSentry } from '../utils/axiosErrorEnhancer';
import { readHttpFailureContext, recordHttpFailure } from '../utils/httpFailureContext';
import { adjust } from '../utils/safe-math';
import { getTickIntervalMultiplier } from '../utils/tickIntervalOverride';
import { signHttpCapture } from '../utils/signHttpCapture';
import { BaseWebSocket } from './base.ws';
import {
  clientOrderIdShapeMessage,
  clientOrderIdSplitMessage,
  defaultMintCandidates,
  isRegularClientOrderShape,
  orderCarriesClientOrderId,
  type ClientOrderIdPlaceOrderOpts,
  type ClientOrderLookup,
  type ClientOrderIdRule,
  type PlacedOrderRef,
} from './clientOrderId';
import { ExternalPublicDataProvider } from './external-public-data';
import { reportException } from '../telemetry';
import { InProcessPublicDataProvider } from './in-process-public-data';
import { Frame } from '@tealstreet/safe-cex-wasm/wire';
import { FrameApplier, FrameApplyError } from '../store/frameApplier';
import { ExchangeEmitter } from './exchangeEmitter';

const normalizeSymbols = (symbols: string[]): string[] => {
  const deduped = new Set<string>();

  symbols.forEach((symbol) => {
    if (typeof symbol === 'string' && symbol.trim()) {
      deduped.add(symbol);
    }
  });

  return Array.from(deduped);
};

/** Minimal shape of the reconnect diagnostics a socket exposes. */
interface SocketDiagnosticsSource {
  lastCloseCode?: number;
  lastCloseAt?: number;
  lifetimeCloseCount?: number;
  onCloseCount?: number;
  isReconnectPauseActive?: boolean;
}

export const summarizeSocketDiagnostics = <K extends 'public' | 'private'>(
  sockets: readonly SocketDiagnosticsSource[],
  kind: K,
) => {
  let closeCode: number | undefined;
  let closeAt = -1;
  let closeCountTotal = 0;
  let closeStreak = 0;
  let paused = false;

  for (const socket of sockets) {
    if (!socket) continue;

    closeCountTotal += socket.lifetimeCloseCount ?? 0;
    closeStreak += socket.onCloseCount ?? 0;
    if (socket.isReconnectPauseActive) paused = true;

    // Most-recent close wins; a socket that never closed has lastCloseAt 0.
    const at = socket.lastCloseAt ?? 0;
    if (socket.lastCloseCode !== undefined && at > closeAt) {
      closeAt = at;
      closeCode = socket.lastCloseCode;
    }
  }

  return {
    [`${kind}CloseCode`]: closeCode,
    [`${kind}CloseCountTotal`]: closeCountTotal,
    [`${kind}CloseStreak`]: closeStreak,
    [`${kind}ReconnectPaused`]: paused,
  } as Record<`${K}CloseCode`, number | undefined> &
    Record<`${K}CloseCountTotal` | `${K}CloseStreak`, number> &
    Record<`${K}ReconnectPaused`, boolean>;
};

interface SocketHealthSource extends SocketDiagnosticsSource {
  isInitialized: boolean;
  isHealthy: boolean;
  lastMessageTimestamp?: number;
  isRecoveryComplete?: boolean;
}

interface PublicDataSource {
  isInitialized: boolean;
  isHealthy: boolean;
  lastMessageTimestamp?: number;
}

/**
 * The store's `socketHealth` from an exchange's sockets. A gateway provider
 * feeds public data without a socket of its own and still counts as a source.
 */
export const summarizeSocketHealth = (
  publicSockets: readonly SocketHealthSource[],
  privateSockets: readonly SocketHealthSource[],
  gatewayProvider?: PublicDataSource,
): NonNullable<StoreData['socketHealth']> => {
  const initializedPublicSockets = publicSockets.filter((s) => s.isInitialized);
  const initializedGatewayProvider = gatewayProvider?.isInitialized ? gatewayProvider : undefined;
  const hasInitializedPublicSource = initializedPublicSockets.length > 0 || !!initializedGatewayProvider;
  const publicHealthy =
    !hasInitializedPublicSource ||
    initializedPublicSockets.some((s) => s.isHealthy) ||
    initializedGatewayProvider?.isHealthy === true;
  const publicTimestamps = initializedPublicSockets.map((s) => s.lastMessageTimestamp || 0);
  if (initializedGatewayProvider) {
    publicTimestamps.push(initializedGatewayProvider.lastMessageTimestamp || 0);
  }
  const publicLastMessage = publicTimestamps.length > 0 ? Math.max(...publicTimestamps) || undefined : undefined;

  const initializedPrivateSockets = privateSockets.filter((s) => s.isInitialized);
  const privateHealthy =
    privateSockets.every((socket) => socket.isRecoveryComplete !== false) &&
    (initializedPrivateSockets.length === 0 || initializedPrivateSockets.some((s) => s.isHealthy));
  const privateTimestamps = initializedPrivateSockets.map((s) => s.lastMessageTimestamp || 0);
  const privateLastMessage = privateTimestamps.length > 0 ? Math.max(...privateTimestamps) || undefined : undefined;

  return {
    publicHealthy,
    privateHealthy,
    publicInitialized: hasInitializedPublicSource,
    privateInitialized: initializedPrivateSockets.length > 0,
    publicLastMessage,
    privateLastMessage,
    ...summarizeSocketDiagnostics(publicSockets, 'public'),
    ...summarizeSocketDiagnostics(privateSockets, 'private'),
  };
};

// 7 doublings caps a 500ms tick at 60s; the streak stops growing there so a
// long outage can't push recovery latency past a minute.
const AUTH_FAILURE_MAX_STREAK = 7;
const AUTH_FAILURE_MAX_TICK_INTERVAL = 60_000;

/**
 * BaseExchange supplies a few deliberately inert fallbacks so that older
 * integrations can omit optional methods.  Metadata must never advertise one
 * of those fallbacks as an implemented capability.  Brand the actual runtime
 * function rather than inspecting source text; derived class fields replace
 * the branded function during construction.  This is intentionally only a
 * non-fallback baseline: a venue-specific override still needs behavioral
 * regressions and, where applicable, credentialed validation.
 */
const baseCapabilityFallback = Symbol('baseCapabilityFallback');

export type BaseCapabilityMethod =
  | 'fetchClosedPnl'
  | 'fetchHistoricalFills'
  | 'fetchIncomeHistory'
  | 'fetchIncomeHistoryRange'
  | 'fetchHistoricalOrders'
  | 'placeOrder'
  | 'fetchOrders'
  | 'fetchPositions'
  | 'start'
  | 'fetchOptionChain'
  | 'fetchOptionExpiries'
  | 'fetchAccountGreeks';

type BrandedCapabilityFallback = {
  [baseCapabilityFallback]?: BaseCapabilityMethod;
};

export const markBaseCapabilityFallback = <T extends object>(method: BaseCapabilityMethod, fn: T): T => {
  Object.defineProperty(fn, baseCapabilityFallback, {
    value: method,
    configurable: false,
    enumerable: false,
    writable: false,
  });
  return fn;
};

const isBaseCapabilityFallback = (value: unknown, method: BaseCapabilityMethod): boolean =>
  typeof value === 'function' && (value as BrandedCapabilityFallback)[baseCapabilityFallback] === method;

/**
 * Is this override an unconditional `return []`?
 *
 * `isBaseCapabilityFallback` answers "was this never implemented", which is not the same
 * question as "does this do anything". `coinbase` and `phemex` both define
 * `fetchHistoricalTrades` whose entire body is `return []` — a real override, unbranded, and
 * therefore indistinguishable from a working implementation to every capability check we
 * had. That is how a metadata flag stays "accurate" while pointing at nothing.
 *
 * Strict on purpose. Many genuine implementations OPEN with a guard clause —
 * `if (this.isPublicDataOnly) return [];` — and must not be classified as stubs, so the test
 * is that the body, once comments are stripped, is exactly that one statement and nothing
 * else.
 */
const isEmptyStubCapability = (value: unknown): boolean => {
  if (typeof value !== 'function') return false;

  const source = Function.prototype.toString
    .call(value)
    // Strip comments first, so a commented-out implementation cannot disguise an empty body.
    .replace(/\/\*[\s\S]*?\*\//g, '')
    .replace(/\/\/[^\n]*/g, '');

  const brace = source.indexOf('{');
  if (brace === -1) {
    // Concise arrow body with no block: `async () => []`. Same inert shape, different syntax
    // — and the one the base fallbacks themselves are written in, so missing it would make
    // this function blind to exactly the case it exists for.
    const arrow = source.indexOf('=>');
    if (arrow === -1) return false;
    return /^\s*\[\s*\]\s*;?\s*$/.test(source.slice(arrow + 2));
  }

  const body = source.slice(brace + 1, source.lastIndexOf('}'));
  return /^\s*return\s*\[\s*\]\s*;?\s*$/.test(body);
};

/**
 * Does this method do nothing — whether because it was never implemented, or because its
 * implementation is an empty stub? This is the question a capability flag should be checked
 * against; `isBaseCapabilityFallback` alone answers only half of it.
 */
const isInertCapability = (value: unknown, method: BaseCapabilityMethod): boolean =>
  isBaseCapabilityFallback(value, method) || isEmptyStubCapability(value);

/** The method behind each capability; a capability is present when its method does something. */
export const CAPABILITY_METHODS = {
  startup: 'start',
  trading: 'placeOrder',
  openOrders: 'fetchOrders',
  positions: 'fetchPositions',
  fillHistory: 'fetchHistoricalFills',
  orderHistory: 'fetchHistoricalOrders',
  incomeHistory: 'fetchIncomeHistory',
  incomeRange: 'fetchIncomeHistoryRange',
  closedPnl: 'fetchClosedPnl',
  optionChain: 'fetchOptionChain',
  optionExpiries: 'fetchOptionExpiries',
  accountGreeks: 'fetchAccountGreeks',
} as const satisfies Record<string, BaseCapabilityMethod>;

/** What this venue can answer. The only supported way to ask; never inspect the methods. */
export type ExchangeCapabilities = Record<keyof typeof CAPABILITY_METHODS, boolean>;

export type EngineSnapshot = {
  frame: ArrayBuffer;
  fetchedLeverage: Record<string, FetchedLeverage>;
};

/** Methods a main-thread Rust mirror forwards whole to its worker's engine. */
export const WORKER_FORWARDED_METHODS = [
  'refreshMarkets',
  'placeOrder',
  'placeOrders',
  'placeOrderWithClientOrderId',
  'updateOrder',
  'cancelOrders',
  'cancelSymbolOrders',
  'cancelAllOrders',
  'findOrderByClientOrderId',
  'nuke',
  'nukePositions',
  'nukeOrders',
  'fetchPublicTrades',
  'fetchOHLCV',
  'fetchHistoricalFills',
  'fetchHistoricalOrders',
  'fetchIncomeHistory',
  'fetchIncomeHistoryRange',
] as const;

export type WorkerForwardedMethod = (typeof WORKER_FORWARDED_METHODS)[number];

/**
 * Methods only some exchanges define (Rust capability-adopted ones), forwarded
 * by name; the worker rejects them where its exchange lacks the method.
 */
export const WORKER_OPTIONAL_METHODS = [
  'fetchHistoricalPositions',
  // Rust raw closed-position rows, which a main-thread mirror maps into its journal.
  'fetchClosedPositions',
  'changeAccountTier',
  'fetchAccountTier',
  'fetchEngineCapabilities',
  // Rust native account reads; a main-thread mirror asks the worker's engine.
  'getAccount',
  'fetchPositions',
  'fetchBalance',
  'fetchPositionMode',
] as const;

const WORKER_READ_METHODS: ReadonlySet<string> = new Set([
  'refreshMarkets',
  'fetchPositions',
  'fetchBalance',
  'fetchPositionMode',
  'findOrderByClientOrderId',
  'fetchPublicTrades',
  'fetchOHLCV',
  'fetchHistoricalFills',
  'fetchHistoricalOrders',
  'fetchIncomeHistory',
  'fetchIncomeHistoryRange',
  'fetchHistoricalPositions',
  'fetchClosedPositions',
  'fetchAccountTier',
  'fetchEngineCapabilities',
  'getAccount',
]);

/** Worker RPCs that dispatch venue writes: an interrupted one has an unknown outcome. */
export const WORKER_WRITE_METHODS: ReadonlySet<string> = new Set<string>([
  'setLeverage',
  'scheduleCancel',
  'changePositionMode',
  'changeMarginType',
  ...[...WORKER_FORWARDED_METHODS, ...WORKER_OPTIONAL_METHODS].filter((method) => !WORKER_READ_METHODS.has(method)),
]);

/** Caller client order IDs a worker placement RPC carries, for an interrupted write's outcome. */
export const workerCallClientOrderIds = (method: unknown, args: unknown): string[] | undefined => {
  if (!Array.isArray(args)) return undefined;
  let orders: unknown[] = [];
  if (method === 'placeOrders' && Array.isArray(args[0])) orders = args[0];
  else if (method === 'placeOrder' || method === 'placeOrderWithClientOrderId') orders = [args[0]];
  const ids = orders
    .map((order) => (order as { clientOrderId?: unknown } | null | undefined)?.clientOrderId)
    .filter((id): id is string => typeof id === 'string');
  return ids.length ? ids : undefined;
};

export type CallableMethodsInWorker =
  | WorkerForwardedMethod
  | 'setLeverage'
  | 'scheduleCancel'
  | 'changePositionMode'
  | 'changeMarginType'
  | 'fetchAndSetPrivateSnapshot'
  | 'fetchEngineSnapshot'
  | 'reconnect'
  | 'recoverAfterOffline'
  | 'fetchOHLCV'
  | 'fetchHistoricalFills'
  | 'fetchOrders'
  | 'fetchAndSetOrders'
  | 'updateSymbolMetadata'
  | 'fetchLastLogsFromWebsockets'
  | 'removeOrders'
  | 'fetchIncomeHistory'
  | 'fetchIncomeHistoryRange'
  | 'fetchHistoricalOrders'
  | 'fetchClosedPnl';

export interface Exchange {
  // udf endpoints for TV, if supported
  initTime: number;
  udfEndpoint?: string;
  udfEndpointTestnet?: string;

  publicWebsocket?: BaseWebSocket<any>;
  publicWebsocketSecondary?: BaseWebSocket<any>;
  publicInverseWebsocket?: BaseWebSocket<any>;
  privateWebsocket?: BaseWebSocket<any>;
  businessWebsocket?: BaseWebSocket<any>;
  futuresWebsocket?: BaseWebSocket<any>;

  /**
   * External public data provider for WebSocket multiplexing or in-process sharing.
   * When configured, the exchange receives public data from an external source.
   */
  externalPublicDataProvider?: ExternalPublicDataProvider | InProcessPublicDataProvider;

  get name(): ExchangeName;
  get isOnline(): boolean;
  /** What this venue can answer. Rust venues know theirs once the engine exists. */
  get capabilities(): ExchangeCapabilities;
  /** `capabilities`, once the venue can know them (Rust creates its engine, without starting it). */
  loadCapabilities: () => Promise<ExchangeCapabilities>;
  /**
   * Returns true if this exchange is running in public-data-only mode.
   */
  get isPublicDataOnly(): boolean;
  /**
   * Returns true if this exchange uses external public data from a gateway.
   */
  get usesExternalPublicData(): boolean;
  /**
   * Returns true if this exchange only connects private WS (public data from shared exchange).
   */
  get isPrivateOnly(): boolean;
  /**
   * Disable public websocket connections (replace connectAndSubscribe with no-ops).
   */
  disablePublicWebsockets(): void;
  setOnlineStatus: (isOnline: boolean) => void;

  store: Store;
  emitter: TinyEmitter;
  options: ExchangeOptions;
  isDisposed: boolean;
  on: TinyEmitter['on'];
  once: TinyEmitter['once'];
  off: TinyEmitter['off'];
  dispose: () => void;
  getAccount: () => Promise<ExchangeAccount>;
  validateAccount: () => Promise<ValidateAccountResult>;
  fetchPositions: (symbols?: string[]) => Promise<Position[]>;
  fetchOrders: (symbols?: string[]) => Promise<Order[]>;
  syncAccountState: () => void;
  start: () => Promise<void>;
  nuke: (tries?: number) => Promise<void>;
  nukePositions: (tries?: number) => Promise<void>;
  nukeOrders: () => Promise<void>;
  changePositionMode: (hedged: boolean, symbol?: string) => Promise<void>;
  changeMarginType: (isolated: boolean, symbol?: string) => Promise<void>;
  setLeverage: (symbol: string, leverage: number, side?: PositionSide) => Promise<void>;
  setAllLeverage: (leverage: number) => Promise<void>;
  placeOrder: (opts: PlaceOrderOpts) => Promise<string[]>;
  placeOrders: (orders: PlaceOrderOpts[]) => Promise<string[]>;
  placeOrderWithClientOrderId: (opts: ClientOrderIdPlaceOrderOpts) => Promise<PlacedOrderRef>;
  mintClientOrderId: (opts: PlaceOrderOpts) => string | undefined;
  findOrderByClientOrderId: (symbol: string, clientOrderId: string) => Promise<ClientOrderLookup>;
  updateOrder: (opts: UpdateOrderOpts) => Promise<string[]>;
  cancelOrders: (orders: Order[]) => Promise<void>;
  cancelSymbolOrders: (symbol: string) => Promise<void>;
  cancelAllOrders: () => Promise<void>;
  scheduleCancel: (time?: number | null) => Promise<void>;
  fetchOHLCV: (opts: OHLCVOptions) => Promise<Candle[]>;
  fetchHistoricalFills: (opts: ExecutionsOptions) => Promise<Execution[]>;
  refreshMarkets: () => Promise<{ added: Market[]; removed: Market[] }>;
  fetchMarkets?: () => Promise<Market[]>;
  fetchTickers?: () => Promise<Ticker[]>;
  fetchSymbolMaxLeverage?: (symbol: string) => Promise<number>;
  fetchLeverageForSymbol: (symbol: string) => Promise<FetchedLeverage | null>;
  fetchEngineSnapshot: () => Promise<EngineSnapshot>;
  updateSymbolMetadata: (symbol: string) => Promise<void>;
  fetchClosedPnl: (opts: HistoricalTradeOptions) => Promise<ClosedPnlEntry[]>;
  fetchIncomeHistory: (opts?: IncomeHistoryOptions) => Promise<IncomeRecord[]>;
  fetchHistoricalOrders: (opts?: OrderHistoryOptions) => Promise<HistoricalOrder[]>;
  fetchOptionChain: (query: OptionChainQuery) => Promise<OptionChainRow[]>;
  fetchOptionExpiries: (underlying: string) => Promise<number[]>;
  fetchAccountGreeks: () => Promise<AccountGreeks[]>;
  fetchIncomeHistoryRange: (startTime: number, endTime: number, incomeType?: string) => Promise<IncomeRecord[]>;
  fetchPublicTrades: (symbol: string, limit?: number) => Promise<PublicTrade[]>;
  getActiveTickerSymbols: () => string[];
  setActiveTickerSymbols: (symbols: string[]) => void;
  listenOHLCV: (o: OHLCVOptions, c: (c: Candle) => void) => () => void;
  listenOHLCVs: (o: OHLCVSOptions, c: (c: Candle & { symbol: string; interval: Timeframe }) => void) => void;
  listenOrderBook: (s: string, c: (o: OrderBook) => void) => () => void;
  listenOHLCVWebworker: (o: Pick<OHLCVOptions, 'symbol' | 'interval'>, c: (c: Candle) => void) => () => void;
  listenOrderBookWebworker: (s: string, c: (o: OrderBook) => void) => () => void;
  updatePositionsInWebworker: (symbols?: string[]) => Promise<void>;
  callMethodInWorker: <M extends CallableMethodsInWorker>(
    method: M,
    ...args: Parameters<this[M]>
  ) => Promise<ReturnType<this[M]>>;
  afterMethodCalledInWorker: <M extends CallableMethodsInWorker>(
    result: ReturnType<this[M]>,
    method: M,
    ...args: Parameters<this[M]>
  ) => void;
  fetchAndSetPrivateSnapshot: () => Promise<void>;
  fetchAndSetOrders: (symbols?: string[]) => Promise<Order[]>;
  removeOrders: (orders: Pick<Order, 'id'>[]) => void;
  reconnect: () => void;
  recoverAfterOffline: () => boolean;
  getOrFetchPositionsForSymbol: (symbol: string) => Promise<Position[]>;
  getAllSockets(): BaseWebSocket<any>[];
  getPublicSockets(): BaseWebSocket<any>[];
  getPrivateSockets(): BaseWebSocket<any>[];
  syncSocketHealthToStore?: () => void;
  fetchLastLogsFromWebsockets: () => string[];

  /**
   * Sign a remote HTTP request using this exchange's axios interceptors.
   * Returns signed headers, params, data via the capture adapter pattern.
   */
  signHttpRequest: (request: RemoteSignerRequest) => Promise<RemoteSignerResponse>;

  /**
   * Sign a remote WS auth request using this exchange's secret.
   * Returns the signature string for WebSocket authentication.
   */
  signWsAuth: (request: RemoteSignerRequest) => Promise<RemoteSignerResponse>;
}

export type AccountMarketOverlay = (markets: Market[]) => Market[];

export class BaseExchange implements Exchange {
  isDemo = false;
  isSpot = false;
  initTime = Date.now();

  _exchangeName: ExchangeName;
  store: Store;
  options: ExchangeOptions;
  emitter: ExchangeEmitter;

  publicWebsocket?: BaseWebSocket<any>;
  publicWebsocketSecondary?: BaseWebSocket<any>;
  publicInverseWebsocket?: BaseWebSocket<any>;
  privateWebsocket?: BaseWebSocket<any>;
  futuresWebsocket?: BaseWebSocket<any>;
  businessWebsocket?: BaseWebSocket<any>;
  secondsTickersWebsocket?: BaseWebSocket<any>;
  thirdTickersWebsocket?: BaseWebSocket<any>;

  /**
   * External public data provider for WebSocket multiplexing or in-process sharing.
   * When set (via ExchangeOptions.externalPublicData), the exchange receives
   * public data from an external source instead of connecting directly to the exchange.
   */
  externalPublicDataProvider?: ExternalPublicDataProvider | InProcessPublicDataProvider;

  isDisposed: boolean = false;
  isNuking: boolean = false;
  lastReconnectEmit = 0;
  private _activeTickerSymbols: string[] = [];

  // Online/offline state - used to prevent reconnection spam when network is down
  private _isOnline: boolean = true;

  // Watchdog timer for detecting stuck websockets
  private watchdogTimer?: NodeJS.Timeout;
  private readonly WATCHDOG_INTERVAL_MS = 10_000; // Check every 10 seconds
  private readonly WATCHDOG_STALE_THRESHOLD_MS = 30_000; // Consider stale after 30s without messages
  private readonly WATCHDOG_CONNECTION_TIMEOUT_MS = 60_000; // Max time to wait for first message after init
  private _lastHttpFailure?: HttpFailureRecord;
  private remoteSignerAvailabilityCleanups = new Set<() => void>();
  private readonly unsubscribeFromStore: () => void;
  private applyingWorkerChaserTags = false;

  on: TinyEmitter['on'];
  once: TinyEmitter['once'];
  off: TinyEmitter['off'];

  grafanaEndpointsEvery: string[] = [];
  grafanaEndpointsPeriodic: string[] = [];

  // Raw primary/secondary REST clients, populated by createXhrWithSecondaryKey.
  // Useful for explicitly hitting one key (e.g. validating both on startup).
  _primaryXhr?: Axios;
  _secondaryXhr?: Axios;

  /**
   * Build the authenticated REST client, routing GET (read) requests to the
   * secondary (read-only) key's client when one is configured. Writes stay on
   * the primary key. Stores the raw primary/secondary clients on the instance.
   * When no secondary key is set, all three are the same client — behaviour is
   * identical to a plain createApi() call, so wiring this in is safe/default-off.
   */
  protected createXhrWithSecondaryKey(
    opts: ExchangeOptions,
    createApi: (options: ExchangeOptions, exchange: BaseExchange) => AxiosInstance,
    otherOptions?: {
      proxyUrl?: string;
      rateLimit?: (axiosInstance: AxiosInstance) => AxiosInstance;
      secondaryRateLimit?: (axiosInstance: AxiosInstance) => AxiosInstance;
    },
  ): AxiosInstance {
    const [xhr, primaryXhr, secondaryXhr] = createApiWithSecondaryKey(opts, this, createApi, otherOptions);
    this._primaryXhr = primaryXhr;
    this._secondaryXhr = secondaryXhr;
    return xhr;
  }

  constructor(exchangeName: ExchangeName, opts: ExchangeOptions, store: Store) {
    this._exchangeName = exchangeName;
    this.options = opts;
    this.store = store;
    this.emitter = new ExchangeEmitter(exchangeName);
    this._activeTickerSymbols = normalizeSymbols(opts.tuning?.activeTickerSymbols || []);

    this.on = this.emitter.on.bind(this.emitter);
    this.once = this.emitter.once.bind(this.emitter);
    this.off = this.emitter.off.bind(this.emitter);

    this.unsubscribeFromStore = this.store.subscribe((data) => {
      // Frames carry native orders; chaser tags arrive independently with the
      // worker's derived fields. Reapply on either notification order.
      if (this.workerFrameApplier && !this.applyingWorkerChaserTags) {
        const orders = chaserOrderUpdates(data);
        if (orders.length) {
          this.applyingWorkerChaserTags = true;
          try {
            this.store.addOrUpdateOrders(orders.map(({ order, extra }) => ({ ...order, extra })));
          } finally {
            this.applyingWorkerChaserTags = false;
          }
          return;
        }
      }
      this.emitter.emit('update', data);
    });
  }

  getActiveTickerSymbols = () => {
    return this._activeTickerSymbols;
  };

  setActiveTickerSymbols = (symbols: string[]) => {
    const next = normalizeSymbols(symbols);

    if (
      next.length === this._activeTickerSymbols.length &&
      next.every((symbol, index) => symbol === this._activeTickerSymbols[index])
    ) {
      return;
    }

    this._activeTickerSymbols = next;
    this.emitter.emit('activeTickerSymbols:changed', next);
  };

  protected getTickerSymbolsForSubscription(availableSymbols: string[]) {
    const activeSymbols = this.getActiveTickerSymbols();
    if (!activeSymbols.length) {
      return availableSymbols;
    }

    const activeSet = new Set(activeSymbols);
    const filteredSymbols = availableSymbols.filter((symbol) => activeSet.has(symbol));

    return filteredSymbols.length ? filteredSymbols : availableSymbols;
  }

  onWSPublicClose = () => {};

  /**
   * Returns true if this exchange can authenticate requests —
   * either locally (key + secret) or via remote signing (key + remoteSigner or payloadSigner).
   */
  protected get canSign(): boolean {
    return !!this.options.key && (!!this.options.secret || !!this.options.remoteSigner || !!this.options.payloadSigner);
  }

  private _authFailureStreak = 0;

  /**
   * Per-exchange REST tick interval. Metadata is the source of truth;
   * `options.extra.tickInterval` is an explicit override (the web wrapper's
   * browser-side slowdown multiplier, dev knobs). Single source of truth
   * means non-web consumers (exchange-server, CLI, mobile) read the same
   * value as the browser without an options-builder having to inject it.
   *
   * If metadata is missing and there is no override, returns undefined and
   * `loop()` warns at the use site. The fix is to set `tickInterval` on the
   * exchange's metadata entry — not to add another fallback here.
   *
   * Scaled by `_authFailureStreak` so a rejected key stops hammering the venue.
   */
  protected getTickInterval(): number | undefined {
    const override = this.options.extra?.tickInterval;
    if (override === Number.POSITIVE_INFINITY) return override;
    const configured = override && override > 0 ? override : getExchangeMetadata(this._exchangeName).tickInterval;

    // Server-pushed slowdown for a venue that is rate-limiting us. Applied to the
    // BASE, before the auth-failure backoff, so the two compose the way you would
    // want during an incident: a slowed venue with a rejected key still tops out
    // at the same ceiling rather than multiplying past it.
    const base = configured ? configured * getTickIntervalMultiplier(this._exchangeName) : configured;

    if (!base || this._authFailureStreak === 0) return base;

    return Math.min(base * 2 ** this._authFailureStreak, AUTH_FAILURE_MAX_TICK_INTERVAL);
  }

  /**
   * Exponential tick backoff for an account whose credentials the venue keeps
   * rejecting. A bad key otherwise polls forever at the metadata tick interval
   * — bybit at 500ms emitted 120 errors in a 30s window, and nothing anywhere
   * backed off. Backoff rather than a hard stop, so fixing the key or riding
   * out venue maintenance still recovers on its own.
   *
   * Call from the transport layer, which is the only place that sees the HTTP
   * status; classification from an error *message* is unreliable.
   */
  recordAuthFailure() {
    if (this._authFailureStreak < AUTH_FAILURE_MAX_STREAK) this._authFailureStreak += 1;
  }

  recordAuthSuccess() {
    this._authFailureStreak = 0;
  }

  /** Native adapters aggregate fixed signer/scope streaks under the same Base cap. */
  protected setAuthFailureStreak(streak: number): number {
    if (!Number.isFinite(streak)) throw new RangeError('Authentication streak must be finite');
    return (this._authFailureStreak = Math.max(0, Math.min(AUTH_FAILURE_MAX_STREAK, Math.trunc(streak))));
  }

  /** Test/diagnostic accessor — not part of the consumer contract. */
  get authFailureStreak() {
    return this._authFailureStreak;
  }

  /**
   * Returns true if this exchange is in public-data-only mode
   * (no credentials or explicitly flagged — no private WebSocket, no trading).
   */
  get isPublicDataOnly(): boolean {
    return this.options.publicDataOnly === true || !this.canSign;
  }

  loadCapabilities = async (): Promise<ExchangeCapabilities> => this.capabilities;

  get capabilities(): ExchangeCapabilities {
    const answers = {} as ExchangeCapabilities;
    for (const [capability, method] of Object.entries(CAPABILITY_METHODS) as Array<
      [keyof ExchangeCapabilities, BaseCapabilityMethod]
    >)
      answers[capability] = !isInertCapability(this[method], method);
    return answers;
  }

  /**
   * Returns true if this exchange should only connect private WebSocket.
   * Public data (tickers, orderbook, OHLCV) comes from a shared public exchange.
   */
  get isPrivateOnly(): boolean {
    return this.options.privateOnly === true;
  }

  /**
   * Returns true if this exchange is configured to use external public data
   * from a gateway instead of direct WebSocket connections.
   */
  get usesExternalPublicData(): boolean {
    return !!this.options.externalPublicData;
  }

  /**
   * Disable all public websocket connections by replacing connectAndSubscribe with no-ops.
   * Called externally (e.g., by ExchangeWorker) when privateOnly is true, after construction
   * but before start(). This avoids modifying every exchange's start() method.
   */
  disablePublicWebsockets(): void {
    const noop = () => {};
    if (this.publicWebsocket) this.publicWebsocket.connectAndSubscribe = noop;
    if (this.publicWebsocketSecondary) this.publicWebsocketSecondary.connectAndSubscribe = noop;
    if (this.publicInverseWebsocket) this.publicInverseWebsocket.connectAndSubscribe = noop;
    // Also patch secondsTickersWebsocket and thirdTickersWebsocket if present (Binance)
    if (this.secondsTickersWebsocket) this.secondsTickersWebsocket.connectAndSubscribe = noop;
    if (this.thirdTickersWebsocket) this.thirdTickersWebsocket.connectAndSubscribe = noop;
  }

  /** Account-specific transform over shared public markets; null until loaded. */
  private accountMarketOverlay: AccountMarketOverlay | null = null;

  /**
   * Applies this account's own market data over markets from a shared public
   * exchange, which has no keys and so cannot know it. Called by both public
   * data providers on every market delivery.
   */
  adaptExternalMarkets(markets: Market[]): Market[] {
    return this.accountMarketOverlay ? this.accountMarketOverlay(markets) : markets;
  }

  /** True when markets come from a shared keyless exchange but this account can sign. */
  protected get needsAccountMarketOverlay(): boolean {
    return Boolean(this.options.externalPublicData) && !this.isPublicDataOnly;
  }

  /**
   * Call from start() BEFORE initExternalPublicData: the hosted store delta
   * ignores nested market fields, so an overlay applied after the first
   * snapshot would not reach the client until the next full snapshot.
   * The overlay must return new objects — shared markets are referenced by
   * every other account on the venue.
   */
  protected loadAccountMarketOverlay = async (load: () => Promise<AccountMarketOverlay | null>) => {
    try {
      this.accountMarketOverlay = await load();
      if (this.isDisposed || this.store.markets.length === 0) return;
      this.store.update({ markets: this.adaptExternalMarkets(this.store.markets) });
    } catch (err: any) {
      const message = err?.response?.data?.msg || err?.message;
      if (err instanceof Error) this.emitAndLogError(err, message ? `Exchange Error: ${message}` : undefined);
      else this.emitAndLogError(message);
    }
  };

  /**
   * Initialize external public data provider if configured.
   * Call this in the exchange's start() method BEFORE connecting public websockets.
   * If external public data is configured, this method will:
   * 1. Create the ExternalPublicDataProvider or InProcessPublicDataProvider
   * 2. Connect to the source
   * 3. Wait until markets AND tickers are loaded in the store
   * 4. Return true (so the exchange knows to skip direct public WS connection)
   *
   * @returns true if external public data is being used, false otherwise
   */
  protected async initExternalPublicData(): Promise<boolean> {
    const config = this.options.externalPublicData;
    if (!config) {
      return false;
    }

    // Validate that the exchange supports public-only mode
    const meta = exchangeMetadata[this._exchangeName];
    if (meta && !meta.supportsPublicOnly) {
      throw new Error(
        `Exchange "${this._exchangeName}" does not support public-only mode (supportsPublicOnly is false). ` +
          `Cannot use externalPublicData with this exchange.`,
      );
    }

    if (config.type === 'websocket') {
      this.log(`Using external public data gateway: ${config.url}`, LogSeverity.Info);

      this.externalPublicDataProvider = new ExternalPublicDataProvider(this, {
        url: config.url,
        exchangeId: config.exchangeId,
        authToken: config.authToken,
      });

      this.externalPublicDataProvider.connect();
    } else if (config.type === 'in-process') {
      this.log('Using in-process public data source', LogSeverity.Info);

      this.externalPublicDataProvider = new InProcessPublicDataProvider(this, config.source);
      this.externalPublicDataProvider.connect();
    }

    // Wait until markets and tickers are loaded from the external source
    // before returning, so the exchange's private startup has data available.
    await this.waitForExternalPublicDataLoaded();

    return true;
  }

  /**
   * Wait until both markets and tickers are loaded in the store.
   * Used after connecting an external public data provider to ensure
   * data is available before the exchange proceeds with private startup.
   */
  private waitForExternalPublicDataLoaded(): Promise<void> {
    const { loaded } = this.store;
    if (loaded.markets && loaded.tickers) {
      return Promise.resolve();
    }

    const TIMEOUT_MS = 30_000;
    const POLL_INTERVAL_MS = 100;

    return new Promise<void>((resolve) => {
      let pollTimer: ReturnType<typeof setTimeout> | undefined;

      const cleanup = () => {
        if (pollTimer) clearTimeout(pollTimer);
        clearTimeout(timer);
      };

      const timer = setTimeout(() => {
        cleanup();
        this.log('Timeout waiting for external public data (markets/tickers)', LogSeverity.Warning);
        // Resolve anyway — let the exchange proceed; it may still work
        // if data arrives shortly after (e.g., private startup is slow).
        resolve();
      }, TIMEOUT_MS);

      const poll = () => {
        if (this.isDisposed) {
          cleanup();
          resolve();
          return;
        }

        const { loaded: l } = this.store;
        if (l.markets && l.tickers) {
          cleanup();
          this.log('External public data loaded (markets + tickers)', LogSeverity.Info);
          resolve();
          return;
        }

        pollTimer = setTimeout(poll, POLL_INTERVAL_MS);
      };

      pollTimer = setTimeout(poll, POLL_INTERVAL_MS);
    });
  }

  getAccount = async () => {
    await Promise.reject(new Error('Not implemented'));
    return {} as ExchangeAccount;
  };

  validateAccount = async (): Promise<ValidateAccountResult> => {
    await Promise.reject(new Error('Not implemented'));
    return { error: '' };
  };

  fetchPositions = async (_symbols?: string[]): Promise<Position[]> => {
    await Promise.reject(new Error('Not implemented'));
    return [];
  };

  fetchOrders = async (_symbols?: string[]): Promise<Order[]> => {
    await Promise.reject(new Error('Not implemented'));
    return [];
  };

  /** Synchronize an in-process native account mirror before a store-based decision. No network refresh. */
  syncAccountState = (): void => {};

  fetchMarkets = async (): Promise<Market[]> => {
    await Promise.reject(new Error('Not implemented'));
    return [];
  };

  fetchTickers = async (): Promise<Ticker[]> => {
    await Promise.reject(new Error('Not implemented'));
    return [];
  };

  /**
   * Re-fetch markets from the exchange and update the store.
   * Emits 'markets:changed' with { added, removed } if the market list changed.
   * Per-symbol WS handlers can listen for this event to subscribe to new tickers.
   */
  refreshMarkets = async (): Promise<{ added: Market[]; removed: Market[] }> => {
    const oldMarkets = new Map(this.store.markets.map((m) => [m.id, m]));
    const newMarkets = await this.fetchMarkets();

    const added = newMarkets.filter((m) => !oldMarkets.has(m.id));
    const removed = Array.from(oldMarkets.values()).filter((m) => !newMarkets.find((n) => n.id === m.id));

    if (added.length > 0 || removed.length > 0) {
      this.store.update({ markets: newMarkets });
      this.emitter.emit('markets:changed', { added, removed });
      this.log(
        `Markets refreshed: +${added.length} -${removed.length} (total: ${newMarkets.length})`,
        LogSeverity.Info,
      );
    }

    return { added, removed };
  };

  log = (message: string, severity: LogSeverity = LogSeverity.Info, engineEvent?: EngineEvent) => {
    if (this.options.tuning?.consoleLogging !== false) {
      console.log(message, severity);
    }
    if (engineEvent) this.emitter.emit('log', `[${this.name}] ${message}`, severity, engineEvent);
    else this.emitter.emit('log', `[${this.name}] ${message}`, severity);
  };

  start = markBaseCapabilityFallback('start', async () => {
    await Promise.reject(new Error('Not implemented'));
  });

  fetchSymbolMaxLeverage = async (_symbol: string) => {
    await Promise.reject(new Error('Not implemented'));
    return 1;
  };

  updateSymbolMetadata = async (_symbol: string) => {
    // Silently do nothing in base implementation
  };

  setLeverage = async (_symbol: string, _leverage: number) => {
    await Promise.reject(new Error('Not implemented'));
  };

  fetchLeverageForSymbol = async (symbol: string): Promise<FetchedLeverage | null> => {
    try {
      const leverage = await this.fetchSymbolMaxLeverage(symbol);
      const leverageObj: FetchedLeverage = {
        buyLeverage: leverage,
        sellLeverage: leverage,
      };
      this.store.update({
        fetchedLeverage: {
          ...this.store.fetchedLeverage,
          [symbol]: leverageObj,
        },
      });
      return leverageObj;
    } catch (err) {
      this.log(`Error fetching leverage for ${symbol}: ${err}`, LogSeverity.Warning);
      return null;
    }
  };

  changePositionMode = async (_hedged: boolean) => {
    await Promise.reject(new Error('Not implemented'));
  };

  changeMarginType = async (_isolated: boolean) => {
    await Promise.reject(new Error('Not implemented'));
  };

  setAllLeverage = async (inputLeverage: number) => {
    await forEachSeries(this.store.positions, async (position) => {
      if (!this.isDisposed) {
        await this.setLeverage(position.symbol, inputLeverage);
      }
    });
  };

  placeOrder = markBaseCapabilityFallback('placeOrder', async (_opts: PlaceOrderOpts) => {
    await Promise.reject(new Error('Not implemented'));
    return [] as string[];
  });

  placeOrders = async (orders: PlaceOrderOpts[]) => {
    this.assertCallerClientOrderIds(orders);
    const orderIds = await mapSeries(orders, (order) => this.placeOrder(order));
    return orderIds.flat();
  };

  // The returned clientOrderId is the wire value: honoring adapters send the
  // caller ID verbatim or reject, so it is never recomposed here.
  placeOrderWithClientOrderId = async (opts: ClientOrderIdPlaceOrderOpts): Promise<PlacedOrderRef> => {
    // A worker or JS caller can drop the field; never place an untracked order in its name.
    if (typeof opts.clientOrderId !== 'string') {
      return this.rejectPreflight(`${this._exchangeName} placeOrderWithClientOrderId requires a clientOrderId`);
    }
    const clientOrderId = this.resolveCallerClientOrderId(opts) as string;
    const orderIds = await this.placeOrder(opts);
    return { orderIds, clientOrderId };
  };

  /**
   * A client order ID this venue will honor on `opts`, in the venue's own format,
   * or undefined when it would refuse one. Never emits: callers ask before sending.
   */
  mintClientOrderId = (opts: PlaceOrderOpts): string | undefined => {
    const rule = this.clientOrderIdRule;
    if (!rule || !(rule.isHonorableShape ?? isRegularClientOrderShape)(opts)) return undefined;
    if (!this.store.marketsBySymbol[opts.symbol] || this.wouldSplitOrder(opts)) return undefined;
    // A venue's own format carries its attribution; never trade it for a generic ID.
    const candidates = rule.mint ? [rule.mint()] : defaultMintCandidates();
    return candidates.find((candidate) => rule.isValid(candidate));
  };

  /**
   * Finds the order placed with `clientOrderId`, for a placement whose response
   * was lost. This base version sees open orders only, so a miss is never
   * definitive: a filled or cancelled order is invisible to it, and several
   * venues' fetchOrders return the store on failure. Venues with a single-order
   * endpoint override it. Never writes the store.
   */
  findOrderByClientOrderId = (symbol: string, clientOrderId: string): Promise<ClientOrderLookup> =>
    this.findOrderFromOpenOrders(symbol, clientOrderId);

  /** The lookup from open orders alone: a miss is never definitive. */
  protected findOrderFromOpenOrders = async (symbol: string, clientOrderId: string): Promise<ClientOrderLookup> => {
    const notFound: ClientOrderLookup = { found: false, definitive: false };
    // A venue that refuses caller IDs never sent one; some (Hyperliquid) also write the store while fetching.
    if (!this.clientOrderIdRule) return notFound;
    const matches = (order: Order) => order.symbol === symbol && orderCarriesClientOrderId(order, clientOrderId);
    const inStore = this.store.orders.find(matches);
    if (inStore) return { found: true, order: inStore };
    const order = (await this.fetchOrders([symbol])).find(matches);
    return order ? { found: true, order } : notFound;
  };

  /** Venue rule for `PlaceOrderOpts.clientOrderId`; undefined means the venue cannot honor caller IDs. */
  protected readonly clientOrderIdRule: ClientOrderIdRule | undefined = undefined;

  /**
   * Returns the caller's clientOrderId for a shape this venue can honor, or
   * rejects (emit, then throw) before any write. Call before any request.
   */
  protected resolveCallerClientOrderId(opts: PlaceOrderOpts): string | undefined {
    const { clientOrderId } = opts;
    if (clientOrderId === undefined) return undefined;
    const rule = this.clientOrderIdRule;
    if (!rule) {
      return this.rejectPreflight(`${this._exchangeName} cannot honor a caller clientOrderId`);
    }
    if (!(rule.isHonorableShape ?? isRegularClientOrderShape)(opts)) {
      return this.rejectPreflight(clientOrderIdShapeMessage(rule.label, opts));
    }
    if (typeof clientOrderId !== 'string' || !rule.isValid(clientOrderId)) {
      return this.rejectPreflight(`${rule.label} clientOrderId must ${rule.requirement}`);
    }
    this.assertCallerClientOrderIdUnsplit(opts);
    return clientOrderId;
  }

  /** Whole-batch fail-closed: every caller ID is honorable and unique, or nothing is sent. */
  protected assertCallerClientOrderIds(orders: PlaceOrderOpts[]) {
    const seen = new Set<string>();
    for (const order of orders) {
      const clientOrderId = this.resolveCallerClientOrderId(order);
      if (clientOrderId === undefined) continue;
      if (seen.has(clientOrderId)) {
        this.rejectPreflight(`${this.clientOrderIdRule!.label} clientOrderId ${clientOrderId} is duplicated in one batch`);
      }
      seen.add(clientOrderId);
    }
  }

  protected assertSingleNativeClientOrderId(clientOrderId: string | undefined, nativeOrderCount: number) {
    if (clientOrderId !== undefined && nativeOrderCount !== 1) {
      this.rejectPreflight(clientOrderIdSplitMessage(this.clientOrderIdRule?.label ?? this._exchangeName));
    }
  }

  // An order above the market's maxSize splits into several native orders, so it rejects up front;
  // adapters still assert the payload count after formatting as a backstop.
  private wouldSplitOrder(opts: PlaceOrderOpts): boolean {
    const market = this.store.marketsBySymbol[opts.symbol];
    if (!market) return false;
    const maxSize = market.limits.amount.max;
    return maxSize > 0 && adjust(opts.amount, market.precision.amount) > maxSize;
  }

  private assertCallerClientOrderIdUnsplit(opts: PlaceOrderOpts) {
    if (opts.clientOrderId === undefined) return;
    if (this.wouldSplitOrder(opts)) {
      this.rejectPreflight(
        `${this.clientOrderIdRule?.label ?? this._exchangeName} order exceeds the market's max size for one order; a caller clientOrderId cannot span several orders`,
      );
    }
  }

  updateOrder = async (_opts: UpdateOrderOpts) => {
    await Promise.reject(new Error('Not implemented'));
    return [] as string[];
  };

  cancelOrders = async (_orders: Order[]) => {
    await Promise.reject(new Error('Not implemented'));
  };

  cancelSymbolOrders = async (symbol: string) => {
    const orders = this.store.orders.filter((o) => o.symbol === symbol);
    if (orders.length > 0) {
      await this.cancelOrders(orders);
    }
  };

  cancelAllOrders = () => this.cancelEachOrderSymbol();

  protected cancelEachOrderSymbol = async () => {
    const symbols = uniq(this.store.orders.map((order) => order.symbol));

    await forEachSeries(symbols, async (symbol) => {
      if (!this.isDisposed) {
        await this.cancelSymbolOrders(symbol);
      }
    });
  };

  scheduleCancel = async (_time?: number | null) => {
    await Promise.reject(new Error('Not implemented'));
  };

  fetchOHLCV = async (_opts: OHLCVOptions) => {
    return await Promise.resolve([] as Candle[]);
  };

  fetchHistoricalFills = markBaseCapabilityFallback('fetchHistoricalFills', async (_opts: ExecutionsOptions) => {
    return await Promise.resolve([] as Execution[]);
  });

  /**
   * Closed round trips for the journal, folded from this venue's fills.
   *
   * Delegates to `buildFoldedPositionRecords`, the SAME implementation the audit harness has
   * been grading against venue ledgers all week. A second copy written for the UI is exactly
   * where the two would drift, and the drift would be invisible because both would look
   * correct alone.
   *
   * **THROWS when the venue has no usable fills axis, and never returns `[]`.** An empty array
   * is indistinguishable from an account with no trades — the failure this whole surface was
   * built around — so the caller must be able to tell "we could not ask" from "nothing
   * happened". The seam turns this into a named, reported state.
   *
   * Whether a venue may be shown AT ALL is not decided here. That lives once in
   * the host's journal selection, so a venue is never fetched and then discarded, and no adapter can
   * answer the question differently from its neighbours.
   */
  /**
   * Closed round trips for the journal.
   *
   * **REFUSES BY DEFAULT.** A venue must declare its records path explicitly; there is no
   * inherited behaviour.
   *
   * This started as a fold-from-fills default and that was the structural cause of three
   * separate silent-empty bugs in one sitting. okx, woo, paradex and hyperliquid each
   * inherited a fold that is wrong for them and returned `[]` — an empty journal reading as
   * "you have no trades" for accounts with 134, 263, 64 and 103 real round trips. Each fix
   * looked complete from inside the case that motivated it, and left its siblings intact.
   *
   * A default cannot be right for a venue nobody checked. Refusing makes the siblings RAISE
   * THEIR HANDS instead of relying on anyone to enumerate them: a new venue with no declared
   * path fails loudly and is reported as an account we could not read, which is true.
   */
  fetchHistoricalPositions = async (_opts: {
    account: string;
    from?: number;
    to?: number;
  }): Promise<PositionRecord[]> => {
    throw new Error(
      `${this.constructor.name}: no journal records path declared. Override ` +
        `fetchHistoricalPositions — fold from fills via foldClosedPositionRecords, or map ` +
        `this venue's reported closed axis.`,
    );
  };

  /**
   * Fold this venue's FILLS into round trips. Shared with the audit harness, which has been
   * grading this exact fold against venue ledgers.
   *
   * `symbolScoped` where the venue's fills route demands a symbol (binance, paradex,
   * hyperliquid). Account-wide is one request instead of N and is preferred where allowed.
   */
  protected foldClosedPositionRecords = async (
    opts: { account: string; from?: number; to?: number },
    config: { symbolScoped?: boolean } = {},
  ): Promise<PositionRecord[]> => {
    if (!this.capabilities.fillHistory) {
      throw new Error(`${this.constructor.name}: no fills axis, cannot fold journal records`);
    }

    const to = opts.to ?? Date.now();
    const from = opts.from ?? to - 30 * 24 * 60 * 60 * 1000;

    const { records } = await buildFoldedPositionRecords(this as never, {
      account: opts.account,
      from,
      to,
      symbolScoped: config.symbolScoped,
      resolveSettleCurrency: (symbol) => this.store?.marketsBySymbol?.[symbol]?.quote || 'USDT',
    });

    return records;
  };

  // Branded so support can be detected. `typeof exchange.fetchIncomeHistory` is TRUE
  // on every venue because this field always exists — any consumer duck-typing on that
  // silently treats all 11 exchanges as supported and then renders an empty ledger.
  // Read `exchange.capabilities.incomeHistory`.
  fetchIncomeHistory = markBaseCapabilityFallback(
    'fetchIncomeHistory',
    async (_opts?: IncomeHistoryOptions): Promise<IncomeRecord[]> => {
      return await Promise.resolve([] as IncomeRecord[]);
    },
  );

  fetchIncomeHistoryRange = markBaseCapabilityFallback(
    'fetchIncomeHistoryRange',
    async (_startTime: number, _endTime: number, _incomeType?: string): Promise<IncomeRecord[]> => {
      return await Promise.resolve([] as IncomeRecord[]);
    },
  );

  /**
   * Exchange-reported closed positions — the low-multiplicity anchor, as distinct from
   * positions inferred by polling in the browser. Branded and non-optional: it was
   * declared `fetchClosedPnl?` on the interface, so consumers guarded with
   * `if (!exchange?.fetchClosedPnl)`, which cannot distinguish an implementation from
   * this stub once the field always exists.
   */
  fetchClosedPnl = markBaseCapabilityFallback(
    'fetchClosedPnl',
    async (_opts: HistoricalTradeOptions): Promise<ClosedPnlEntry[]> => {
      return await Promise.resolve([] as ClosedPnlEntry[]);
    },
  );

  /**
   * Historical orders, including ones that never filled. Base returns [] and is
   * branded, so an unimplemented venue is distinguishable from one with no orders.
   */
  fetchHistoricalOrders = markBaseCapabilityFallback(
    'fetchHistoricalOrders',
    async (_opts?: OrderHistoryOptions): Promise<HistoricalOrder[]> => {
      return await Promise.resolve([] as HistoricalOrder[]);
    },
  );

  // Options are opt-in per venue. These reject, so a caller that skipped the capability
  // check gets an error naming the venue, never an empty chain that reads as "no options".
  fetchOptionChain = markBaseCapabilityFallback(
    'fetchOptionChain',
    async (_query: OptionChainQuery): Promise<OptionChainRow[]> => {
      throw new Error(`Option chains are not supported on ${this._exchangeName}`);
    },
  );

  fetchOptionExpiries = markBaseCapabilityFallback(
    'fetchOptionExpiries',
    async (_underlying: string): Promise<number[]> => {
      throw new Error(`Option expiries are not supported on ${this._exchangeName}`);
    },
  );

  fetchAccountGreeks = markBaseCapabilityFallback('fetchAccountGreeks', async (): Promise<AccountGreeks[]> => {
    throw new Error(`Account greeks are not supported on ${this._exchangeName}`);
  });

  fetchPublicTrades = async (_symbol: string, _limit?: number): Promise<PublicTrade[]> => {
    await Promise.reject(new Error('Not implemented'));
    return [];
  };

  /**
   * Listen to OHLCV candle updates.
   * If using external public data, routes through the gateway provider.
   * Exchanges should override this method and check usesExternalPublicData
   * to use direct WebSocket if not using gateway.
   */
  listenOHLCV = (opts: OHLCVOptions, callback: (candle: Candle) => void) => {
    // If using external public data provider, route through it
    if (this.externalPublicDataProvider) {
      return this.externalPublicDataProvider.subscribeCandle(opts, callback);
    }
    // Default implementation - exchanges should override
    return () => {};
  };

  listenOHLCVs = (
    _opts: OHLCVSOptions,
    _callback: (candle: Candle & { symbol: string; interval: Timeframe }) => void,
  ) => {
    return;
  };

  /**
   * Listen to orderbook updates.
   * If using external public data, routes through the gateway provider.
   * Exchanges should override this method and check usesExternalPublicData
   * to use direct WebSocket if not using gateway.
   */
  listenOrderBook = (symbol: string, callback: (orderBook: OrderBook) => void) => {
    // If using external public data provider, route through it
    if (this.externalPublicDataProvider) {
      return this.externalPublicDataProvider.subscribeOrderbook(symbol, callback);
    }
    // Default implementation - exchanges should override
    return () => {};
  };

  deriveAlgoOrdersFromNormalOrdersOpts = (opts: PlaceOrderOpts[]) => {
    // Indexed for-loop with direct push instead of `reduce + [...acc, ...n]`.
    // The prior shape was O(N²) — each iteration spread the growing acc;
    // for a 50-order batch with SL+TP that's ~2.5K element-reference
    // copies just to derive the algo orders. User-action path so not
    // hot-frequency, but the pattern is exactly the one the docs flag.
    const result: PlaceOrderOpts[] = [];
    for (let i = 0; i < opts.length; i++) {
      const o = opts[i];
      const inverseSide = o.side === OrderSide.Buy ? OrderSide.Sell : OrderSide.Buy;
      if (o.stopLoss) {
        result.push({
          symbol: o.symbol,
          side: inverseSide,
          type: OrderType.StopLoss,
          price: o.stopLoss,
          amount: o.amount,
          slTriggerType: o.slTriggerType,
          maxSlippagePct: o.maxSlippagePct,
        });
      }
      if (o.takeProfit) {
        result.push({
          symbol: o.symbol,
          side: inverseSide,
          type: OrderType.TakeProfit,
          price: o.takeProfit,
          amount: o.amount,
          tpTriggerType: o.tpTriggerType,
          maxSlippagePct: o.maxSlippagePct,
        });
      }
    }
    return result;
  };

  nuke = async (tries = 0) => {
    if (!this.isDisposed && !this.isNuking) {
      this.isNuking = true;

      try {
        // close all positions
        const openPositions = this.store.positions.filter((position) => position.contracts > 0);

        await forEachSeries(openPositions, async (position) => {
          try {
            await this.placeOrder({
              symbol: position.symbol,
              side: position.side === 'long' ? OrderSide.Sell : OrderSide.Buy,
              type: OrderType.Market,
              amount: position.contracts,
              reduceOnly: true,
            });
          } catch (err: any) {
            // don't let one failing symbol abort closing the remaining positions
            this.log(`Nuke: failed to close position on ${position.symbol}: ${err?.message || err}`, LogSeverity.Error);
          }
        });

        // cancel all orders
        await this.cancelAllOrders();
      } finally {
        this.isNuking = false;
      }
    }

    const openPositions = this.store.positions.filter((position) => position.contracts > 0);

    if (tries + 1 <= 3 && openPositions.length > 0) {
      await sleep(100);
      await this.nuke(tries + 1);
    }
  };

  nukePositions = async (tries = 0) => {
    if (!this.isDisposed && !this.isNuking) {
      this.isNuking = true;

      try {
        // close all positions
        const openPositions = this.store.positions.filter((position) => position.contracts > 0);

        await forEachSeries(openPositions, async (position) => {
          try {
            await this.placeOrder({
              symbol: position.symbol,
              side: position.side === 'long' ? OrderSide.Sell : OrderSide.Buy,
              type: OrderType.Market,
              amount: position.contracts,
              reduceOnly: true,
            });
          } catch (err: any) {
            // don't let one failing symbol abort closing the remaining positions
            this.log(`Nuke: failed to close position on ${position.symbol}: ${err?.message || err}`, LogSeverity.Error);
          }
        });
      } finally {
        this.isNuking = false;
      }
    }

    const openPositions = this.store.positions.filter((position) => position.contracts > 0);

    if (tries + 1 <= 3 && openPositions.length > 0) {
      await sleep(100);
      await this.nukePositions(tries + 1);
    }
  };

  nukeOrders = async () => {
    if (!this.isDisposed && !this.isNuking) {
      this.isNuking = true;
      try {
        // cancel all orders
        await this.cancelAllOrders();
      } finally {
        this.isNuking = false;
      }
    }
  };

  /**
   * Waits for the remote signer to become available.
   * Returns true if signer is available, false if exchange was disposed while waiting.
   * When no remoteSigner is configured, returns true immediately.
   */
  protected async waitForRemoteSigner(): Promise<boolean> {
    const remoteSigner = this.options.remoteSigner;
    if (!remoteSigner || remoteSigner.isAvailable()) return true;

    this.emitter.emit('remoteSignerStatus', 'disconnected');

    return new Promise<boolean>((resolve) => {
      if (this.isDisposed) {
        resolve(false);
        return;
      }
      if (remoteSigner.isAvailable()) {
        this.emitter.emit('remoteSignerStatus', 'connected');
        resolve(true);
        return;
      }
      // `settled` + `finish` ensure the promise resolves exactly once, whether
      // the signer becomes available or the exchange is disposed while waiting.
      let settled = false;
      const finish = (available: boolean) => {
        if (settled) return;
        settled = true;
        unsubscribe();
        this.remoteSignerAvailabilityCleanups.delete(disposalCleanup);
        if (available) this.emitter.emit('remoteSignerStatus', 'connected');
        resolve(available);
      };
      const unsubscribe = remoteSigner.onAvailabilityChange((available) => {
        if (available) finish(true);
      });
      // Registered so dispose() settles this waiter with `false` (and unsubscribes)
      // instead of leaving it pending forever after the listener is torn down.
      const disposalCleanup = () => finish(false);
      this.remoteSignerAvailabilityCleanups.add(disposalCleanup);
    });
  }

  dispose() {
    this.isDisposed = true;
    this.unsubscribeFromStore();
    this.remoteSignerAvailabilityCleanups.forEach((cleanup) => cleanup());
    this.remoteSignerAvailabilityCleanups.clear();

    // Stop the watchdog timer
    this.stopWatchdog();

    // Dispose external public data provider if present
    if (this.externalPublicDataProvider) {
      this.externalPublicDataProvider.dispose();
      this.externalPublicDataProvider = undefined;
    }

    // Properly dispose all WebSocket connections
    const sockets = this.getAllSockets();
    sockets.forEach((socket) => {
      if (socket) {
        socket.dispose(); // Use dispose() which properly cleans up timeouts and calls ws.close()
      }
    });

    this.off('update');
    this.off('fill');
    this.off('error');

    this.off('listenOHLCV');
    this.off('subscribeOHLCV');
    this.off('unsubscribeOHLCV');

    this.off('listenOrderBook');
    this.off('subscribeOrderBook');
    this.off('unsubscribeOrderBook');

    this.off('updatePositions');
    this.off('updatePositions:done');
    this.off('callMethod');
    this.off('callMethod:done');

    this.off('log');
    this.off('warning');
    this.off('ws-reconnected');
    this.off('remoteSignerStatus');
    this.off('markets:changed');
    this.off('activeTickerSymbols:changed');

    this.off('grafana-error');
    this.off('grafana-http-request');
    this.off('grafana-http-response');
    this.off('grafana-info');
    this.off('grafana-pws');
    this.off('grafana-pws-sub');

    this.store.reset();
  }

  listenOHLCVWebworker(o: Pick<OHLCVOptions, 'symbol' | 'interval'>, c: (c: Candle) => void) {
    const subId = nanoid();
    this.emitter.emit('subscribeOHLCV', { symbol: o.symbol, interval: o.interval, subId });
    const handler = (event: any) => {
      if (event.symbol === o.symbol && event.interval === o.interval) {
        c(event.candle);
      }
    };
    this.emitter.on('listenOHLCV', handler);
    return () => {
      this.emitter.off('listenOHLCV', handler);
      this.emitter.emit('unsubscribeOHLCV', { symbol: o.symbol, interval: o.interval, subId });
    };
  }

  private workerFrameApplier?: FrameApplier;

  /** Engine-backed venues take the record a snapshot frame carries; others have none. */
  protected adoptEngineCapabilities(_capabilities: EngineCapabilities): void {}
  private workerFrameRecoveryPending = false;

  fetchEngineSnapshot = async (): Promise<EngineSnapshot> => {
    throw new Error('Engine snapshots are unavailable for this exchange');
  };

  protected dispatchFrameEvent(callback: () => void) {
    try {
      callback();
    } catch (error) {
      console.error('Engine frame listener failed', error);
    }
  }

  /**
   * Main thread: apply one Rust engine frame from the worker (`op: 'frame'`)
   * to this store and emit its books, candles and fills as the worker's
   * object messages are emitted, so subscribers cannot tell the difference.
   */
  applyWorkerFrame(buffer: ArrayBuffer) {
    this.applyWorkerFrameBuffer(buffer, true);
  }

  private applyWorkerFrameBuffer(buffer: ArrayBuffer, recover: boolean) {
    this.workerFrameApplier ??= new FrameApplier(this.store);
    let frame: Frame | undefined;
    let needsSnapshot = false;
    try {
      frame = new Frame(buffer);
    } catch (error) {
      this.workerFrameApplier.invalidateMarketIdentity();
      needsSnapshot = true;
      console.error('Invalid engine frame', error);
    }
    if (frame) {
      try {
        const capabilities = frame.capabilities();
        if (capabilities) this.adoptEngineCapabilities(capabilities);
      } catch (error) {
        console.error('Engine capabilities section failed', error);
      }
      try {
        this.workerFrameApplier.apply(frame);
      } catch (error) {
        needsSnapshot = error instanceof FrameApplyError;
        console.error('Engine frame apply failed', error);
      }
      const deliver = <T>(read: () => T[], event: string) => {
        let rows: T[];
        try {
          rows = read();
        } catch (error) {
          needsSnapshot = true;
          console.error('Engine frame section failed', error);
          return;
        }
        for (const row of rows) this.dispatchFrameEvent(() => this.emitter.emit(event, row));
      };
      deliver(() => this.workerFrameApplier!.books(frame!), 'listenOrderBook');
      deliver(() => this.workerFrameApplier!.candles(frame!), 'listenOHLCV');
      deliver(() => this.workerFrameApplier!.fills(frame!), 'fill');
    }
    if (!needsSnapshot || !recover || this.workerFrameRecoveryPending || this.isDisposed) return;
    this.workerFrameRecoveryPending = true;
    void Promise.resolve()
      .then(async () => {
        const snapshot = await (this as Exchange).callMethodInWorker('fetchEngineSnapshot');
        if (!this.isDisposed) {
          this.applyWorkerFrameBuffer(snapshot.frame, false);
          this.dispatchFrameEvent(() => this.workerFrameApplier!.applyLeverageSnapshot(snapshot.fetchedLeverage));
        }
      })
      .catch((error) => console.error('Engine frame recovery failed', error))
      .finally(() => {
        this.workerFrameRecoveryPending = false;
      });
  }

  listenOrderBookWebworker(s: string, c: (o: OrderBook) => void) {
    const subId = nanoid();
    this.emitter.emit('subscribeOrderBook', { symbol: s, subId });
    const handler = (event: any) => {
      if (event.symbol === s) {
        c(event.orderbook);
      }
    };
    this.emitter.on('listenOrderBook', handler);
    return () => {
      this.emitter.off('listenOrderBook', handler);
      this.emitter.emit('unsubscribeOrderBook', { symbol: s, subId });
    };
  }

  fetchAndSetOrders = async (symbols?: string[]): Promise<Order[]> => {
    const orders = await this.fetchOrders(symbols);

    // if we pass symbols assume its not a full update...
    if (symbols) {
      for (const symbol of symbols) {
        const symbolOrders = orders.filter((o) => o.symbol === symbol);
        this.store.setOrdersForSymbol(symbol, symbolOrders);
      }
    } else {
      this.store.update({
        orders,
        loaded: { ...this.store.loaded, orders: true },
      });
    }

    return orders;
  };

  updatePositionsInWebworker = async (symbols?: string[]) => {
    const requestId = nanoid();
    this.emitter.emit('updatePositions', { symbols, requestId });

    return new Promise<void>((resolve, reject) => {
      const timeout = setTimeout(() => {
        // Timeout path must also remove the listener, otherwise every timed-out
        // call leaks a handleDone closure permanently attached to the emitter.
        this.emitter.off('updatePositions:done', handleDone);
        reject(new Error('Timeout updatePositionsInWebworker'));
      }, 5000);
      const handleDone = (event: any) => {
        if (event.requestId === requestId) {
          clearTimeout(timeout);
          this.emitter.off('updatePositions:done', handleDone);
          if (event.success) {
            resolve();
          } else {
            // Only log actual errors, not expected states like retry_too_soon
            if (event.reason && event.reason !== 'retry_too_soon') {
              console.error('reject in updatePositionsInWebworker', event.reason);
            }
            reject(new Error(event.reason || 'Unknown error in updatePositionsInWebworker'));
          }
        }
      };
      this.emitter.on('updatePositions:done', handleDone);
    });
  };

  protected workerCommandTimeoutMs = (_method: CallableMethodsInWorker) => 5000;

  protected workerCommandTimeoutError = (_method: CallableMethodsInWorker, _args: unknown[]): Error =>
    new Error('Timeout callMethodInWorker');

  callMethodInWorker = async <M extends CallableMethodsInWorker>(
    method: M,
    ...args: Parameters<this[M]>
  ): Promise<ReturnType<this[M]>> => {
    const requestId = nanoid();

    return new Promise<ReturnType<this[M]>>((resolve, reject) => {
      const timeout = setTimeout(() => {
        // Timeout path must also remove the listener, otherwise every timed-out
        // call leaks a handleDone closure permanently attached to the emitter.
        this.emitter.off('callMethod:done', handleDone);
        reject(this.workerCommandTimeoutError(method, args));
      }, this.workerCommandTimeoutMs(method));
      const handleDone = (event: any) => {
        if (event.requestId === requestId) {
          clearTimeout(timeout);
          this.emitter.off('callMethod:done', handleDone);
          if (event.success) {
            const result = withHistoryCompleteness(event.result, event.completeness);
            resolve(result);
            this.afterMethodCalledInWorker(result, method, ...args);
          } else {
            reject(deserializeCommandError(event.result));
          }
        }
      };
      this.emitter.on('callMethod:done', handleDone);
      try {
        this.emitter.dispatch('callMethod', { method, args, requestId });
      } catch (error) {
        clearTimeout(timeout);
        this.emitter.off('callMethod:done', handleDone);
        reject(error);
      }
    });
  };

  afterMethodCalledInWorker = <M extends CallableMethodsInWorker>(
    _result: ReturnType<this[M]>,
    _method: M,
    ..._args: Parameters<this[M]>
  ) => {};

  removeOrders = (orders: Pick<Order, 'id'>[]) => {
    this.store.removeOrders(orders);
  };

  // called on ws reconnect before resubscribing, refetch any private data snapshots needed
  // that aren't handled in tick in case there has private state changes while disconnected...
  // Usually only need to fetch orders here as rest handled in tick, but consider broader refetch
  fetchAndSetPrivateSnapshot = async (tries = 1) => {
    if (this.isPublicDataOnly) return;
    // Disposal guard: this runs on every private-WS (re)connect via the
    // re-open resync (BaseWebSocket.trackConnected) with tries=10. Without these checks a disposed
    // account (browser gone / remote signer dead) would keep retrying
    // fetchOrders() — each retry signs a REST request that throws
    // "Remote signer unavailable", flooding logs/CPU forever. See
    // base.ws.ts send()/dispose() guards for the matching reconnect side.
    if (this.isDisposed) return;

    let count = 0;
    while (count < tries) {
      if (this.isDisposed) return;
      try {
        const orders = await this.fetchOrders();
        if (this.isDisposed) return;
        this.store.update({
          orders,
          loaded: { ...this.store.loaded, orders: true },
        });
        return;
      } catch (e: any) {
        if (this.isDisposed) return;
        this.emitAndLogError(e);
        count++;
        if (count < tries) await sleep(1000);
      }
    }
  };

  private _fetchPositionForSymbolPromise: Record<string, Promise<Position[]> | undefined> = {};

  /**
   * Fetches positions for a symbol, if not found in store. Update store after fetch.
   * @param symbol
   */
  getOrFetchPositionsForSymbol = async (symbol: string) => {
    // If the exchange doesn't support positions, return empty array
    const features = exchangeMetadata[this._exchangeName]?.features;
    if (features?.positions === false) {
      return [];
    }
    let foundPositions = this.store.positions.filter((p) => p.symbol === symbol);
    if (foundPositions.length === 0) {
      const existingPromise = this._fetchPositionForSymbolPromise[symbol];
      if (existingPromise) {
        return await existingPromise;
      }
      // check if store loaded, else easy out
      if (!this.store.loaded.positions) {
        return [];
      }
      console.warn(`No positions found for ${symbol} on ${this.name}, fetching...`);
      const newPromise = this.fetchPositions([symbol]);
      this._fetchPositionForSymbolPromise[symbol] = newPromise;
      foundPositions = await newPromise;

      if (this.isDisposed) return [];

      delete this._fetchPositionForSymbolPromise[symbol];
      this.store.addOrUpdatePositions(foundPositions);
    }
    return foundPositions;
  };

  get name() {
    return this._exchangeName;
  }

  get isOnline() {
    return this._isOnline;
  }

  /**
   * Sets the online/offline status for this exchange.
   * When transitioning from offline to online, triggers WebSocket reconnection.
   * When transitioning to offline, WebSocket reconnect loops will pause until online again.
   */
  setOnlineStatus = (isOnline: boolean) => {
    const wasOffline = !this._isOnline;
    this._isOnline = isOnline;

    if (wasOffline && isOnline) {
      this.log('Network came online - triggering WebSocket reconnection', LogSeverity.Info);
      this.recoverAfterOffline();
    } else if (!isOnline) {
      this.log('Network went offline - WebSocket reconnects will be paused', LogSeverity.Info);
    }
  };

  reconnect = () => {
    this.getAllSockets().forEach((socket) => socket?.reconnect());
  };

  /**
   * See `BaseWebSocket.recoverAfterOffline`: reconnect now, past any offline backoff.
   * False when a socket cannot (it runs its own close loop), so the caller reloads.
   */
  recoverAfterOffline = (): boolean =>
    this.getAllSockets()
      .map((socket) => socket?.recoverAfterOffline() ?? true)
      .every(Boolean);

  /**
   * Note the most recent non-2xx that axios resolved instead of rejecting.
   *
   * Called from the REST response interceptors for every client. A successful
   * response clears the slot: a stale failure is worse than none, because it
   * reads as current. Never throws — telemetry must not be able to break a
   * request.
   */
  public noteHttpResponse(response: unknown, url?: string) {
    try {
      this._lastHttpFailure = recordHttpFailure(response, Date.now(), url);
    } catch {
      /* recording must never break a request */
    }
  }

  emitAndLogError(err: string | Error | undefined | null, msg?: string, extraData?: Record<string, any>) {
    if (this.isDisposed) {
      console.warn('exchange error (disposed, suppressing)', err);
      return;
    }

    const structuredExtraData =
      extraData && typeof extraData === 'object' && !Array.isArray(extraData) ? extraData : {};
    const suppressConsoleLog = structuredExtraData.suppressConsoleLog === true;

    if (!suppressConsoleLog) {
      console.error('exchange error', err);
      console.trace();
    }

    let formattedMsg;
    let errorObject: Error | undefined;
    let sentryContext: any = {};
    const sentryTags =
      structuredExtraData.sentryTags && typeof structuredExtraData.sentryTags === 'object'
        ? structuredExtraData.sentryTags
        : {};
    const sentryExtra =
      structuredExtraData.sentryExtra && typeof structuredExtraData.sentryExtra === 'object'
        ? structuredExtraData.sentryExtra
        : {};
    const legacyExtraData = Object.fromEntries(
      Object.entries(structuredExtraData).filter(
        ([key]) =>
          key !== 'sentryTags' &&
          key !== 'sentryExtra' &&
          key !== 'suppressConsoleLog' &&
          key !== 'engineEvent' &&
          key !== 'classified',
      ),
    );

    // Handle undefined, null, or other falsy values
    if (!err) {
      formattedMsg = msg || 'Unknown Exchange Error';
      errorObject = new Error(formattedMsg);
      errorObject.name = 'ExchangeError';
    } else if (typeof err === 'string') {
      formattedMsg = (msg || 'Exchange Error') + ': ' + err;
      // Create an Error object for Sentry
      errorObject = new Error(formattedMsg);
      errorObject.name = 'ExchangeError';
    } else {
      errorObject = err;

      // Check if this is an enhanced axios error
      const enhancedError = err as EnhancedAxiosError;
      if (enhancedError?.context && enhancedError?.enhancedMessage) {
        // Use the enhanced message for better context
        formattedMsg = msg || enhancedError?.enhancedMessage;

        // Format error context for Sentry
        const errorForSentry = formatErrorForSentry(enhancedError);
        sentryContext = {
          ...errorForSentry.tags,
          ...errorForSentry.extra,
          ...legacyExtraData,
        };
      } else {
        if (msg) {
          formattedMsg = msg;
        } else {
          formattedMsg = err.message || String(err);
        }

        // Check if this is a TradeTunnel error with URL context
        const tunnelError = err as any;
        if (tunnelError?.url || tunnelError?.method) {
          sentryContext = {
            ...sentryContext,
            url: tunnelError.url,
            method: tunnelError.method,
            originalError: tunnelError.originalError,
            errorType: tunnelError.name || 'TradeTunnelError',
            ...legacyExtraData,
          };
        }
      }
    }
    sentryContext = {
      ...sentryContext,
      ...legacyExtraData,
    };

    // A swallowed non-2xx, tagged as the CORRELATION it is.
    //
    // Skipped when the error is an enhanced axios error: those rejected
    // normally, so `formatErrorForSentry` already put a causal `statusCode` on
    // them and a nearby-but-unrelated status beside it would actively mislead.
    // Tags only — `sanitizeWorkerSentryData` caps worker `extra` at 12 keys in
    // insertion order, so anything appended there is silently dropped.
    // Consumed once, so one failure cannot decorate every error behind it.
    const enhancedForCorrelation = err as EnhancedAxiosError | null;
    let correlationTags: Record<string, unknown> = {};
    if (!enhancedForCorrelation?.context) {
      correlationTags = readHttpFailureContext(this._lastHttpFailure, Date.now()) ?? {};
      // Cleared either way: consumed when it correlated, dropped when it had
      // already expired, so a stale record cannot sit in the slot forever.
      this._lastHttpFailure = undefined;
    }

    // A diagnostic's own classification (engine events) wins over the error value's.
    const classified = classifiedError(structuredExtraData.classified) ?? classifiedError(err);
    // Typed engine errors (kind/outcome/venueCode) as closed-set tags; explicit tags still win.
    const typedTags = {
      ...exchangeErrorTags(classified),
      ...(structuredExtraData.engineEvent ? { event_code: structuredExtraData.engineEvent.code } : {}),
    };

    // Detect if we're in a web worker
    const isWebWorker = typeof self !== 'undefined' && typeof window === 'undefined';

    if (isWebWorker) {
      // In web worker: emit error with Sentry flag and enhanced context
      const enhancedError = typeof err === 'object' && err !== null ? (err as EnhancedAxiosError) : null;
      this.emitter.emit('error', formattedMsg, {
        sendToSentry: true,
        sentryData: {
          error: errorObject,
          tags: {
            exchange: this.name,
            module: 'Exchange',
            severity: 'error',
            ...sentryContext,
            ...typedTags,
            ...sentryTags,
            ...correlationTags,
          },
          extra: {
            originalMessage: formattedMsg,
            exchangeOptions: {
              testnet: this.options.testnet,
              publicDataOnly: this.options.publicDataOnly,
            },
            ...sentryContext,
            ...sentryExtra,
            ...(structuredExtraData.engineEvent ? { engineEvent: structuredExtraData.engineEvent } : {}),
          },
        },
        // Pass enhanced error context for better toast messages
        enhancedContext: enhancedError?.context,
        engineEvent: structuredExtraData.engineEvent,
        classified,
        suppressConsoleLog,
      });
    } else {
      // In main thread: report directly for a better stack trace (to the host's sink).
      if (errorObject) {
        reportException(errorObject, {
          tags: {
            exchange: this.name,
            module: 'Exchange',
            severity: 'error',
            ...sentryContext,
            ...typedTags,
            ...sentryTags,
            ...correlationTags,
          },
          extra: {
            originalMessage: formattedMsg,
            exchangeOptions: {
              testnet: this.options.testnet,
              publicDataOnly: this.options.publicDataOnly,
            },
            ...sentryContext,
            ...sentryExtra,
            ...(structuredExtraData.engineEvent ? { engineEvent: structuredExtraData.engineEvent } : {}),
          },
        });
      }

      // Emit error with enhanced context for better toast messages
      const enhancedError = typeof err === 'object' && err !== null ? (err as EnhancedAxiosError) : null;
      this.emitter.emit('error', formattedMsg, {
        enhancedContext: enhancedError?.context,
        // Typed engine errors: kind/outcome/venueCode, so listeners need not match text.
        engineEvent: structuredExtraData.engineEvent,
        classified,
        suppressConsoleLog,
      });
    }
  }

  /** Pre-flight reject: emit first (toast pipeline), then throw. See src/exchanges/CLAUDE.md. */
  protected rejectPreflight(message: string): never {
    this.emitAndLogError(message);
    throw new Error(message);
  }

  // if multiple child WS they will all send reconnect messages, try and only emit once...
  emitWsReconnected() {
    const now = Date.now();
    if (now - this.lastReconnectEmit > 5000) {
      this.lastReconnectEmit = now;
      this.emitter.emit('ws-reconnected', '');
    }
  }

  public getAllSockets(): BaseWebSocket<any>[] {
    return [
      this.privateWebsocket,
      this.publicWebsocket,
      this.publicWebsocketSecondary,
      this.publicInverseWebsocket,
      this.futuresWebsocket,
      this.businessWebsocket,
      this.secondsTickersWebsocket,
      this.thirdTickersWebsocket,
    ].filter((socket): socket is BaseWebSocket<any> => socket !== undefined);
  }

  /** Returns all public WebSockets (market data, tickers, etc.) */
  public getPublicSockets(): BaseWebSocket<any>[] {
    return [
      this.publicWebsocket,
      this.publicWebsocketSecondary,
      this.publicInverseWebsocket,
      this.futuresWebsocket,
      this.businessWebsocket,
      this.secondsTickersWebsocket,
      this.thirdTickersWebsocket,
    ].filter((socket): socket is BaseWebSocket<any> => socket !== undefined);
  }

  /** Returns all private WebSockets (orders, positions, balance) */
  public getPrivateSockets(): BaseWebSocket<any>[] {
    return [this.privateWebsocket].filter((socket): socket is BaseWebSocket<any> => socket !== undefined);
  }

  /** Aggregates socket health from all websockets and syncs to store */
  public syncSocketHealthToStore = () => {
    // Start watchdog on first sync (when websockets start receiving messages).
    // startWatchdog is idempotent; the aggregation itself lives in the internal
    // variant, which the watchdog audit also calls.
    this.startWatchdog();
    this.syncSocketHealthToStoreInternal();
  };

  /** Starts the watchdog timer to detect stuck websockets */
  private startWatchdog = () => {
    if (this.watchdogTimer || this.isDisposed) return;

    const scheduleNext = () => {
      if (this.isDisposed) return;
      this.watchdogTimer = setTimeout(() => {
        this.auditSocketHealth();
        scheduleNext();
      }, this.WATCHDOG_INTERVAL_MS);
    };

    scheduleNext();
  };

  /** Stops the watchdog timer */
  private stopWatchdog = () => {
    if (this.watchdogTimer) {
      clearTimeout(this.watchdogTimer);
      this.watchdogTimer = undefined;
    }
  };

  /** Audits all websockets and forces reconnect on stuck ones */
  private auditSocketHealth = () => {
    if (this.isDisposed) {
      this.stopWatchdog();
      return;
    }

    const now = Date.now();
    const allSockets = this.getAllSockets();

    for (const socket of allSockets) {
      const isConnected = socket.ws?.readyState === WebSocket.OPEN;
      const timeSinceLastMessage = socket.lastReceived > 0 ? now - socket.lastReceived : Infinity;
      const socketStaleThresholdMs = socket.getWatchdogStaleThresholdMs() ?? this.WATCHDOG_STALE_THRESHOLD_MS;

      // Case 1: Connected and initialized, but no recent messages (stale)
      if (
        isConnected &&
        socket.isInitialized &&
        socket.lastReceived > 0 &&
        timeSinceLastMessage > socketStaleThresholdMs
      ) {
        this.log(
          `[Watchdog] Socket stale: ${Math.round(timeSinceLastMessage / 1000)}s since last message, forcing reconnect`,
          LogSeverity.Warning,
        );
        socket.reconnect();
        continue;
      }

      // Case 2: Connected but never received messages (stuck on connect)
      // Use this socket's latest open time, not exchange construction time. A
      // newly reconnected socket gets the same first-message grace as its first
      // connection.
      if (isConnected && !socket.isInitialized) {
        const timeSinceConnected = socket.lastConnectedAt > 0 ? now - socket.lastConnectedAt : 0;
        if (timeSinceConnected > this.WATCHDOG_CONNECTION_TIMEOUT_MS) {
          this.log(
            `[Watchdog] Socket never received messages after ${Math.round(timeSinceConnected / 1000)}s, forcing reconnect`,
            LogSeverity.Warning,
          );
          socket.reconnect();
          continue;
        }
      }

      // Case 3: A previously initialized socket has just reconnected and is
      // waiting for its first new frame. This is normal until the per-socket
      // connection grace expires.
      if (
        isConnected &&
        socket.isInitialized &&
        socket.lastReceived === 0 &&
        socket.lastConnectedAt > 0 &&
        now - socket.lastConnectedAt > this.WATCHDOG_CONNECTION_TIMEOUT_MS
      ) {
        this.log(
          `[Watchdog] Reconnected socket received no messages within ${Math.round(
            this.WATCHDOG_CONNECTION_TIMEOUT_MS / 1000,
          )}s, forcing reconnect`,
          LogSeverity.Warning,
        );
        socket.reconnect();
        continue;
      }
    }

    // Sync health to store after audit (without triggering watchdog start again)
    this.syncSocketHealthToStoreInternal();
  };

  /** Internal method to sync health without starting watchdog (used by audit) */
  /**
   * Collapses per-socket reconnect diagnostics into scalars for the store.
   *
   * An exchange can hold several sockets of the same kind, so: the close code
   * comes from the most-recently-closed one (`lastCloseAt`), counts are summed,
   * and the pause flag is true if ANY socket has given up — that is the state
   * worth surfacing, since it means nothing is retrying.
   *
   * Not covered: neither public-data provider is a `BaseWebSocket`, and both are
   * absent from `getPublicSockets()`, so provider-fed accounts report no close
   * code even though the provider drives `publicHealthy`.
   *
   * Both provider kinds count. An account fed in-process by a shared exchange
   * (the hosted exchange-server arrangement) has no public socket of its own, so
   * excluding it left `publicInitialized` false forever and the connection
   * popover stuck on "Waiting" while data flowed. It also pinned `publicHealthy`
   * true via the no-source branch below, so stale public data could not go red.
   */
  private syncSocketHealthToStoreInternal = () => {
    this.store.updateSocketHealth(
      summarizeSocketHealth(this.getPublicSockets(), this.getPrivateSockets(), this.externalPublicDataProvider),
    );
  };

  public fetchLastLogsFromWebsockets = (): string[] => {
    const logs = this.getAllSockets().map((socket) => socket?.lastLog.toArray());
    return logs.flat();
  };

  /**
   * Sign a remote HTTP request using this exchange's axios interceptors.
   * Capture adapter pattern: swaps the adapter, runs interceptors, captures signed config.
   */
  public signHttpRequest = async (request: RemoteSignerRequest): Promise<RemoteSignerResponse> => {
    const xhr = (this as any).xhr;
    if (!xhr) {
      return { id: request.id, error: 'Exchange does not have xhr for signing' };
    }

    return signHttpCapture(xhr, request);
  };

  /**
   * Sign a remote WS auth request.
   *
   * Every branch must match its venue's local (non-remoteSigner) signing
   * expression byte for byte, or the private feed authenticates as garbage and
   * dies silently — no error, just no fills. The `never` default makes adding an
   * outputFormat without a signer a compile error rather than a silent
   * degradation to plain hex, which is exactly how Kraken broke.
   *
   * Exchanges whose WS auth is not HMAC-SHA256 over the secret (Kraken's
   * challenge, dexes, wallet-based) override this method instead.
   */
  public signWsAuth = async (request: RemoteSignerRequest): Promise<RemoteSignerResponse> => {
    const { toSign, outputFormat } = request.wsAuth ?? {};
    if (!toSign || !outputFormat) {
      return { id: request.id, error: 'Missing wsAuth payload for WS auth signing' };
    }
    const secret = this.options.secret;
    if (!secret) {
      return { id: request.id, error: 'No secret available for WS auth signing' };
    }

    switch (outputFormat) {
      case 'hex':
        return { id: request.id, wsAuthSignature: createHmac('sha256', secret).update(toSign).digest('hex') };
      case 'base64':
        return { id: request.id, wsAuthSignature: createHmac('sha256', secret).update(toSign).digest('base64') };
      case 'hex-base64': {
        // blofin: base64 of the hex digest string, not of the raw digest.
        const hex = createHmac('sha256', secret).update(toSign).digest('hex');
        return { id: request.id, wsAuthSignature: Buffer.from(hex).toString('base64') };
      }
      case 'apexomni-base64': {
        // apexomni: the HMAC key is the base64 of the secret, not the secret.
        const key = Buffer.from(secret).toString('base64');
        return { id: request.id, wsAuthSignature: createHmac('sha256', key).update(toSign).digest('base64') };
      }
      case 'bitunix-double-sha256': {
        // bitunix is NOT HMAC: sha256(sha256(nonce+timestamp+apiKey) + secret),
        // with the secret concatenated as a plain string. `toSign` carries the
        // inner digest input.
        const inner = bytesToHex(sha256(utf8ToBytes(toSign)));
        return { id: request.id, wsAuthSignature: bytesToHex(sha256(utf8ToBytes(inner + secret))) };
      }
      case 'kraken-challenge':
        return { id: request.id, error: 'kraken-challenge requires the KrakenExchange signWsAuth override' };
      case 'hyperliquid-l1':
      case 'hyperliquid-address':
      case 'lighter-tx':
      case 'paradex-stark':
      case 'paradex-public':
        return { id: request.id, error: `${outputFormat} requires the Rust engine signer` };
      case 'coinbase-jwt':
        return { id: request.id, error: 'coinbase-jwt requires the CoinbaseExchange signWsAuth override' };
      default: {
        const unhandled: never = outputFormat;
        return { id: request.id, error: `Unsupported WS auth outputFormat: ${String(unhandled)}` };
      }
    }
  };

  // Whether order is TP/SL or trailing stop
  protected isTpSlOrTrailingOrder = (order: Order) => {
    return (
      order.type === OrderType.TakeProfit ||
      order.type === OrderType.StopLoss ||
      order.type === OrderType.TrailingStopLoss
    );
  };

  // Whether order is TP/SL excluding trailing
  protected isTpSlOrder = (order: Order) => {
    return order.type === OrderType.TakeProfit || order.type === OrderType.StopLoss;
  };
}
