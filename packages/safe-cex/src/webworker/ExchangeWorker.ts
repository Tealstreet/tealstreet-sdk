import type { Exchange } from '../exchanges/base';
import type { PackedTickerDelta, Store } from '../store/store.interface';
import type { ExchangeName, ExchangeOptions, OrderFillEvent, StoreData, Ticker } from '../types';
import type { StoreWorkerPayload, StoreWorkerStorePayload } from './StoreWorkerPayload';

import throttle from 'lodash/throttle';

import { exchangeRegistry } from '../exchanges/registry';
import { isRustVenue, RustEngineExchange } from '../exchanges/rust/rust.exchange';
import { DefaultStore, PACKED_TICKER_FIELDS } from '../store/store.base';
import { virtualClock } from '../utils/virtualClock';
import { AppSettingsWorkerType } from './AppSettingsWorkerType';
import { chaserOrderManagerInstance } from './chaserOrder/ChaserOrderManager';
import { ExchangeMetadataWorkerType } from './ExchangeMetadataWorkerType';
import { mainAcceptsFrames } from './frameForwarding';
import { BaseHandler } from './handlers/BaseHandler';
import { CallMethodHandler } from './handlers/CallMethodHandler';
import { ChaserOrderHandler } from './handlers/ChaserOrderHandler';
import { CommandHandler } from './handlers/CommandHandler';
import { DevHandler } from './handlers/DevHandler';
import { OHLCVSubscriptionsHandler } from './handlers/OHLCVSubscriptionsHandler';
import { OrderBookSubscriptionsHandler } from './handlers/OrderBookSubscriptionsHandler';
import { TwapHandler } from './handlers/TwapHandler';
import { UiStateHandler } from './handlers/UiStateHandler';
import { UpdatePositionsHandler } from './handlers/UpdatePositionsHandler';
import { exchangeDependentPostprocessors, independentPostprocessors } from './postProcessors/registry';

type WorkerSentryControl = {
  reportKey?: string;
  reportCooldownMs?: number;
  tags?: Record<string, unknown>;
  extra?: Record<string, unknown>;
};

type WorkerSentryControlledError = {
  safeCexSentryReportKey?: unknown;
  safeCexSentryReportCooldownMs?: unknown;
  safeCexSentryTags?: unknown;
  safeCexSentryExtra?: unknown;
};

const isRecord = (value: unknown): value is Record<string, unknown> =>
  Boolean(value && typeof value === 'object' && !Array.isArray(value));

const getWorkerSentryControl = (error: unknown): WorkerSentryControl => {
  if (!isRecord(error)) return {};

  const controlled = error as WorkerSentryControlledError;
  return {
    reportKey:
      typeof controlled.safeCexSentryReportKey === 'string' && controlled.safeCexSentryReportKey
        ? controlled.safeCexSentryReportKey
        : undefined,
    reportCooldownMs:
      typeof controlled.safeCexSentryReportCooldownMs === 'number' &&
      Number.isFinite(controlled.safeCexSentryReportCooldownMs) &&
      controlled.safeCexSentryReportCooldownMs > 0
        ? controlled.safeCexSentryReportCooldownMs
        : undefined,
    tags: isRecord(controlled.safeCexSentryTags) ? controlled.safeCexSentryTags : undefined,
    extra: isRecord(controlled.safeCexSentryExtra) ? controlled.safeCexSentryExtra : undefined,
  };
};

const splitWorkerLogExtra = (extra: Record<string, any> | undefined) => {
  if (!isRecord(extra)) {
    return { extra: {}, sentryTags: {}, sentryExtra: {}, suppressConsoleLog: false };
  }

  const sentryTags = isRecord(extra.sentryTags) ? extra.sentryTags : {};
  const sentryExtra = isRecord(extra.sentryExtra) ? extra.sentryExtra : {};
  const suppressConsoleLog = Boolean(extra.suppressConsoleLog);
  const plainExtra = Object.fromEntries(
    Object.entries(extra).filter(
      ([key]) => key !== 'sentryTags' && key !== 'sentryExtra' && key !== 'suppressConsoleLog',
    ),
  );

  return { extra: plainExtra, sentryTags, sentryExtra, suppressConsoleLog };
};

