import { Exchange } from '../../exchanges/base';
import { ExchangeName } from '../../types';
import { AppSettingsWorkerType } from '../AppSettingsWorkerType';
import { ExchangeMetadataWorkerType } from '../ExchangeMetadataWorkerType';
import { StoreWorkerPayload } from '../StoreWorkerPayload';

export type RustAlgoExchange = {
  runsRustAlgos: boolean;
  routeAlgoOp(accountName: string, op: string, rust: () => void, ts: () => void): boolean;
  algoOp(op: string, data: unknown): void;
};

/** An exchange whose engine may run this account's algos (`RustEngineExchange`). */
export const rustAlgoExchange = (exchange: unknown): RustAlgoExchange | undefined =>
  typeof (exchange as Partial<RustAlgoExchange> | undefined)?.routeAlgoOp === 'function'
    ? (exchange as RustAlgoExchange)
    : undefined;

const PENDING_TIMEOUT_MS = 30_000;
const PENDING_POLL_MS = 500;
const PENDING_TIMEOUT_REPORT_COOLDOWN_MS = 5 * 60_000;

export abstract class BaseHandler {
  protected _accountName: string;
  protected _exchangeName: ExchangeName;
  protected _exchange: Exchange;
  protected _ctx: Worker;
  protected _settings: AppSettingsWorkerType;
  protected _metadata: ExchangeMetadataWorkerType;

  // Buffer for messages that arrive before markets are loaded
  private _pendingUntilMarkets: StoreWorkerPayload[] = [];
  private _pendingTimeout: ReturnType<typeof setTimeout> | null = null;
  private _pendingCheckTimer: ReturnType<typeof setTimeout> | null = null;
  private _firstPendingAt: number | null = null;
  private _lastPendingTimeoutReportAt: number | null = null;
  private _suppressedPendingTimeoutReports = 0;

  constructor(
    accountName: string,
    exchangeName: ExchangeName,
    exchange: Exchange,
    ctx: Worker,
    settings: AppSettingsWorkerType,
    metadata: ExchangeMetadataWorkerType,
  ) {
    this._accountName = accountName;
    this._exchangeName = exchangeName;
    this._exchange = exchange;
    this._ctx = ctx;
    this._settings = settings;
    this._metadata = metadata;
  }

  abstract handle(payload: StoreWorkerPayload): void;

  /**
   * A Rust-engine account built with algos runs chaser, TWAP and exec ops in its
   * engine (`rust`, by default `RustEngineExchange.algoOp`), never in the TS
   * algos. True when sent there, or held until the engine host starts: the
   * exchange then replays it here, in Rust or through this handler's TS path.
   */
  protected runInRustAlgos(payload: StoreWorkerPayload, rust?: () => void): boolean {
    const exchange = rustAlgoExchange(this._exchange);
    if (!exchange) return false;
    return exchange.routeAlgoOp(
      this._accountName,
      payload.op,
      rust ?? (() => exchange.algoOp(payload.op, payload.data)),
      () => this.handle(payload),
    );
  }

