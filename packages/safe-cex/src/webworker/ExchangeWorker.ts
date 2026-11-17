import type { CommandProgress } from '@tealstreet/safe-cex-wasm/host';
import type { SerializedCommandError } from '../commandOutcome';
import type { Exchange } from '../exchanges/base';
import type { ExchangeName, ExchangeOptions, OrderFillEvent, StoreData, Ticker } from '../types';
import type { StoreWorkerCommandProgress, StoreWorkerPayload } from './StoreWorkerPayload';
import type { TwapOwner, TwapWorkerContext } from './twap/TwapOwner';
import { captureTwapOwner } from './twap/TwapOwner';


import { onTunnelLaneFallback, tunnelLanePort } from '@tealstreet/http-over-websocket/src/tunnelEndpoints';
import { classifiedError, exchangeErrorTags, historyErrorDiagnostics, isErrorReported } from '../commandOutcome';
import { WORKER_WRITE_METHODS, workerCallClientOrderIds } from '../exchanges/base';
import { exchangeRegistry } from '../exchanges/registry';
import { assertBetaAccount } from '../exchanges/rust/betaVenues';
import { snapshotDelegatedSigningOptions } from '../exchanges/rust/delegatedSigningSnapshot';
import { RustEngineExchange } from '../exchanges/rust/rust.exchange';
import { FrameStore } from '../store/frameStore';
import { unsupportedVenueError } from '../unsupportedVenue';
import { diagnosticConsole, diagnosticText, redactDiagnosticData } from '../utils/diagnosticRedaction';
import { virtualClock } from '../utils/virtualClock';
import { workerErrorProtectionTags } from '../utils/workerErrorProtectionTags';
import { AppSettingsWorkerType } from './AppSettingsWorkerType';
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

// The tunnel lane is per realm, so one live ExchangeWorker reports this realm's fallback.
const liveExchangeWorkers = new Set<ExchangeWorker>();
onTunnelLaneFallback((failures) => liveExchangeWorkers.values().next().value?.reportTunnelLaneFallback(failures));

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

/** A slow link is not a failed start: the Rust engine gets this long, other venues keep their 30 s. */
const RUST_START_TIMEOUT_MS = 120_000;
const START_TIMEOUT_MS = 30_000;
/** A Rust start that still ran out of time starts over after each of these, then gives up visibly. */
const START_RETRY_DELAYS_MS = [10_000, 20_000, 40_000, 60_000];

/** A request held for an exchange that does not exist yet, matching the engine's write refusal. */
const BUFFERED_REQUEST_WAIT_MS = 30_000;

const USER_STARTED_ALGORITHMS = new Set<StoreWorkerPayload['op']>(['chaser:start', 'twap:start', 'twap:resume']);

export class ExchangeWorker {
  private readonly _exchangeName: ExchangeName;
  private readonly _accountName: string;
  private readonly _options: ExchangeOptions;
  private readonly _ctx: TwapWorkerContext;
  private readonly _twapOwner: TwapOwner | null | undefined;
  private readonly _settings: AppSettingsWorkerType;
  private readonly _metadata: ExchangeMetadataWorkerType;
  private readonly _sessionKey: string;
  instanceId: string | undefined = undefined;

  private _exchange: Exchange | null = null;
  private _store = new FrameStore();
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
  private _startTimedOut = false;
  private _startAnswer: string | undefined;
  private _startRetries = 0;
  /** Runs once, when a retried start finally brings the account up. */
  onStartRetrySucceeded: (() => void) | undefined;
  private _unsubscribeStore: (() => void) | null = null;
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

  private _handlers: BaseHandler[] = [];
  /** Venue data reaches main as engine frames, not in store payloads. */
  private _forwardsFrames = false;
  /** Main decodes books, candles and fills from the frames itself, so their ops go quiet. */
  private _mainDecodesFrames = false;
  private _sentryReportCooldowns = new Map<string, { lastReportedAt: number; suppressedCount: number }>();

