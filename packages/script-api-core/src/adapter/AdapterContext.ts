/**
 * CoreAdapter — what every host (standalone CLI, web, future mobile) implements
 * to mount the script kernel. See SPEC §11 Phase 1 + Phase 2 + §9.
 *
 * Two contract rules:
 *   1. The dispatcher resolves accounts (SPEC §2.6) before calling the
 *      adapter, so every method takes already-resolved `account`s.
 *   2. The dispatcher runs capability gates (SPEC §4) before calling the
 *      adapter; adapters never re-check. Belt-and-suspenders gates inside
 *      the adapter would conflate "denied" with "couldn't dispatch."
 *
 * Returns are `MaybePromise<T>` so adapters can stay sync where convenient
 * (read paths often read straight from a store map); the proxy always
 * awaits, so the script-facing API is uniformly async.
 *
 * Phase 2 adds:
 *   - The full `exchange.*` write surface (Step 3 / Step 4).
 *   - `print(text, kind)` — host-routed output sink.
 *   - `runCommand(command, opts)` — host-routed REPL escape hatch.
 * `signer` stays `unknown`; Phase 8 swaps in `PostMessageRemoteSigner` per
 * SPEC §8.3.
 */
import type {
  Balance,
  Candle,
  ExchangeName,
  MarginType,
  Market,
  Order,
  OrderBook,
  Position,
  PositionMode,
  PositionSide,
  Ticker,
  Timeframe,
} from '@tealstreet/safe-cex/src/types';

import type {
  AccountInfo,
  CancelFilter,
  ClosePositionOpts,
  CombinedBalance,
  HistoricalPosition,
  PlaceOrderOptions,
  PrintKind,
  RunCommandResult,
  UpdateOrderPatch,
} from '../api/CoreApi.types';

export type MaybePromise<T> = T | Promise<T>;

export interface CoreAdapter {
  // ── Ambient state ──────────────────────────────────────────────────────
  getActiveAccount(): MaybePromise<string>;
  /** Streaming-account names only. */
  getActiveAccounts(): MaybePromise<readonly string[]>;
  getActiveSymbol(): MaybePromise<string>;
  getActiveExchange(): MaybePromise<ExchangeName>;
  getAccounts(): MaybePromise<readonly AccountInfo[]>;

  // ── Per-account reads (account is already-resolved) ────────────────────
  getMarkets(account: string): MaybePromise<readonly Market[]>;
  getMarket(symbol: string, account: string): MaybePromise<Market | undefined>;

  getTicker(symbol: string, account: string): MaybePromise<Ticker | undefined>;
  getTickers(symbols: readonly string[], account: string): MaybePromise<readonly Ticker[]>;

  getPositions(opts: { account: string; symbol?: string }): MaybePromise<readonly Position[]>;
  getHistoricalPosition(opts: { account: string; symbol: string }): MaybePromise<HistoricalPosition | undefined>;

  getOrders(opts: { account: string; symbol?: string }): MaybePromise<readonly Order[]>;

  getBalance(account: string): MaybePromise<Balance | undefined>;
  /** When `accounts` is omitted, host aggregates all streaming accounts. */
  getCombinedBalance(opts: { accounts?: readonly string[] }): MaybePromise<CombinedBalance>;

  getOrderbook(symbol: string, opts: { account: string; depth?: number }): MaybePromise<OrderBook | undefined>;

  getCandles(
    symbol: string,
    timeframe: Timeframe,
    opts: { account: string; from?: number; to?: number; limit?: number },
  ): MaybePromise<readonly Candle[]>;

  // ── Write surface (account is already-resolved) ────────────────────────
  /**
   * Place a single order. `account` is the dispatcher-resolved account name.
   * `opts.account` is dropped from `PlaceOrderOptions` here because the
   * dispatcher already normalized it; carrying it would create two sources
   * of truth.
   */
  placeOrder(opts: Omit<PlaceOrderOptions, 'account'>, account: string): MaybePromise<{ orderIds: readonly string[] }>;
  placeOrders(
    opts: ReadonlyArray<Omit<PlaceOrderOptions, 'account'>>,
    account: string,
  ): MaybePromise<{ orderIds: readonly string[] }>;
  cancelOrder(orderId: string, opts: { account: string; symbol?: string }): MaybePromise<void>;
  cancelOrders(filter: CancelFilter, account: string): MaybePromise<{ canceled: number }>;
  updateOrder(orderId: string, patch: UpdateOrderPatch, account: string): MaybePromise<void>;
  closePosition(
    opts: Omit<ClosePositionOpts, 'account'>,
    account: string,
  ): MaybePromise<{ orderIds: readonly string[] }>;
  /** `side` names one leg of a hedge-mode position. Hosts whose venue has no
   *  per-side leverage ignore it; dropping it from the contract instead would
   *  silently set the wrong leg on the venues that do. */
  setLeverage(symbol: string, leverage: number, account: string, side?: PositionSide): MaybePromise<void>;
  setPositionMode(mode: PositionMode, account: string): MaybePromise<void>;
  setMarginType(symbol: string, marginType: MarginType, account: string): MaybePromise<void>;

  /**
   * Execute one already-allowlisted REPL verb. The dispatcher has already
   * validated the verb against `capabilities.runCommand.allowedVerbs` and
   * tracked recursion depth before this call lands.
   */
  runCommand(
    command: string,
    opts: { account: string; symbol?: string; depth: number },
  ): MaybePromise<RunCommandResult>;

  // ── Output sink ────────────────────────────────────────────────────────
  /**
   * Emit one line. The standalone CLI maps `kind` through chalk; the web
   * inline-invocation path drives the in-app console panel. Hosts that
   * also want to capture for later (e.g. webhook fire mirror) wrap their
   * own buffer around the call.
   */
  print(text: string, kind: PrintKind): MaybePromise<void>;

  // ── Reserved for later phases ──────────────────────────────────────────
  /** Phase 8 swaps in PostMessageRemoteSigner per SPEC §8.3. */
  readonly signer: unknown;
}
