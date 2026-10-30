import type { CommandProgress } from '@tealstreet/safe-cex-wasm/host';
import type { SerializedCommandError } from '../commandOutcome';
import type { Exchange } from '../exchanges/base';
import type { PackedTickerDelta, Store } from '../store/store.interface';
import type { ExchangeName, ExchangeOptions, OrderFillEvent, StoreData, Ticker } from '../types';
import type { StoreWorkerCommandProgress, StoreWorkerPayload, StoreWorkerStorePayload } from './StoreWorkerPayload';
import type { TwapInstance } from './twap/TwapInstance';

import throttle from 'lodash/throttle';

import { classifiedError, exchangeErrorTags } from '../commandOutcome';
import { WORKER_WRITE_METHODS, workerCallClientOrderIds } from '../exchanges/base';
import { exchangeRegistry } from '../exchanges/registry';
import { assertBetaAccount } from '../exchanges/rust/betaVenues';
import { RustEngineExchange } from '../exchanges/rust/rust.exchange';
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
import { twapManager } from './twap/TwapManager';

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

const USER_STARTED_ALGORITHMS = new Set<StoreWorkerPayload['op']>(['chaser:start', 'twap:start', 'twap:resume']);

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
  private _generation = 0;
  private _started = false;
  private _stopped = false;
  private _disposed = false;
  private _recovering = false;
  private _recoveryAttempted = false;
  private _recoveryTimer: ReturnType<typeof setTimeout> | null = null;
  private _recoveryUpdateIds: StoreData['updateIds'] | undefined;
  private _unsubscribeStore: (() => void) | null = null;
  private readonly _ownedTwaps = new Set<TwapInstance>();
  private readonly _publicDemand = new Map<string, StoreWorkerPayload>();
  private readonly _pendingRequests = new Map<string, { payload: StoreWorkerPayload; generation: number }>();
  private _pendingFailureTimer: ReturnType<typeof setTimeout> | null = null;
  private _activeTickerSymbols: string[] = [];
  private _latestUiState: StoreWorkerPayload | null = null;

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
  /** Main decodes books, candles and fills from the frames itself, so their ops go quiet. */
  private _mainDecodesFrames = false;
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
    // The worker owns the account's engine; a forwarded mirror flag would stop it from running one.
    const { workerMirror: _workerMirror, ...ownedOptions } = options;
    this._options = ownedOptions;
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
            classified: classifiedError(error),
            sentryData: {
              error: serializableError,
              tags: {
                source: 'webworker',
                module: 'ExchangeWorker',
                exchange: this._exchangeName,
                account: this._accountName,
                // Typed engine errors carry kind/outcome/venueCode; tag them so Sentry need not match text.
                ...exchangeErrorTags(error),
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
    const generation = ++this._generation;
    let startupTimeout: ReturnType<typeof setTimeout> | undefined;
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
      assertBetaAccount(this._exchangeName, this._options);
      await virtualClock.start();
      if (startupToken !== this._startupToken) {
        return false;
      }

      const loader = exchangeRegistry[this._exchangeName];
      if (!loader) {
        throw new Error(`Unknown exchange: ${this._exchangeName}`);
      }
      const Exchange = await loader();
      if (startupToken !== this._startupToken) {
        return false;
      }
      this._exchange = new Exchange(this._exchangeName, this._options, this._store);
      if (this._recoveryAttempted) this._exchange.setActiveTickerSymbols(this._activeTickerSymbols);
      if (mainAcceptsFrames()) this._mainDecodesFrames = this.forwardFrames(generation);
      // A recovered exchange keeps forwarding where main asked for this account's frames.
      else if (this._forwardsFrames) this.forwardFrames(generation);

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
        startupTimeout = setTimeout(
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
      if (this._exchange?.isDisposed) return false;
      this._started = true;
      return true;
    } catch (e) {
      if (
        startupToken !== this._startupToken ||
        (this._exchange?.isDisposed && !(this._exchange instanceof RustEngineExchange && this._exchange.isEngineDead))
      ) {
        console.warn(`Error starting exchange (disposed, suppressing): ${this._exchangeName}`, e);
        return false;
      }
      const detail = e instanceof Error && e.message ? `: ${e.message}` : '';
      // "Error starting exchange" is the LOG MESSAGE, not `error.message`, so
      // nothing downstream can classify a startup failure from the error alone.
      // Only bybit self-tags (BybitTradingMarketsRequestError); every other
      // venue arrived unlabelled and indistinguishable from any other worker
      // error. The phase is orthogonal to `worker_error_family` — both ride.
      if (this._exchange instanceof RustEngineExchange && this._exchange.isEngineDead) {
        // The adapter already emitted the fatal error. Keep the startup context
        // without sending a second error for the same trap.
        this.log('warn', `Error starting exchange: ${this._exchangeName}${detail}`, { worker_error_phase: 'startup' });
      } else {
        this.logError(`Error starting exchange: ${this._exchangeName}${detail}`, e, {
          sentryTags: { worker_error_phase: 'startup' },
        });
      }
      return false;
    } finally {
      if (startupTimeout !== undefined) clearTimeout(startupTimeout);
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
          if (typeof value === 'number' || field === 'midPrice') {
            mask |= 1 << fieldIndex;
            values.push(value ?? NaN);
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
      this._ctx.postMessage({ op: 'store', data: this._derivedStorePayload(processedData, options?.forceFullStore) });
      return;
    }

    // Send store data with full baseline fields preserved for initial load /
    // sendSnapshot(). Routine live payloads can omit stable fields here; the
    // main-thread store applies the remaining fields as a partial update.
    this._ctx.postMessage({ op: 'store', data: this._prepareStorePayload(processedData, options) });
  };

  /**
   * Frame mode: venue state (markets, tickers, orders, positions, balance,
   * loaded flags) reaches main in frames. Rare query-derived leverage also
   * travels in store payloads when changed, including late-client snapshots.
   * So does the balance when its `info` changes (Bybit wallet, Lighter tier):
   * frames carry no `info`, and this balance already includes every frame sent.
   */
  private _lastFrameLeverage?: StoreData['fetchedLeverage'];
  private _lastFrameBalanceInfo?: unknown;

  private _derivedStorePayload = (data: StoreData, force = false): StoreWorkerStorePayload => {
    const includeLeverage = force || data.fetchedLeverage !== this._lastFrameLeverage;
    this._lastFrameLeverage = data.fetchedLeverage;
    const info = data.balance?.info;
    const includeBalance = info !== undefined && (force || info !== this._lastFrameBalanceInfo);
    this._lastFrameBalanceInfo = info;
    const {
      markets: _markets,
      tickers: _tickers,
      orders: _orders,
      positions: _positions,
      balance,
      fetchedLeverage,
      loaded: _loaded,
      latency: _latency,
      options: _options,
      updateIds: _updateIds,
      ...derived
    } = this._sanitizeForPostMessage(data);
    return {
      ...derived,
      ...(includeLeverage && { fetchedLeverage }),
      ...(includeBalance && { balance }),
    };
  };

  get store() {
    return this._store;
  }

  get exchange() {
    return this._exchange;
  }

  get isRecovering() {
    return this._recovering;
  }

  sendSnapshot() {
    this._throttledStoreUpdate.cancel();
    if (this._exchange?.isDisposed) {
      return;
    }
    if (this._forwardsFrames && this._exchange instanceof RustEngineExchange) {
      const frame = this._exchange.snapshotFrame();
      if (frame) this._ctx.postMessage({ op: 'frame', data: frame, snapshot: true }, [frame]);
    }
    this._storeUpdate(this._store.serialize(), { forceFullStore: true });
  }

  /**
   * Main reads venue data straight from this account's frames; store payloads
   * carry the rest. Per account: a main-thread engine that is frame-fed asks for
   * it (`forwardFrames`), and one that is not keeps full store payloads.
   */
  private forwardFrames(generation = this._generation) {
    const exchange = this._exchange;
    if (!(exchange instanceof RustEngineExchange)) return false;
    this._forwardsFrames = true;
    exchange.onFrame = (frame, snapshot) => {
      if (generation === this._generation && !this._stopped) {
        this._ctx.postMessage({ op: 'frame', data: frame, snapshot: snapshot === true }, [frame]);
      }
    };
    return true;
  }

  dispose() {
    if (this._disposed) return;
    this._disposed = true;
    this._stopped = true;
    this._started = false;
    this._recovering = false;
    this._generation++;
    this._throttledStoreUpdate.cancel();
    if (this._recoveryTimer !== null) clearTimeout(this._recoveryTimer);
    this._recoveryTimer = null;
    this._publicDemand.clear();
    this._latestUiState = null;
    this._deferInterruptedRequests({
      message: 'Exchange disposed; interrupted operation outcomes are unknown and were not replayed.',
      name: 'EngineDisposedError',
      code: 'ENGINE_DISPOSED',
    });
    this._dropMessageBuffer();
    // Bump the token so any in-flight start() observes the change after its
    // next await and bails instead of proceeding to clobber state.
    this._startupToken++;
    this._cancelStartup?.();
    this._cancelStartup = null;
    try {
      this._stopOwnedTwaps();
      if (this._exchange) chaserOrderManagerInstance.cleanupExchange(this._exchange);
    } finally {
      try {
        // Native pending-command rejections (or acknowledgements already
        // queued) reach their original caller before the task fallback.
        this._exchange?.dispose();
      } finally {
        this._clearRuntimePipeline();
      }
    }
  }

  private _clearRuntimePipeline() {
    this._unsubscribeStore?.();
    this._unsubscribeStore = null;
    this._throttledStoreUpdate.cancel();
    this._handlers.forEach((handler) => handler.dispose());
    this._handlers = [];
    this._postprocessors = [];
    this._exchangeDependentPostprocessors = [];
  }

  handlePayload = (payload: StoreWorkerPayload) => {
    if (payload.op === 'dispose' || payload.op === 'deactivate') {
      this.dispose();
      if (payload.op === 'deactivate') {
        this._exchange = null;
        this._ctx.postMessage({ op: 'deactivate-status', data: true });
      }
      return;
    }
    if (this._stopped) {
      this._rejectUnavailablePayload(payload);
      return;
    }
    const isDemand = this._recordPublicDemand(payload);
    const isUiState = payload.op === 'ui:state';
    if (isUiState) this._latestUiState = { ...payload, data: { ...payload.data } };
    if (this._recovering || (this._exchange instanceof RustEngineExchange && this._exchange.isEngineDead)) {
      // Subscription state is safe to retain; trading/query requests are never replayed.
      if (!isDemand && !isUiState) this._rejectUnavailablePayload(payload);
      return;
    }
    if (!this._exchange) {
      // Buffer messages until exchange is initialized
      const MAX_BUFFER_SIZE = 1000;
      if (this._messageBuffer.length < MAX_BUFFER_SIZE) {
        this._messageBuffer.push(payload);
      } else {
        this._rejectNeverSent(payload, 'Worker startup queue is full; nothing was sent.');
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
    const requestKey = this._requestKey(payload);
    if (requestKey) this._pendingRequests.set(requestKey, { payload, generation: this._generation });
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
      case 'sayHi':
        break;
      case 'forwardFrames':
        // Started engines get a baseline at once; one still starting sends its own first.
        if (this.forwardFrames() && (this._exchange as RustEngineExchange).hasStarted) this.sendSnapshot();
        break;
      default:
        // TWAP construction is synchronous. Track the actual instances made by
        // this dispatch rather than using account names in the global manager.
        const dispatchGeneration = this._generation;
        const previousTwaps =
          typeof payload.op === 'string' && payload.op.startsWith('twap:')
            ? new Set(twapManager.runningTwaps.values())
            : null;
        for (let i = 0; i < this._handlers.length; i++) {
          this._handlers[i].handle(payload);
        }
        if (previousTwaps) {
          for (const twap of this._ownedTwaps) {
            if (twapManager.getById(twap.id) !== twap) this._ownedTwaps.delete(twap);
          }
          for (const twap of twapManager.runningTwaps.values()) {
            if (!previousTwaps.has(twap)) {
              if (dispatchGeneration !== this._generation || this._recovering || this._stopped) {
                void twap
                  .terminate()
                  .catch((error: unknown) => this.logError('Unable to stop interrupted TWAP.', error));
              } else this._ownedTwaps.add(twap);
            }
          }
        }
    }
  };

  private _recordPublicDemand(payload: StoreWorkerPayload): boolean {
    const book = payload.op === 'subscribeOrderBook' || payload.op === 'unsubscribeOrderBook';
    const candle = payload.op === 'subscribeOHLCV' || payload.op === 'unsubscribeOHLCV';
    if (!book && !candle) return false;
    const data = payload.data;
    if (!data || typeof data !== 'object') return true;
    if (data.subId !== undefined && typeof data.subId !== 'string' && typeof data.subId !== 'number') return true;
    // Unsubscribe handlers identify the stream by subId alone; the caller need
    // not repeat its symbol or candle interval.
    const key = JSON.stringify([book ? 'book' : 'candle', data.subId]);
    if (payload.op.startsWith('unsubscribe')) this._publicDemand.delete(key);
    else if (typeof data.symbol === 'string') this._publicDemand.set(key, { ...payload, data: { ...data } });
    return true;
  }

  private _requestKey(payload: StoreWorkerPayload) {
    if (!['callMethod', 'updatePositions', 'exec'].includes(payload.op)) return undefined;
    const id = payload.op === 'exec' ? payload.data?.reqId : payload.data?.requestId;
    return typeof id === 'string' || typeof id === 'number' ? JSON.stringify([payload.op, id]) : undefined;
  }

  private _replyRequestKey(message: StoreWorkerPayload) {
    const op =
      message.op === 'callMethod:done'
        ? 'callMethod'
        : message.op === 'updatePositions:done'
          ? 'updatePositions'
          : message.op === 'exec-result'
            ? 'exec'
            : undefined;
    if (!op) return undefined;
    const data = Array.isArray(message.data) ? message.data[0] : message.data;
    return this._requestKey({ op, data });
  }

  private _completeRequest(message: StoreWorkerPayload) {
    const key = this._replyRequestKey(message);
    if (key) this._pendingRequests.delete(key);
  }

  private _forwardLateReply(message: StoreWorkerPayload, generation: number) {
    const key = this._replyRequestKey(message);
    const data = Array.isArray(message.data) ? message.data[0] : message.data;
    // A completed command's correlated result belongs to its original caller,
    // including known write IDs acknowledged just before the trap. This cannot
    // publish state or complete a request from the replacement generation.
    if (!key || typeof data?.success !== 'boolean' || this._pendingRequests.get(key)?.generation !== generation) return;
    this._pendingRequests.delete(key);
    this._ctx.postMessage(message);
  }

  private _deferInterruptedRequests(fallback: SerializedCommandError) {
    // Explicit disposal after a trap must retain that generation's grace
    // window, rather than erase its native command-specific interruption.
    if (this._pendingFailureTimer !== null || !this._pendingRequests.size) return;
    const interrupted = [...this._pendingRequests.entries()];
    this._pendingFailureTimer = setTimeout(() => {
      this._pendingFailureTimer = null;
      for (const [key, request] of interrupted) {
        if (this._pendingRequests.get(key) !== request) continue;
        this._pendingRequests.delete(key);
        // Only these RPC methods dispatch venue writes. An executor's text and
        // caller IDs cannot be inferred here; actual native errors win first.
        const method = request.payload.op === 'callMethod' ? request.payload.data?.method : undefined;
        const clientOrderIds = workerCallClientOrderIds(method, request.payload.data?.args);
        const details: SerializedCommandError = {
          ...fallback,
          ...(typeof method === 'string' && { command: method }),
          ...(WORKER_WRITE_METHODS.has(method) && { outcome: 'unknown' as const }),
          ...(clientOrderIds && { clientOrderIds }),
        };
        this._rejectUnavailablePayload(request.payload, fallback.message, details);
      }
    }, 0);
  }

  /** A request still buffered when the account stops was never dispatched; answer it rather than leave it pending. */
  private _rejectNeverSent(
    payload: StoreWorkerPayload,
    message = 'Exchange stopped before this operation was sent; nothing was sent.',
  ) {
    if (!this._requestKey(payload)) return;
    const method = payload.op === 'callMethod' ? payload.data?.method : undefined;
    this._rejectUnavailablePayload(payload, message, {
      message,
      name: 'EngineUnavailableError',
      outcome: 'notSent',
      ...(typeof method === 'string' && { command: method }),
    });
  }

  private _dropMessageBuffer() {
    const dropped = this._messageBuffer;
    this._messageBuffer = [];
    for (const payload of dropped) this._rejectNeverSent(payload);
  }

  private _rejectUnavailablePayload(
    payload: StoreWorkerPayload,
    message = 'Rust engine unavailable; operation was not replayed. Reinitialize the account if recovery failed.',
    errorDetails?: SerializedCommandError,
  ) {
    if (payload.op === 'callMethod') {
      this._ctx.postMessage({
        op: 'callMethod:done',
        data: {
          requestId: payload.data?.requestId,
          success: false,
          result: errorDetails ?? { message, name: 'EngineUnavailableError' },
        },
      });
    } else if (payload.op === 'updatePositions') {
      this._ctx.postMessage({
        op: 'updatePositions:done',
        data: {
          requestId: payload.data?.requestId,
          success: false,
          reason: message,
          ...(errorDetails && { errorDetails }),
        },
      });
    } else if (payload.op === 'exec') {
      this._ctx.postMessage({
        op: 'exec-result',
        data: [
          {
            reqId: payload.data?.reqId,
            handled: true,
            success: false,
            error: message,
            ...(errorDetails && { errorDetails }),
          },
        ],
      });
    } else if (USER_STARTED_ALGORITHMS.has(payload.op)) {
      this.logError(message, new Error(message), { payloadOp: payload.op }, false);
    }
    // Store sync, UI state and stop/cleanup ops have nothing left to act on; an
    // error per routine message would toast on every symbol switch.
  }

  private _stopOwnedTwaps() {
    for (const twap of this._ownedTwaps) {
      void twap.terminate().catch((error: unknown) => this.logError('Unable to stop interrupted TWAP.', error));
    }
    this._ownedTwaps.clear();
  }

  private _invalidateReadiness() {
    const unavailable = {
      loaded: { markets: false, tickers: false, balance: false, orders: false, positions: false },
      activeChasers: [],
      activeTWAPs: [],
    };
    this._store.update(unavailable);
    // Availability is host lifecycle metadata. Keep last known venue values
    // until a replacement emits its real native empty/full snapshots.
    this._ctx.postMessage({ op: 'store', data: unavailable });
  }

  private _handleEngineDead(error: Error) {
    // Startup belongs to store.webworker's !started cleanup. Never race it by
    // scheduling a replacement before that initial start has returned.
    if (this._stopped) return;
    // BaseExchange.dispose resets its store. Preserve the last native freshness
    // counters before releasing it, so the replacement cannot rewind them.
    this._recoveryUpdateIds = { ...this._store.updateIds };
    if (!this._started || this._recovering) {
      this._invalidateReadiness();
      return;
    }
    const cannotRecover = this._recoveryAttempted || this._options.publicFeed?.mode === 'shared';
    this._started = false;
    this._recovering = true;
    this._activeTickerSymbols = [...(this._exchange?.getActiveTickerSymbols() ?? [])];
    this._deferInterruptedRequests({
      message:
        'Rust engine stopped; operation outcome is unknown. Reconcile the account before retrying; no operation was replayed.',
      name: 'EngineDeadError',
      code: 'ENGINE_DEAD',
      kind: 'internal',
    });
    // Stop TWAPs while their handler context is still current, so the final
    // completed state reaches the main thread that persists and resumes them.
    this._stopOwnedTwaps();
    this._generation++;
    this._clearRuntimePipeline();
    if (this._exchange) {
      chaserOrderManagerInstance.cleanupExchange(this._exchange);
      this._exchange.dispose();
    }
    this._dropMessageBuffer();
    this._invalidateReadiness();
    this.log('warn', 'Rust engine stopped; interrupted writes may have an unknown outcome and will not be replayed.', {
      reason: error.message,
    });
    if (cannotRecover) {
      this._stopped = true;
      this._recovering = false;
      const message =
        this._options.publicFeed?.mode === 'shared'
          ? 'Rust shared public feed requires a fresh provider attachment and account init after engine failure.'
          : 'Rust engine recovery exhausted; reinitialize the account to retry.';
      this._ctx.postMessage({ op: 'warning', data: [message] });
      return;
    }
    this._recoveryAttempted = true;
    const token = this._startupToken;
    this._recoveryTimer = setTimeout(() => {
      this._recoveryTimer = null;
      void this._recoverEngine(token).catch((failure: unknown) => {
        if (token !== this._startupToken || this._stopped) return;
        this.dispose();
        this.logError('Rust engine recovery failed; reinitialize the account.', failure);
      });
    }, 500);
  }

  private async _recoverEngine(token: number) {
    if (token !== this._startupToken || this._stopped) return;
    const previousUpdateIds = this._recoveryUpdateIds ?? this._store.serialize().updateIds;
    this._store = new DefaultStore();
    this._store.update({
      updateIds: {
        orders: (previousUpdateIds?.orders ?? 0) + 1,
        positions: (previousUpdateIds?.positions ?? 0) + 1,
      },
    });
    this._lastSentMarketsVersion = null;
    this._lastSentTickersVersion = null;
    this._lastSentTickerBaseline = null;
    this._lastSentOrdersUpdateId = null;
    this._lastSentPositionsUpdateId = null;
    this._lastPayloadHadActiveChasers = false;
    // In frame mode this is deliberately a FULL reset payload, rather than a
    // derived-only payload, so stale balances/orders/loaded flags disappear.
    this._ctx.postMessage({ op: 'store', data: this._sanitizeForPostMessage(this._store.serialize()) });
    const started = await this.start();
    if (token !== this._startupToken || this._stopped) return;
    if (!started || this._exchange?.isDisposed) {
      this.dispose();
      this._ctx.postMessage({ op: 'warning', data: ['Rust engine recovery failed; reinitialize the account.'] });
      return;
    }
    this._recovering = false;
    if (this._latestUiState) this.handlePayload(this._latestUiState);
    for (const demand of this._publicDemand.values()) this.handlePayload(demand);
    if (this._stopped || this._exchange?.isDisposed) return;
    this.sendSnapshot();
    if (!this._stopped && !this._exchange?.isDisposed) this._ctx.postMessage({ op: 'ws-reconnected' });
  }

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

    const generation = this._generation;
    const handlerContext = new Proxy(this._ctx, {
      get: (target, property) =>
        property === 'postMessage'
          ? (...args: Parameters<Worker['postMessage']>) => {
              if (generation === this._generation && !this._stopped) {
                this._completeRequest(args[0]);
                target.postMessage(...args);
              } else {
                this._forwardLateReply(args[0], generation);
              }
            }
          : Reflect.get(target, property),
    });
    this._handlers = this._handlerClasses.map(
      (Handler) =>
        new Handler(
          this._accountName,
          this._exchangeName,
          this._exchange!,
          handlerContext,
          this._settings,
          this._metadata,
        ),
    );
  }

  private _setupExchangeEventListeners() {
    const exchange = this._exchange!;
    const generation = this._generation;
    const isCurrent = () => generation === this._generation && !this._stopped;
    this._unsubscribeStore = exchange.store.subscribe((data) => {
      if (isCurrent()) this._throttledStoreUpdate(data);
    });
    if (exchange instanceof RustEngineExchange) {
      exchange.on('command-progress', (progress: CommandProgress) => {
        // A command deadline can remove its RPC before native code chooses an
        // identity. The current account still owns those later receipts; closed
        // generations can neither publish them nor relabel them as a new host.
        if (!isCurrent()) return;
        const data: StoreWorkerCommandProgress = { ...progress, account: this._accountName, generation };
        this._ctx.postMessage({ op: 'command-progress', data });
      });
      exchange.on('engine-dead', (error: Error) => {
        if (isCurrent()) this._handleEngineDead(error);
      });
    }

    this._exchange!.on('fill', (event: OrderFillEvent) => {
      if (isCurrent() && !this._mainDecodesFrames) this._ctx.postMessage({ op: 'fill', data: event });
    });

    const logEvents = [
      'engine-event',
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
        const message = { op: event as StoreWorkerPayload['op'], data: args };
        if (isCurrent()) {
          this._completeRequest(message);
          this._ctx.postMessage(message);
        } else {
          this._forwardLateReply(message, generation);
        }
      });
    });

    this._exchange!.on('ws-reconnected', () => {
      if (isCurrent()) this._ctx.postMessage({ op: 'ws-reconnected' });
    });

    this._exchange!.on('deactivate', () => {
      if (!isCurrent()) return;
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