export class ExchangeWorker {
  private readonly _exchangeName: ExchangeName;
  private readonly _accountName: string;
  private readonly _options: ExchangeOptions;
  private readonly _ctx: Worker;
  private readonly _settings: AppSettingsWorkerType;
  private readonly _metadata: ExchangeMetadataWorkerType;
  private readonly _sessionKey: string;
  private _throttledStoreUpdate: ReturnType<typeof throttle<(data: StoreData) => void>>;
  instanceId: string | undefined = undefined;

  private _exchange: Exchange | null = null;
  private _store: Store = new DefaultStore();
  private _messageBuffer: StoreWorkerPayload[] = [];
  // Monotonic token bumped on every dispose() to invalidate any in-flight
  // start() call. Together with _cancelStartup, this lets a dispose abort a
  // concurrent startup instead of letting it race and clobber fresh state.
  private _startupToken = 0;
  private _cancelStartup: (() => void) | null = null;

  private readonly _handlerClasses = [
    OHLCVSubscriptionsHandler,
    OrderBookSubscriptionsHandler,
    ChaserOrderHandler,
    TwapHandler,
    DevHandler,
    CommandHandler,
    UpdatePositionsHandler,
    CallMethodHandler,
    UiStateHandler,
  ];

  private _postprocessors: { process(store: any): void }[] = [];
  private _exchangeDependentPostprocessors: { process(store: any): void }[] = [];
  private _handlers: BaseHandler[] = [];
  private _lastSentMarketsVersion: number | null = null;
  private _lastSentTickersVersion: number | null = null;
  private _lastSentTickerBaseline: Ticker[] | null = null;
  private _lastSentOrdersUpdateId: number | null = null;
  private _lastSentPositionsUpdateId: number | null = null;
  private _lastPayloadHadActiveChasers = false;
  /** Venue data reaches main as engine frames, not in store payloads. */
  private _forwardsFrames = false;
  private _sentryReportCooldowns = new Map<string, { lastReportedAt: number; suppressedCount: number }>();

  constructor(
    ctx: Worker,
    accountName: string,
    exchangeName: ExchangeName,
    options: ExchangeOptions,
    settings: AppSettingsWorkerType,
    metadata: ExchangeMetadataWorkerType,
    sessionKey: string,
    instanceId: string | undefined = undefined,
  ) {
    this._exchangeName = exchangeName;
    this._accountName = accountName;
    this._options = options;
    this._ctx = ctx;
    this._settings = settings;
    this._metadata = metadata;
    this._sessionKey = sessionKey;
    this.instanceId = instanceId;

    const throttleMs = 250 * (this._options?.tuning?.dataUpdateThrottleMultiplier || 1);
    this._throttledStoreUpdate = throttle((data: StoreData) => this._storeUpdate(data), throttleMs, {
      trailing: true,
    });
  }

  /**
   * Helper method to send log messages through the worker messaging system
   */
  private log(level: 'info' | 'warn' | 'error', message: string, context?: Record<string, any>) {
    this._ctx.postMessage({
      op: 'log',
      data: {
        level,
        message,
        context: {
          exchange: this._exchangeName,
          account: this._accountName,
          ...context,
        },
      },
    });
  }

