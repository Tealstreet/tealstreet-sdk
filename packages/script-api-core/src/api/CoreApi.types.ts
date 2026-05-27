/**
 * CoreApi — the portable, runtime-agnostic surface every script sees.
 * See docs/custom-scripts/SPEC.md §2.1.
 *
 * Phase 1 ships the FULL type contract (read-side getters + exchange-actions
 * + io + storage + utils + constants + meta). Only the read-side has
 * runtime impls in Phase 1; actions/io/storage are types-only until later
 * phases (see SPEC §11).
 *
 * Async-first, POJO-only — every method returns Promise<T> even when the
 * v1 impl is synchronous-under-the-hood, and every input/output must
 * survive structuredClone() per SPEC §3.
 */
import type {
  Balance,
  Candle,
  ExchangeName,
  Market,
  MarginType,
  Order,
  OrderBook,
  OrderSide,
  OrderStatus,
  OrderStopTriggerType,
  OrderTimeInForce,
  OrderType,
  Position,
  PositionMode,
  PositionSide,
  Ticker,
  Timeframe,
  UpdateOrderOpts,
} from '@tealstreet/safe-cex/src/types';

import type { Capabilities, TargetName } from './capabilities.types';

// ───────────────────────────────────────────────────────────────────────────
// Re-exports
// ───────────────────────────────────────────────────────────────────────────

export type {
  Balance,
  Candle,
  ExchangeName,
  Market,
  Order,
  OrderBook,
  Position,
  Ticker,
  Timeframe,
  UpdateOrderOpts,
} from '@tealstreet/safe-cex/src/types';

export {
  MarginType,
  OrderSide,
  OrderStatus,
  OrderStopTriggerType,
  OrderTimeInForce,
  OrderType,
  PositionMode,
  PositionSide,
} from '@tealstreet/safe-cex/src/types';

// ───────────────────────────────────────────────────────────────────────────
// NEW POJOs defined in core (all structured-clone-safe)
// ───────────────────────────────────────────────────────────────────────────

/**
 * Minimal account descriptor surfaced to scripts. Never includes API keys
 * or other secret material — the script-api-core boundary intentionally
 * hides credential-shaped fields.
 */
export interface AccountInfo {
  readonly name: string;
  readonly exchange: ExchangeName;
  readonly isStreaming: boolean;
}

/**
 * Cross-account aggregated balance summary. All values are denominated in
 * the host's configured display currency (USD by default).
 */
export interface CombinedBalance {
  readonly totalEquity: number;
  readonly totalAvailable: number;
  readonly totalUsed: number;
  readonly totalUpnl: number;
  readonly longExposure: number;
  readonly shortExposure: number;
  readonly currency: string;
}

/**
 * Historical (closed) position record. Flattened from the web's
 * positionsHistory selectors. Pure data — no methods.
 */
export interface HistoricalPosition {
  readonly account: string;
  readonly symbol: string;
  readonly side: 'long' | 'short';
  readonly contracts: number;
  readonly entryPrice: number;
  readonly unrealizedPnl: number;
  readonly fees: number;
  /** Epoch ms when the historical record was last updated. */
  readonly timestamp: number;
}

/** SPEC §7.4 — print severity. */
export type PrintKind = 'print' | 'success' | 'info' | 'warn' | 'error';

/** SPEC §2.7 — formatPnl options (text-only). */
export interface FormatPnlOpts {
  readonly currency?: string;
  readonly precision?: number;
  readonly signed?: boolean;
  readonly abbreviated?: boolean;
}

/** SPEC §2.7 — runCommand return shape. */
export interface RunCommandResult {
  readonly ok: boolean;
  readonly capturedIds: readonly string[];
  readonly output: ReadonlyArray<{ readonly text: string; readonly kind: PrintKind }>;
  readonly error?: string;
}

/**
 * Output shape for the future `utils.calculateOrdersRisk` helper — risk
 * amount + breakeven distance. The method itself is omitted from the
 * v1.0.0 `CoreUtils` surface (see `src/utils/index.ts`); this type
 * stays exported so the impl can land without a types-only breaking
 * change.
 */
export interface RiskResult {
  /** Quote-currency risk if the worst-case stop fills. */
  readonly riskAmount: number;
  /** Risk as a fraction of position notional. */
  readonly riskPercent: number;
  /** Computed breakeven price for the order set vs. the open position. */
  readonly breakeven: number;
  /** Absolute price distance from current to breakeven. */
  readonly distance: number;
}

/** Output of utils.calculateReturns — ROE / ROI on the open position. */
export interface ReturnsResult {
  readonly roe: number;
  readonly roi: number;
}

// ───────────────────────────────────────────────────────────────────────────
// Action surface — types only in Phase 1, impls land Phase 2
// ───────────────────────────────────────────────────────────────────────────