  /**
   * Buffer a payload until markets are loaded. Returns true if buffered,
   * false if markets are already loaded (caller should proceed normally).
   */
  protected bufferUntilMarketsLoaded(payload: StoreWorkerPayload): boolean {
    if (this._exchange.store.markets.length > 0) {
      this._resetPendingTimeoutReportEpisode();
      return false;
    }

    this._firstPendingAt ??= Date.now();
    this._pendingUntilMarkets.push(payload);

    this._schedulePendingMarketsCheck();

    // Timeout: drop pending and notify client if markets never load
    if (!this._pendingTimeout) {
      this._pendingTimeout = setTimeout(() => {
        if (this._pendingUntilMarkets.length > 0) {
          const now = Date.now();
          const shouldReport =
            this._lastPendingTimeoutReportAt === null ||
            now - this._lastPendingTimeoutReportAt >= PENDING_TIMEOUT_REPORT_COOLDOWN_MS;

          if (shouldReport) {
            const msg = `Markets still not loaded after ${PENDING_TIMEOUT_MS / 1000}s, dropping ${this._pendingUntilMarkets.length} pending subscriptions`;
            console.error(`[${this.constructor.name}] ${msg}`);
            const error = new Error(msg);
            error.name = 'MarketsNotLoadedError';
            this._ctx.postMessage({
              op: 'error',
              data: [
                msg,
                {
                  sendToSentry: true,
                  sentryData: {
                    error,
                    tags: {
                      exchange: this._exchange.name,
                      module: 'BaseHandler',
                      worker_handler: this.constructor.name,
                      worker_error_family: 'markets_not_loaded',
                    },
                    extra: {
                      ...this._getPendingTelemetry(),
                      suppressedPendingTimeoutReports: this._suppressedPendingTimeoutReports,
                    },
                  },
                },
              ],
            });
            this._lastPendingTimeoutReportAt = now;
            this._suppressedPendingTimeoutReports = 0;
          } else {
            this._suppressedPendingTimeoutReports += 1;
          }
          this._pendingUntilMarkets = [];
        }
        this._clearPendingTimers();
      }, PENDING_TIMEOUT_MS);
    }
    return true;
  }

  private _schedulePendingMarketsCheck(): void {
    if (this._pendingCheckTimer || this._pendingUntilMarkets.length === 0) return;
    this._pendingCheckTimer = setTimeout(() => {
      this._pendingCheckTimer = null;
      if (this._pendingUntilMarkets.length === 0) return;
      if (this._exchange.store.markets.length > 0) {
        this._flushPending();
        return;
      }
      this._schedulePendingMarketsCheck();
    }, PENDING_POLL_MS);
  }

  private _flushPending(): void {
    this._clearPendingTimers();
    this._resetPendingTimeoutReportEpisode();
    const toReplay = [...this._pendingUntilMarkets];
    this._pendingUntilMarkets = [];
    for (const p of toReplay) this.handle(p);
  }

  private _clearPendingTimers(): void {
    if (this._pendingTimeout) {
      clearTimeout(this._pendingTimeout);
      this._pendingTimeout = null;
    }
    if (this._pendingCheckTimer) {
      clearTimeout(this._pendingCheckTimer);
      this._pendingCheckTimer = null;
    }
    this._firstPendingAt = null;
  }

  private _getPendingTelemetry(): Record<string, unknown> {
    const pendingOpCounts: Record<string, number> = {};
    for (const payload of this._pendingUntilMarkets) {
      pendingOpCounts[payload.op] = (pendingOpCounts[payload.op] ?? 0) + 1;
    }

    const socketHealth = this._exchange.store.socketHealth;
    return {
      marketCount: this._exchange.store.markets.length,
      loadedMarkets: this._exchange.store.loaded?.markets === true,
      pendingCount: this._pendingUntilMarkets.length,
      pendingAgeMs: this._firstPendingAt === null ? 0 : Date.now() - this._firstPendingAt,
      pendingOpCounts,
      exchangeOptions: {
        testnet: this._exchange.options?.testnet,
        publicDataOnly: this._exchange.options?.publicDataOnly,
      },
      socketHealth: {
        publicInitialized: socketHealth?.publicInitialized,
        privateInitialized: socketHealth?.privateInitialized,
        publicHealthy: socketHealth?.publicHealthy,
        privateHealthy: socketHealth?.privateHealthy,
        publicCloseCode: socketHealth?.publicCloseCode,
        privateCloseCode: socketHealth?.privateCloseCode,
        publicCloseStreak: socketHealth?.publicCloseStreak,
        privateCloseStreak: socketHealth?.privateCloseStreak,
        publicReconnectPaused: socketHealth?.publicReconnectPaused,
        privateReconnectPaused: socketHealth?.privateReconnectPaused,
      },
    };
  }

  dispose() {
    this._clearPendingTimers();
    this._resetPendingTimeoutReportEpisode();
    this._pendingUntilMarkets = [];
  }

  private _resetPendingTimeoutReportEpisode(): void {
    this._lastPendingTimeoutReportAt = null;
    this._suppressedPendingTimeoutReports = 0;
  }
}