  /**
   * Helper method to send errors through the worker messaging system
   */
  private logError(message: string, error: Error | unknown, extra?: Record<string, any>, sendToSentry = true) {
    const workerSentryControl = getWorkerSentryControl(error);
    const splitExtra = splitWorkerLogExtra(extra);
    let effectiveSendToSentry = sendToSentry;
    let suppressConsoleLog = splitExtra.suppressConsoleLog;
    let suppressedRepeatedReports = 0;

    if (sendToSentry && workerSentryControl.reportKey && workerSentryControl.reportCooldownMs) {
      const now = Date.now();
      const previous = this._sentryReportCooldowns.get(workerSentryControl.reportKey);
      const shouldReport =
        previous === undefined || now - previous.lastReportedAt >= workerSentryControl.reportCooldownMs;

      if (shouldReport) {
        suppressedRepeatedReports = previous?.suppressedCount ?? 0;
        this._sentryReportCooldowns.set(workerSentryControl.reportKey, {
          lastReportedAt: now,
          suppressedCount: 0,
        });
      } else {
        effectiveSendToSentry = false;
        suppressConsoleLog = true;
        this._sentryReportCooldowns.set(workerSentryControl.reportKey, {
          lastReportedAt: previous.lastReportedAt,
          suppressedCount: previous.suppressedCount + 1,
        });
      }
    }

    // Create a serializable error object
    const serializableError =
      error instanceof Error
        ? {
            message: error.message,
            name: error.name,
            stack: error.stack,
          }
        : {
            message: String(error),
            name: 'UnknownError',
            stack: new Error().stack,
          };

    try {
      this._ctx.postMessage({
        op: 'error',
        data: [
          message,
          {
            sendToSentry: effectiveSendToSentry,
            suppressConsoleLog,
            sentryData: {
              error: serializableError,
              tags: {
                source: 'webworker',
                module: 'ExchangeWorker',
                exchange: this._exchangeName,
                account: this._accountName,
                ...splitExtra.sentryTags,
                ...workerSentryControl.tags,
              },
              extra: {
                ...splitExtra.extra,
                ...splitExtra.sentryExtra,
                ...workerSentryControl.extra,
                ...(suppressedRepeatedReports > 0 ? { suppressedRepeatedReports } : {}),
              },
            },
          },
        ],
      });
    } catch (e) {
      // Fallback if postMessage fails
      console.error('[ExchangeWorker] Failed to send error to main thread:', message, error, e);
    }
  }

  private _flushBufferedMessages() {
    if (this._messageBuffer.length === 0) {
      return;
    }

    const bufferCount = this._messageBuffer.length;

    if (bufferCount > 10) {
      this.log('info', `Processed ${bufferCount} buffered messages during initialization`, {
        bufferCount: bufferCount,
      });
    }

    const bufferedMessages = [...this._messageBuffer];
    this._messageBuffer = [];

    bufferedMessages.forEach((payload, index) => {
      try {
        this.handlePayload(payload);
      } catch (error) {
        this.logError(`Failed to process buffered message ${index + 1}/${bufferCount} ` + String(error), error, {
          payloadOp: payload.op,
          messageIndex: index,
          totalBuffered: bufferCount,
        });
      }
    });
  }