export interface PlaceOrderOptions {
  readonly symbol: string;
  readonly side: OrderSide;
  readonly type: OrderType;
  readonly amount: number;
  readonly price?: number;
  readonly reduceOnly?: boolean;
  readonly timeInForce?: OrderTimeInForce;
  readonly stopLoss?: number;
  readonly takeProfit?: number;
  readonly slTriggerType?: OrderStopTriggerType;
  readonly tpTriggerType?: OrderStopTriggerType;
  readonly account?: string;
}

export interface CancelFilter {
  readonly symbol?: string;
  readonly side?: OrderSide;
  readonly reduceOnly?: boolean;
}

export interface ClosePositionOpts {
  readonly symbol: string;
  readonly side?: PositionSide;
  readonly amount?: number;
  readonly account?: string;
}

// ───────────────────────────────────────────────────────────────────────────
// Frozen string-literal enums on api.constants.*
// ───────────────────────────────────────────────────────────────────────────

/**
 * The full ExchangeName union as a frozen literal-map. ExchangeName itself
 * is a TS union (not an enum), so api.constants.ExchangeName ships a
 * plain frozen object whose keys mirror the union members.
 * Kept in step with @tealstreet/safe-cex/src/types ExchangeName.
 */
export type ExchangeNameMap = Readonly<Record<ExchangeName, ExchangeName>>;

/** The bundle exposed as api.constants.* — frozen at construction. */
export interface CoreConstants {
  readonly ExchangeName: ExchangeNameMap;
  readonly OrderSide: typeof OrderSide;
  readonly OrderType: typeof OrderType;
  readonly OrderStatus: typeof OrderStatus;
  readonly OrderTimeInForce: typeof OrderTimeInForce;
  readonly OrderStopTriggerType: typeof OrderStopTriggerType;
  readonly PositionSide: typeof PositionSide;
  readonly PositionMode: typeof PositionMode;
  readonly MarginType: typeof MarginType;
}

// ───────────────────────────────────────────────────────────────────────────
// utils.* — pure helpers exposed under api.utils
// ───────────────────────────────────────────────────────────────────────────

export interface CoreUtils {
  sleep(ms: number): Promise<void>;
  uuid(): string;
  adjust(value: number, step: number): number;

  formatPnl(value: number, opts?: FormatPnlOpts): string;
  // `calculateOrdersRisk` is omitted from the v1.0.0 surface — see
  // `src/utils/index.ts` for why. Re-add once shape discrepancy is resolved.
  calculateReturns(
    position: Pick<Position, 'contracts' | 'leverage' | 'unrealizedPnl' | 'entryPrice'>,
    overrideUPNL?: number,
  ): ReturnsResult;
  getMarginCurrency(market: Market): string;
  convertSymbol(symbol: string, from: ExchangeName, to: ExchangeName): string;

  getTickerPredictedBestBid(ticker: Ticker): number;
  getTickerPredictedBestAsk(ticker: Ticker): number;
  getBBOMid(ticker: Ticker): number;
  getNearTouchPrice(side: OrderSide, ticker: Ticker): number;
  getFarTouchPrice(side: OrderSide, ticker: Ticker): number;

  getCandleStartTime(timeframe: Timeframe, epoch: number): number;
  getCandleEndTime(timeframe: Timeframe, epoch: number): number;
  getNumCandlesInRange(timeframe: Timeframe, from: number, to: number): number;
  normalizeEpochToSeconds(epoch: number): number;
}

// ───────────────────────────────────────────────────────────────────────────
// meta — script identity & runtime context per SPEC §2.7
// ───────────────────────────────────────────────────────────────────────────

export type InvokedFrom = 'cli' | 'hotkey' | 'webhook' | 'inline';

export interface CoreMeta {
  /** Stable per-script UUID — used for storage namespacing. Never changes on rename. */
  readonly scriptId: string;
  /** User-set; may change. */
  readonly scriptName: string;
  readonly invokedFrom: InvokedFrom;
  readonly targets: readonly TargetName[];
  /** Full frozen capability struct — see SPEC §2.7. */
  readonly capabilities: Readonly<Capabilities>;
  /** Populated when invoked via `loop <variant>: myScript`. See SPEC §7.5. */
  readonly loopSymbol?: string;
}

// ───────────────────────────────────────────────────────────────────────────
// io / storage / exchange — types only in Phase 1
// ───────────────────────────────────────────────────────────────────────────