  constructor(
    ctx: TwapWorkerContext,
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
    options = snapshotDelegatedSigningOptions(exchangeName, options);
    const { workerMirror: _workerMirror, ...ownedOptions } = options;
    this._options = ownedOptions;
    this._ctx = ctx;
    this._twapOwner = captureTwapOwner(ctx.getTwapOwner && (() => ctx.getTwapOwner!()));
    this._settings = settings;
    this._metadata = metadata;
    this._sessionKey = sessionKey;
    this.instanceId = instanceId;
    liveExchangeWorkers.add(this);


  }

  /**
   * Helper method to send log messages through the worker messaging system
   */
  private log(level: 'info' | 'warn' | 'error', message: string, context?: Record<string, any>) {
    this._ctx.postMessage({
      op: 'log',
      data: [
        diagnosticText({
          level,
          message,
          context: { exchange: this._exchangeName, account: this._accountName, ...context },
        }),
        level === 'warn' ? 'warning' : level,
      ],
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
            ...historyErrorDiagnostics(error),
          }
        : {
            message: String(error),
            name: 'UnknownError',
            stack: new Error().stack,
            ...historyErrorDiagnostics(error),
          };

    try {
      this._ctx.postMessage(
        redactDiagnosticData({
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
                  tunnel_port: tunnelLanePort(),
                  ...splitExtra.sentryTags,
                  ...workerSentryControl.tags,
                  ...workerErrorProtectionTags(error),
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
        }),
      );
    } catch (e) {
      // Fallback if postMessage fails
      diagnosticConsole.error('[ExchangeWorker] Failed to send error to main thread:', message, error, e);
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
        this._dispatchBufferedMessage(payload);
      } catch (error) {
        this.logError(`Failed to process buffered message ${index + 1}/${bufferCount} ` + String(error), error, {
          payloadOp: payload.op,
          messageIndex: index,
          totalBuffered: bufferCount,
        });
      }
    });
  }

  private _flushBufferedCalls() {
    const calls = this._messageBuffer.filter((payload) => payload.op === 'callMethod');
    this._messageBuffer = this._messageBuffer.filter((payload) => payload.op !== 'callMethod');
    calls.forEach((payload) => this._dispatchBufferedMessage(payload));
  }

  private _dispatchBufferedMessage(payload: StoreWorkerPayload) {
    if (!this._twapOwnerCurrent()) {
      this._rejectUndispatched(payload, 'Account owner changed before this operation was sent; nothing was sent.');
      return;
    }
    this.handlePayload(payload);
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
    this._startTimedOut = false;
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
        throw unsupportedVenueError(`Unknown exchange: ${this._exchangeName}`);
      }
      const Exchange = await loader();
      if (startupToken !== this._startupToken) {
        return false;
      }
      this._exchange = new Exchange(this._exchangeName, this._options, this._store);
      if (this._exchange instanceof RustEngineExchange && (this._twapOwner !== undefined || this._ctx.getTwapOwner)) {
        this._exchange.setTwapOwnerProvider(
          () => captureTwapOwner(() => this._ctx.getTwapOwner?.() ?? null) ?? null,
          this._twapOwner ?? null,
        );
      }
      if (this._recoveryAttempted) this._exchange.setActiveTickerSymbols(this._activeTickerSymbols);
      this._mainDecodesFrames = this.forwardFrames(generation, true);
      // A recovered exchange keeps forwarding where main asked for this account's frames.


      // When privateOnly, disable public websocket connections before start().
      // Public data (tickers, orderbook, OHLCV) is provided by a shared public exchange.
      if (this._options.privateOnly) {
        this._exchange.disablePublicWebsockets();
      }

      this._initializeProcessorsAndHandlers();
      this._setupExchangeEventListeners();

      this.log('info', `Starting exchange in webworker: ${this._exchangeName}`);

      const isRust = this._exchange instanceof RustEngineExchange;
      const startTimeoutMs = isRust ? RUST_START_TIMEOUT_MS : START_TIMEOUT_MS;
      const startedAt = performance.now();
      const startPromise = this._exchange.start().then(() => 'started' as const);
      // Its calls queue under the engine's own bounds; held here they would wait out the whole start.
      if (isRust) this._flushBufferedCalls();
      const timeoutPromise = new Promise<never>((_, reject) => {
        startupTimeout = setTimeout(() => {
          this._startTimedOut = isRust;
          const answered = (this._exchange as { lastReadFailure?: string } | undefined)?.lastReadFailure;
          this._startAnswer = answered === undefined ? undefined : diagnosticText(answered);
          const error = new Error(
            `Exchange start timed out after ${startTimeoutMs / 1000}s${answered ? `; ${answered}` : ''}`,
          );
          Object.assign(error, {
            safeCexSentryTags: {
              ...(this._exchange instanceof RustEngineExchange ? this._exchange.startupDiagnostics : {}),
              startup_elapsed_ms: String(Math.max(0, performance.now() - startedAt)),
              startup_deadline_ms: String(startTimeoutMs),
            },
          });
          reject(error);
        }, startTimeoutMs);
      });

      const result = await Promise.race([startPromise, timeoutPromise, cancelledPromise]);
      if (result === 'cancelled' || startupToken !== this._startupToken) {
        return false;
      }

      if (!this._twapOwnerCurrent()) {
        this.dispose();
        return false;
      }
      if (this._exchange?.isDisposed) return false;

      // The engine's Rust algos run this account's chasers, TWAPs and CLI;
      // start them before the buffered ops that drive them.
      if (this._exchange instanceof RustEngineExchange) this._exchange.startAlgos(this._accountName);
      this._flushBufferedMessages();
      if (startupToken !== this._startupToken || this._exchange?.isDisposed) {
        return false;
      }

      this.sendSnapshot();
      if (this._exchange?.isDisposed) return false;
      this._started = true;
      return true;
    } catch (e) {
      // Command fencing can retire the adapter before startup resolves or rejects.
      if (!this._twapOwnerCurrent()) {
        this.dispose();
        return false;
      }
      if (
        // Only the worker's own stop is quiet. An exchange that disposed itself because its
        // start failed (a refused key) must report it, or nothing downstream can react.
        startupToken !== this._startupToken ||
        this._disposed ||
        // The exchange told the user itself, then stopped (a lost shared feed).
        (this._exchange?.isDisposed &&
          isErrorReported(e) &&
          !(this._exchange instanceof RustEngineExchange && this._exchange.isEngineDead))
      ) {
        diagnosticConsole.warn(`Error starting exchange (disposed, suppressing): ${this._exchangeName}`, e);
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
    if (this._exchange?.isDisposed) {
      return;
    }
    if (this._forwardsFrames && this._exchange instanceof RustEngineExchange) {
      const frame = this._exchange.snapshotFrame();
      if (frame) this._ctx.postMessage({ op: 'frame', data: frame, snapshot: true }, [frame]);
    }

  }

  /**
   * Main reads venue data straight from this account's frames; store payloads
   * carry the rest. Per account: a main-thread engine that is frame-fed asks for
   * it (`forwardFrames`), and one that is not keeps full store payloads.
   */
  private forwardFrames(generation: number, mainDecodesMarketData: boolean) {
    const exchange = this._exchange;
    if (!(exchange instanceof RustEngineExchange)) return false;
    this._forwardsFrames = true;
    // A frame-fed page engine takes account sections only; books and candles
    // still go out as this worker's own listenOrderBook/listenOHLCV ops.
    exchange.framesCarryMarketData = mainDecodesMarketData;
    exchange.onFrame = (frame, snapshot) => {
      if (generation === this._generation && !this._stopped) {
        this._ctx.postMessage({ op: 'frame', data: frame, snapshot: snapshot === true }, [frame]);
      }
    };
    return true;
  }

  /** This realm's tunnel sockets left the 8443 lane for 443 after repeated failed handshakes. */
  reportTunnelLaneFallback(failures: number) {
    const message = 'Tunnel lane fell back to 443';
    this.logError(message, new Error(message), { sentryTags: { tunnel_lane_failures: String(failures) } });
  }

  dispose() {
    if (this._disposed) return;
    liveExchangeWorkers.delete(this);
    this._disposed = true;
    this._stopped = true;
    this._started = false;
    this._recovering = false;
    this._generation++;
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
      // Native pending-command rejections (or acknowledgements already
      // queued) reach their original caller before the task fallback.
      this._exchange?.dispose();
    } finally {
      this._clearRuntimePipeline();
    }
  }

  private _clearRuntimePipeline() {
    this._unsubscribeStore?.();
    this._unsubscribeStore = null;
    this._handlers.forEach((handler) => handler.dispose());
    this._handlers = [];
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
    if (!this._stopped && !this._twapOwnerCurrent()) this.dispose();
    if (this._stopped) {
      this._rejectUndispatched(payload);
      return;
    }
    const isDemand = this._recordPublicDemand(payload);
    const isUiState = payload.op === 'ui:state';
    if (isUiState) this._latestUiState = { ...payload, data: { ...payload.data } };
    if (this._recovering || (this._exchange instanceof RustEngineExchange && this._exchange.isEngineDead)) {
      // Subscription state is safe to retain; trading/query requests are never replayed.
      if (!isDemand && !isUiState) this._rejectUndispatched(payload);
      return;
    }
    if (!this._exchange) {
      // Buffer messages until exchange is initialized
      const MAX_BUFFER_SIZE = 1000;
      if (this._messageBuffer.length < MAX_BUFFER_SIZE) {
        this._messageBuffer.push(payload);
        if (this._requestKey(payload)) {
          setTimeout(() => {
            const at = this._messageBuffer.indexOf(payload);
            if (at < 0) return;
            this._messageBuffer.splice(at, 1);
            this._rejectNeverSent(payload, 'The account did not load in time; nothing was sent. Retry once it loads.');
          }, BUFFERED_REQUEST_WAIT_MS);
        }
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
      case 'sayHi':
        break;
      case 'forwardFrames':
        // Started engines get a baseline at once; one still starting sends its own first.
        if (this.forwardFrames(this._generation, false) && (this._exchange as RustEngineExchange).hasStarted)
          this.sendSnapshot();
        break;
      default:
        for (let i = 0; i < this._handlers.length; i++) {
          this._handlers[i].handle(payload);
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

  /** Never reached the exchange, so a request is `notSent`; a started algorithm says so too. */
  private _rejectUndispatched(payload: StoreWorkerPayload, message?: string) {
    if (this._requestKey(payload)) this._rejectNeverSent(payload, message);
    else this._rejectUnavailablePayload(payload, message);
  }

  private _dropMessageBuffer() {
    const dropped = this._messageBuffer;
    this._messageBuffer = [];
    for (const payload of dropped) {
      this._rejectUndispatched(payload, 'Exchange stopped before this operation was sent; nothing was sent.');
    }
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

  /** TWAPs in the engine's Rust algos died with it: post their final state. */
  private _finishRustTwaps() {
    for (const handler of this._handlers) if (handler instanceof TwapHandler) handler.finishRustTwaps();
  }

  private _invalidateReadiness() {
    this._store.reset();
  }

  private _twapOwnerCurrent(): boolean {
    const owner = this._twapOwner;
    const current = captureTwapOwner(this._ctx.getTwapOwner && (() => this._ctx.getTwapOwner!()));
    if (owner === undefined || owner === null) return current === owner;
    return !!current && current.ownerId === owner.ownerId && current.generation === owner.generation;
  }

  private _handleEngineDead(error: Error) {
    // Startup belongs to store.webworker's !started cleanup. Never race it by
    // scheduling a replacement before that initial start has returned.
    if (this._stopped) return;
    // BaseExchange.dispose resets its store. Preserve the last native freshness
    // counters before releasing it, so the replacement cannot rewind them.
    this._recoveryUpdateIds = { ...this._store.updateIds };
    if (!this._started || this._recovering) {
      // An op that beat startup can have started a Rust TWAP already.
      this._finishRustTwaps();
      this._invalidateReadiness();
      return;
    }
    const ownerCurrent = this._twapOwnerCurrent();
    const cannotRecover = !ownerCurrent || this._recoveryAttempted || this._options.publicFeed?.mode === 'shared';
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
    // Post the dead engine's TWAP state while the handler context is current,
    // so the main thread that persists and resumes TWAPs does not restore them.
    this._finishRustTwaps();
    this._generation++;
    this._clearRuntimePipeline();
    this._exchange?.dispose();
    this._dropMessageBuffer();
    this._invalidateReadiness();
    this.log('warn', 'Rust engine stopped; interrupted writes may have an unknown outcome and will not be replayed.', {
      reason: error.message,
    });
    if (cannotRecover) {
      this._stopped = true;
      this._recovering = false;
      const message =
        !ownerCurrent
          ? 'TWAP owner changed; reinitialize the account worker.'
          : this._options.publicFeed?.mode === 'shared'
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

  /**
   * A Rust start that ran out of time starts over on this worker, keeping its
   * public demand, and answers what it held. False leaves the failure final.
   */
  retryTimedOutStart(): boolean {
    const delay = START_RETRY_DELAYS_MS[this._startRetries];
    // A shared port needs a fresh provider attachment, as after an engine failure.
    if (!this._startTimedOut || this._stopped || delay === undefined || this._options.publicFeed?.mode === 'shared') {
      if (this._startTimedOut && !this._stopped) {
        const message = `${this._exchangeName} did not finish loading after ${this._startRetries + 1} attempts. Check your connection and reload the account.`;
        // Forced: a start that never connected would otherwise fail the page's connected gate.
        this._ctx.postMessage({ op: 'error', data: [message, { forceToast: true }] });
      }
      return false;
    }
    this._startTimedOut = false;
    this._startRetries++;
    this._recoveryUpdateIds = { ...this._store.updateIds };
    this._recovering = true;
    this._deferInterruptedRequests({
      message: 'Exchange start timed out; interrupted operation outcomes are unknown and were not replayed.',
      name: 'EngineDisposedError',
      code: 'ENGINE_DISPOSED',
    });
    this._finishRustTwaps();
    this._generation++;
    this._clearRuntimePipeline();
    this._exchange?.dispose();
    this._dropMessageBuffer();
    this._invalidateReadiness();
    this._ctx.postMessage({
      op: 'warning',
      data: [
        this._startAnswer
          ? `${this._exchangeName} did not load this account; retrying in ${delay / 1000}s. ${this._startAnswer}`
          : `${this._exchangeName} is still loading on a slow connection; retrying in ${delay / 1000}s.`,
        { forceToast: true },
      ],
    });
    const token = this._startupToken;
    this._recoveryTimer = setTimeout(() => {
      this._recoveryTimer = null;
      void this._recoverEngine(token).catch((failure: unknown) => {
        if (token !== this._startupToken || this._stopped) return;
        this.dispose();
        this.logError('Exchange start retry failed; reinitialize the account.', failure);
      });
    }, delay);
    return true;
  }

  private async _recoverEngine(token: number) {
    if (token !== this._startupToken || this._stopped) return;
    if (!this._twapOwnerCurrent()) {
      this.dispose();
      this._ctx.postMessage({ op: 'warning', data: ['TWAP owner changed; reinitialize the account worker.'] });
      return;
    }
    this._store = new FrameStore();
    const started = await this.start();
    if (token !== this._startupToken || this._stopped) return;
    if (!this._twapOwnerCurrent()) {
      this.dispose();
      this._ctx.postMessage({ op: 'warning', data: ['TWAP owner changed; reinitialize the account worker.'] });
      return;
    }
    if (!started && this.retryTimedOutStart()) return;
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
    if (this._stopped || this._exchange?.isDisposed) return;
    this._ctx.postMessage({ op: 'ws-reconnected' });
    const succeeded = this.onStartRetrySucceeded;
    this.onStartRetrySucceeded = undefined;
    succeeded?.();
  }

  private _initializeProcessorsAndHandlers() {
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
          : property === 'getTwapOwner' && target.getTwapOwner
            ? () => (generation === this._generation && !this._stopped ? (target.getTwapOwner?.() ?? null) : null)
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
      'ws-diagnostic',
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

    this._exchange!.on('deactivate', (refusal?: string) => {
      if (!isCurrent()) return;
      this.log('info', `Exchange emitted deactivate event: ${this._exchangeName}`);
      // Dispose through the worker so any in-flight start() gets cancelled.
      this.dispose();
      this._exchange = null;
      this._clearRuntimePipeline();
      // Notify main thread of deactivation
      this._ctx.postMessage({
        op: 'deactivate-status',
        data: true,
        refusal: diagnosticText(refusal ?? 'The exchange refused this key.'),
      });
    });
  }
}