  /**
   * Bootstraps the exchange inside the worker.
   *
   * Returns `true` if the exchange started successfully, `false` if startup
   * was cancelled (dispose called mid-flight), superseded by a newer
   * _startupToken, or the exchange was disposed before completion.
   *
   * Cancellation path: dispose() bumps _startupToken and invokes
   * _cancelStartup which resolves the cancelledPromise branch of the race;
   * post-await token checks catch the stale run and bail before we clobber
   * fresh state belonging to a subsequent start().
   */
  async start(): Promise<boolean> {
    const startupToken = this._startupToken;
    const cancelledPromise = new Promise<'cancelled'>((resolve) => {
      this._cancelStartup = () => {
        resolve('cancelled');
      };
    });

    // The try spans the WHOLE body, not just the exchange's own start(). Half
    // of startup runs before it — the registry lookup, the dynamic `loader()`
    // chunk fetch, the constructor, handler wiring — and those used to reject
    // out of `start()` entirely. The only caller (`store.webworker.ts`) awaits
    // without a catch, so they reached the global `unhandledrejection` handler
    // as `webworker-uncaught` with no phase tag, AND skipped that caller's
    // `!started` cleanup, leaving a half-initialized worker in the map.
    try {
      await virtualClock.start();
      if (startupToken !== this._startupToken) {
        return false;
      }

      const loader = exchangeRegistry[this._exchangeName];
      if (!loader) {
        throw new Error(`Unknown exchange: ${this._exchangeName}`);
      }
      const runsOnRust = this._options.engine === 'rust' && isRustVenue(this._exchangeName);
      const Exchange = runsOnRust ? RustEngineExchange : await loader();
      if (startupToken !== this._startupToken) {
        return false;
      }
      this._exchange = new Exchange(this._exchangeName, this._options, this._store);
      if (this._exchange instanceof RustEngineExchange && mainAcceptsFrames()) {
        // Main reads venue data straight from the frames; store payloads carry the rest.
        this._forwardsFrames = true;
        this._exchange.onFrame = (frame) => this._ctx.postMessage({ op: 'frame', data: frame }, [frame]);
      }

      // When privateOnly, disable public websocket connections before start().
      // Public data (tickers, orderbook, OHLCV) is provided by a shared public exchange.
      if (this._options.privateOnly) {
        this._exchange.disablePublicWebsockets();
      }

      this._initializeProcessorsAndHandlers();
      this._setupExchangeEventListeners();

      this.log('info', `Starting exchange in webworker: ${this._exchangeName}`);

      const START_TIMEOUT_MS = 30_000;
      const startPromise = this._exchange.start().then(() => 'started' as const);
      const timeoutPromise = new Promise<never>((_, reject) => {
        setTimeout(
          () => reject(new Error(`Exchange start timed out after ${START_TIMEOUT_MS / 1000}s`)),
          START_TIMEOUT_MS,
        );
      });

      const result = await Promise.race([startPromise, timeoutPromise, cancelledPromise]);
      if (result === 'cancelled' || startupToken !== this._startupToken || this._exchange?.isDisposed) {
        return false;
      }

      this._flushBufferedMessages();
      if (startupToken !== this._startupToken || this._exchange?.isDisposed) {
        return false;
      }

      this._throttledStoreUpdate.cancel();
      this._storeUpdate(this._store.serialize(), { forceFullStore: true });
      return true;
    } catch (e) {
      if (startupToken !== this._startupToken || this._exchange?.isDisposed) {
        console.warn(`Error starting exchange (disposed, suppressing): ${this._exchangeName}`, e);
        return false;
      }
      const detail = e instanceof Error && e.message ? `: ${e.message}` : '';
      // "Error starting exchange" is the LOG MESSAGE, not `error.message`, so
      // nothing downstream can classify a startup failure from the error alone.
      // Only bybit self-tags (BybitTradingMarketsRequestError); every other
      // venue arrived unlabelled and indistinguishable from any other worker
      // error. The phase is orthogonal to `worker_error_family` — both ride.
      this.logError(`Error starting exchange: ${this._exchangeName}${detail}`, e, {
        sentryTags: { worker_error_phase: 'startup' },
      });
      return false;
    } finally {
      if (startupToken === this._startupToken) {
        this._cancelStartup = null;
      }
    }
  }