export interface CoreIo {
  /** Emit a line to the host's output sink (REPL on standalone, console panel on web). */
  print(text: string, kind?: PrintKind): void;
  /**
   * Capture-syntax writeback (see SPEC §7.4). The arg shapes:
   *   string                          → captured as-is
   *   string[]                        → comma-joined
   *   number / boolean                → stringified
   *   object / nested arr / anything  → JSON-stringified, sibling .json set
   */
  return(value: unknown): void;
}

export interface CoreStorage {
  get<T>(key: string): Promise<T | undefined>;
  set<T>(key: string, value: T): Promise<void>;
  delete(key: string): Promise<void>;
  list(): Promise<readonly string[]>;
}

export interface CoreExchange {
  placeOrder(opts: PlaceOrderOptions): Promise<{ orderIds: readonly string[] }>;
  placeOrders(opts: readonly PlaceOrderOptions[]): Promise<{ orderIds: readonly string[] }>;
  cancelOrder(orderId: string, opts?: { account?: string; symbol?: string }): Promise<void>;
  cancelOrders(filter: CancelFilter, opts?: { account?: string }): Promise<{ canceled: number }>;
  updateOrder(orderId: string, patch: UpdateOrderOpts, opts?: { account?: string }): Promise<void>;
  closePosition(opts: ClosePositionOpts): Promise<{ orderIds: readonly string[] }>;
  setLeverage(symbol: string, leverage: number, opts?: { account?: string }): Promise<void>;
  setPositionMode(mode: PositionMode, opts?: { account?: string }): Promise<void>;
  setMarginType(symbol: string, marginType: MarginType, opts?: { account?: string }): Promise<void>;
  /** Escape hatch — runs any allowlisted REPL verb. See SPEC §4.2. */
  runCommand(command: string, opts?: { account?: string; symbol?: string }): Promise<RunCommandResult>;
}

// ───────────────────────────────────────────────────────────────────────────
// CoreApi — the contract every script sees as `api`
// ───────────────────────────────────────────────────────────────────────────

/** Optional account-scoping accepted by most read-side getters. */
export interface AccountOpts {
  readonly account?: string;
}

export interface CandleOpts extends AccountOpts {
  readonly from?: number;
  readonly to?: number;
  readonly limit?: number;
}

export interface OrderbookOpts extends AccountOpts {
  readonly depth?: number;
}

export interface PositionOpts extends AccountOpts {
  readonly symbol?: string;
}

export interface OrderQueryOpts extends AccountOpts {
  readonly symbol?: string;
}

export interface CombinedBalanceOpts {
  /** When omitted, all streaming accounts are aggregated. */
  readonly accounts?: readonly string[];
}

export interface HistoricalPositionQuery {
  readonly account: string;
  readonly symbol: string;
}

export interface CoreApi {
  // ── Snapshot getters ───────────────────────────────────────────────────
  getActiveAccount(): Promise<string>;
  getActiveAccounts(): Promise<readonly string[]>;
  getActiveSymbol(): Promise<string>;
  getActiveExchange(): Promise<ExchangeName>;
  getAccounts(): Promise<readonly AccountInfo[]>;

  getMarkets(opts?: AccountOpts): Promise<readonly Market[]>;
  getMarket(symbol: string, opts?: AccountOpts): Promise<Market | undefined>;

  getTicker(symbol: string, opts?: AccountOpts): Promise<Ticker | undefined>;
  getTickers(symbols: readonly string[], opts?: AccountOpts): Promise<readonly Ticker[]>;

  getPositions(opts?: PositionOpts): Promise<readonly Position[]>;
  getHistoricalPosition(query: HistoricalPositionQuery): Promise<HistoricalPosition | undefined>;

  getOrders(opts?: OrderQueryOpts): Promise<readonly Order[]>;

  getBalance(opts?: AccountOpts): Promise<Balance | undefined>;
  getCombinedBalance(opts?: CombinedBalanceOpts): Promise<CombinedBalance>;

  getOrderbook(symbol: string, opts?: OrderbookOpts): Promise<OrderBook | undefined>;
  getCandles(symbol: string, timeframe: Timeframe, opts?: CandleOpts): Promise<readonly Candle[]>;

  // ── Action surface (types only in Phase 1; impls Phase 2) ──────────────
  readonly exchange: CoreExchange;

  // ── I/O (types only in Phase 1; impl wired Phase 2) ────────────────────
  readonly io: CoreIo;

  // ── Persistence (types only in Phase 1; impl Phase 3) ──────────────────
  readonly storage: CoreStorage;

  // ── Pure helpers ───────────────────────────────────────────────────────
  readonly utils: CoreUtils;

  // ── Constants ──────────────────────────────────────────────────────────
  readonly constants: CoreConstants;

  // ── Identity & metadata ────────────────────────────────────────────────
  /** CORE_API_VERSION at the time the api object was constructed. */
  readonly version: string;
  readonly meta: CoreMeta;
}