  /**
   * Sanitize data to remove circular references before sending via postMessage.
   * The main culprit is the 'info' property on orders and positions which contains raw exchange data.
   *
   * Per-array fast-path: scan once for any item carrying `info`. If none do,
   * keep the original array reference instead of running `.map()` (which
   * always allocates a new array of the same length, even when every callback
   * returns the input). Skips a meaningful chunk of allocator pressure when
   * exchanges trim `info` upstream or for fresh accounts with no live data.
   */
  private _sanitizeForPostMessage = (
    data: StoreData,
    options?: { omitOrders?: boolean; omitPositions?: boolean },
  ): StoreWorkerStorePayload => {
    const result: StoreWorkerStorePayload = { ...data };

    // Decouple updateIds from the live store reference. Without this, a WS frame
    // (e.g. order:update Canceled) firing addOrUpdateOrders between
    // _prepareStorePayload returning and the structured clone running at
    // postMessage time will bump `prepared.updateIds.orders` after the
    // shouldSendOrders decision is already locked in as `omit`. The receiver
    // then sees a payload claiming "updateIds=N+1" with `orders` omitted,
    // advances its own updateIds, and the controller's pruneUnchangedWorkerStorePayload
    // drops the orders from the next (real) payload because the updateIds match.
    if (result.updateIds) {
      result.updateIds = { ...result.updateIds };
    }
    if (result.loaded) {
      result.loaded = { ...result.loaded };
    }
    if (result.socketHealth) {
      result.socketHealth = { ...result.socketHealth };
    }

    if (options?.omitOrders) {
      delete result.orders;
    } else if (result.orders && Array.isArray(result.orders)) {
      let needs = false;
      for (let i = 0; i < result.orders.length; i++) {
        if (result.orders[i].info) {
          needs = true;
          break;
        }
      }
      if (needs) {
        result.orders = result.orders.map((order) => {
          if (order.info) {
            const { info: _info, ...orderWithoutInfo } = order;
            return orderWithoutInfo;
          }
          return order;
        });
      }
    }

    if (options?.omitPositions) {
      delete result.positions;
    } else if (result.positions && Array.isArray(result.positions)) {
      let needs = false;
      for (let i = 0; i < result.positions.length; i++) {
        if (result.positions[i].info) {
          needs = true;
          break;
        }
      }
      if (needs) {
        result.positions = result.positions.map((position) => {
          if (position.info) {
            const { info: _info, ...positionWithoutInfo } = position;
            return positionWithoutInfo;
          }
          return position;
        });
      }
    }

    if (result.displayedPositions && Array.isArray(result.displayedPositions)) {
      let needs = false;
      for (let i = 0; i < result.displayedPositions.length; i++) {
        if (result.displayedPositions[i].info) {
          needs = true;
          break;
        }
      }
      if (needs) {
        result.displayedPositions = result.displayedPositions.map((position) => {
          if (position.info) {
            const { info: _info, ...positionWithoutInfo } = position;
            return positionWithoutInfo;
          }
          return position;
        });
      }
    }

    return result;
  };

  private _shouldSendUpdateIdBackedSnapshot = (
    updateId: number | undefined,
    lastSentUpdateId: number | null,
    forceFullStore: boolean | undefined,
  ) => {
    if (forceFullStore) {
      return true;
    }
    if (typeof updateId !== 'number') {
      return true;
    }
    return lastSentUpdateId !== updateId;
  };

  private _recordSentUpdateIdBackedSnapshot = (updateId: number | undefined) => {
    return typeof updateId === 'number' ? updateId : null;
  };

  private _copyTickerBaseline = (tickers: StoreData['tickers']) => {
    let baseline = this._lastSentTickerBaseline;
    if (!baseline || baseline.length !== tickers.length) {
      baseline = new Array<Ticker>(tickers.length);
      this._lastSentTickerBaseline = baseline;
    }

    for (let i = 0; i < tickers.length; i++) {
      const ticker = tickers[i];
      let snapshot = baseline[i] as Ticker | undefined;
      if (!snapshot) {
        snapshot = {
          id: ticker.id,
          symbol: ticker.symbol,
        } as Ticker;
        baseline[i] = snapshot;
      }

      const writableSnapshot = snapshot as any;
      writableSnapshot.id = ticker.id;
      writableSnapshot.symbol = ticker.symbol;
      for (let fieldIndex = 0; fieldIndex < PACKED_TICKER_FIELDS.length; fieldIndex++) {
        const field = PACKED_TICKER_FIELDS[fieldIndex];
        writableSnapshot[field] = ticker[field];
      }
    }
  };

  private _packTickerDeltasSinceBaseline = (tickers: StoreData['tickers']): PackedTickerDelta[] | null => {
    const baseline = this._lastSentTickerBaseline;
    if (!baseline || baseline.length !== tickers.length) {
      return null;
    }

    const packed: PackedTickerDelta[] = [];
    for (let i = 0; i < tickers.length; i++) {
      const ticker = tickers[i];
      const previous = baseline[i];
      if (!previous || ticker.id !== previous.id || ticker.symbol !== previous.symbol) {
        return null;
      }

      let mask = 0;
      const values: number[] = [];
      const writablePrevious = previous as any;
      for (let fieldIndex = 0; fieldIndex < PACKED_TICKER_FIELDS.length; fieldIndex++) {
        const field = PACKED_TICKER_FIELDS[fieldIndex];
        const value = ticker[field];
        if (value !== previous[field]) {
          writablePrevious[field] = value;
          if (typeof value === 'number') {
            mask |= 1 << fieldIndex;
            values.push(value);
          }
        }
      }

      if (mask !== 0) {
        packed.push([i, mask, ...values]);
      }
    }

    return packed;
  };

  private _prepareStorePayload = (data: StoreData, options?: { forceFullStore?: boolean }): StoreWorkerStorePayload => {
    const ordersUpdateId = data.updateIds?.orders;
    const positionsUpdateId = data.updateIds?.positions;
    const hasActiveChasers = (data.activeChasers?.length ?? 0) > 0;
    const canOmitOrders = !hasActiveChasers && !this._lastPayloadHadActiveChasers;
    const shouldSendOrders =
      !canOmitOrders ||
      this._shouldSendUpdateIdBackedSnapshot(ordersUpdateId, this._lastSentOrdersUpdateId, options?.forceFullStore);
    const shouldSendPositions = this._shouldSendUpdateIdBackedSnapshot(
      positionsUpdateId,
      this._lastSentPositionsUpdateId,
      options?.forceFullStore,
    );

    // Raw positions are updateId-backed. Raw orders are too, except active
    // chaser enrichment can update order.extra from activeChasers without an
    // orders updateId bump. displayedPositions are derived from positions plus
    // tickers, markets, balance, active algos, and rolling PnL, so do not gate
    // them on the raw positions updateId here.
    const sanitizedData = this._sanitizeForPostMessage(data, {
      omitOrders: !shouldSendOrders,
      omitPositions: !shouldSendPositions,
    });
    const marketsVersion = this._store.getMarketsVersion();
    const tickersVersion = this._store.getTickersVersion();
    const tickers = sanitizedData.tickers ?? data.tickers;
    let payload: StoreWorkerStorePayload = sanitizedData;
    const ensurePayloadCopy = () => {
      if (payload === sanitizedData) {
        payload = { ...sanitizedData };
      }
      return payload;
    };

    // Packing ticker deltas is a BROWSER-worker optimization: it shrinks the
    // structured clone across a real `postMessage`. In hosted mode the far side
    // is a VirtualWorkerContext that already does its own per-item partial diff
    // of `tickers` (`PARTIAL_DELTA_KEYS`), so packing here stacks two delta
    // layers — and that stack is what froze tickers in production on
    // 2026-09-14.
    //
    // Stacking is unsafe because the layers have different properties. A full
    // array is idempotent: drop it and the next one carries current values. A
    // delta is not: this function advances `_lastSentTickerBaseline` WHILE
    // packing, so anything lost downstream is never sent again. The outer layer
    // both throttles (dropping payloads) and treats the inner delta list as
    // accumulated STATE, which is exactly the mismatch.
    const packTickerDeltas = !this._options.hostedMode;

    const shouldSendFullTickers =
      !packTickerDeltas ||
      options?.forceFullStore ||
      this._lastSentTickersVersion !== tickersVersion ||
      this._lastSentTickerBaseline === null;

    if (shouldSendOrders) {
      this._lastSentOrdersUpdateId = this._recordSentUpdateIdBackedSnapshot(ordersUpdateId);
    }
    if (shouldSendPositions) {
      this._lastSentPositionsUpdateId = this._recordSentUpdateIdBackedSnapshot(positionsUpdateId);
    }
    this._lastPayloadHadActiveChasers = hasActiveChasers;

    if (shouldSendFullTickers) {
      this._lastSentTickersVersion = tickersVersion;
      // No baseline in hosted mode: nothing consumes it, and maintaining it is
      // an O(tickers) copy on every payload.
      if (packTickerDeltas) this._copyTickerBaseline(tickers);
    } else {
      const packedTickerDeltas = this._packTickerDeltasSinceBaseline(tickers);
      if (packedTickerDeltas === null) {
        this._lastSentTickersVersion = tickersVersion;
        this._copyTickerBaseline(tickers);
      } else {
        delete ensurePayloadCopy().tickers;
        if (packedTickerDeltas.length > 0) {
          ensurePayloadCopy().tickerDeltas = packedTickerDeltas;
        }
      }
    }

    if (!options?.forceFullStore && this._lastSentMarketsVersion === marketsVersion) {
      delete ensurePayloadCopy().markets;
      return payload;
    }

    this._lastSentMarketsVersion = marketsVersion;
    return payload;
  };

  private _storeUpdate = (data: StoreData, options?: { forceFullStore?: boolean }) => {
    let processedData = data;
    for (let i = 0; i < this._exchangeDependentPostprocessors.length; i++) {
      this._exchangeDependentPostprocessors[i].process(processedData);
    }
    for (let i = 0; i < this._postprocessors.length; i++) {
      this._postprocessors[i].process(processedData);
    }

    if (this._forwardsFrames) {
      this._ctx.postMessage({ op: 'store', data: this._derivedStorePayload(processedData) });
      return;
    }

    // Send store data with full baseline fields preserved for initial load /
    // sendSnapshot(). Routine live payloads can omit stable fields here; the
    // main-thread store applies the remaining fields as a partial update.
    this._ctx.postMessage({ op: 'store', data: this._prepareStorePayload(processedData, options) });
  };

  /**
   * Frame mode: venue state (markets, tickers, orders, positions, balance,
   * leverage, loaded flags) reaches main in frames, so a store payload carries
   * only what the worker derives from it, sanitized as usual.
   */
  private _derivedStorePayload = (data: StoreData): StoreWorkerStorePayload => {
    const {
      markets: _markets,
      tickers: _tickers,
      orders: _orders,
      positions: _positions,
      balance: _balance,
      fetchedLeverage: _fetchedLeverage,
      loaded: _loaded,
      latency: _latency,
      options: _options,
      updateIds: _updateIds,
      ...derived
    } = this._sanitizeForPostMessage(data);
    return derived;
  };

  get store() {
    return this._store;
  }

  get exchange() {
    return this._exchange;
  }

  sendSnapshot() {
    this._throttledStoreUpdate.cancel();
    if (this._exchange?.isDisposed) {
      return;
    }
    if (this._forwardsFrames && this._exchange instanceof RustEngineExchange) {
      const frame = this._exchange.snapshotFrame();
      if (frame) this._ctx.postMessage({ op: 'frame', data: frame }, [frame]);
    }
    this._storeUpdate(this._store.serialize(), { forceFullStore: true });
  }

  dispose() {
    // Bump the token so any in-flight start() observes the change after its
    // next await and bails instead of proceeding to clobber state.
    this._startupToken++;
    this._cancelStartup?.();
    this._cancelStartup = null;
    if (this._exchange) {
      chaserOrderManagerInstance.cleanupExchange(this._exchange);
    }
    this._exchange?.dispose();
    this._clearRuntimePipeline();
  }

  private _clearRuntimePipeline() {
    this._throttledStoreUpdate.cancel();
    this._handlers.forEach((handler) => handler.dispose());
    this._handlers = [];
    this._postprocessors = [];
    this._exchangeDependentPostprocessors = [];
  }

  handlePayload = (payload: StoreWorkerPayload) => {
    if (!this._exchange) {
      // Buffer messages until exchange is initialized
      const MAX_BUFFER_SIZE = 1000;
      if (this._messageBuffer.length < MAX_BUFFER_SIZE) {
        this._messageBuffer.push(payload);
      } else {
        // Send warning through proper logging infrastructure
        this.logError(
          `Message buffer full for ${this._exchangeName}, dropping message`,
          new Error('Message buffer overflow'),
          {
            droppedPayload: payload.op,
            maxBufferSize: MAX_BUFFER_SIZE,
            bufferSize: this._messageBuffer.length,
          },
          false, // This is a warning, not an error for Sentry
        );
      }
      return;
    }
    switch (payload.op) {
      case 'store':
        this._exchange.store.update(payload.data);
        break;
      case 'merge-loaded':
        // Merge loaded flags from shared public exchange without clobbering
        // existing loaded flags (e.g., balance/orders/positions from private exchange).
        if (payload.data && this._exchange) {
          this._exchange.store.update({
            loaded: { ...this._exchange.store.loaded, ...payload.data },
          });
        }
        break;
      case 'merge-message-handler-stats':
        // Merge hosted public websocket stats into the private account store
        // without overwriting private stats like pong/account streams.
        if (payload.data && this._exchange) {
          this._exchange.store.update({
            messageHandlerStats: {
              ...this._exchange.store.messageHandlerStats,
              ...(payload.data as Record<string, unknown>),
            },
          });
        }
        break;
      case 'dispose':
        this.dispose();
        break;
      case 'deactivate':
        this.log('info', `Deactivating exchange: ${this._exchangeName}`);
        this.dispose();
        this._exchange = null;
        this._clearRuntimePipeline();
        this._ctx.postMessage({ op: 'deactivate-status', data: true });
        break;
      case 'sayHi':
        break;
      default:
        for (let i = 0; i < this._handlers.length; i++) {
          this._handlers[i].handle(payload);
        }
    }
  };

  private _initializeProcessorsAndHandlers() {
    // In hosted mode, independent post-processors run client-side (ExchangeServerWorker).
    // In non-hosted mode, everything runs here.
    if (!this._options.hostedMode) {
      this._postprocessors = independentPostprocessors.map(
        (Cls) => new (Cls as any)(this._accountName, this._exchangeName, this._options),
      );
    }

    // Exchange-dependent post-processors always run where the exchange lives.
    this._exchangeDependentPostprocessors = exchangeDependentPostprocessors.map(
      (Cls) => new (Cls as any)(this._accountName, this._exchangeName, this._options, this._exchange!),
    );

    this._handlers = this._handlerClasses.map(
      (Handler) =>
        new Handler(this._accountName, this._exchangeName, this._exchange!, this._ctx, this._settings, this._metadata),
    );
  }

  private _setupExchangeEventListeners() {
    this._exchange!.store.subscribe((data) => {
      this._throttledStoreUpdate(data);
    });

    this._exchange!.on('fill', (event: OrderFillEvent) => {
      if (!this._forwardsFrames) this._ctx.postMessage({ op: 'fill', data: event });
    });

    const logEvents = [
      'error',
      'warning',
      'log',
      'exec-log',
      // `exec-result` carries the capture-syntax `data.orderIds`
      // payload back to the browser CLI. Wire shape:
      //   { reqId: string, handled: boolean, success?: boolean, error?: string, data?: { orderIds?: string[] } }
      // Same auto-bridge pipeline as `exec-log` — browser receives via
      // exchangeAccountController's worker listener and re-emits on the
      // main-thread `exchange.emitter`.
      'exec-result',
      // `chaser:complete` lets the browser attach TP/SL to a finished chase.
      'chaser:complete',
      'grafana-info',
      'grafana-error',
      'grafana-pws',
      'grafana-pws-sub',
      'grafana-http-request',
      'grafana-http-response',
    ];
    logEvents.forEach((event) => {
      this._exchange!.on(event, (...args: any[]) => {
        this._ctx.postMessage({ op: event, data: args });
      });
    });

    this._exchange!.on('ws-reconnected', () => {
      this._ctx.postMessage({ op: 'ws-reconnected' });
    });

    this._exchange!.on('deactivate', () => {
      this.log('info', `Exchange emitted deactivate event: ${this._exchangeName}`);
      // Dispose through the worker so any in-flight start() gets cancelled.
      this.dispose();
      this._exchange = null;
      this._clearRuntimePipeline();
      // Notify main thread of deactivation
      this._ctx.postMessage({ op: 'deactivate-status', data: true });
    });
  }
}
