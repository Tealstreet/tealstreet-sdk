import type {
  Balance,
  Candle,
  ExchangeName,
  Market,
  Order,
  OrderBook,
  PlaceOrderOpts,
  PositionSide,
  Ticker,
  Timeframe,
  UpdateOrderOpts,
} from '@tealstreet/safe-cex/src/types';
import type { Store } from '@tealstreet/safe-cex/src/store/store.interface';
import type { CommandOutcome } from '@tealstreet/safe-cex/src/commandOutcome';

import type { HistoricalPosition, PrintKind, RunCommandResult } from '../api/CoreApi.types';
import type { MaybePromise } from './AdapterContext';

/**
 * What a HOST supplies so `createCoreAdapter` can build a `CoreAdapter` for
 * it. This is the whole of the runtime-specific surface — the spoke — and it
 * is deliberately small.
 *
 * ## Why this exists
 *
 * There used to be three hand-rolled `CoreAdapter` implementations: web (562
 * lines), CLI (485) and mobile (388). They shared a contract and nothing else,
 * so they drifted in ways nobody was comparing: the CLI applied no rounding
 * and no minimum-size validation, mobile threw on every write, and only web
 * ran the reduce-only clamp. A contract that three hosts satisfy differently
 * is a contract in name only.
 *
 * Mobile had already proved the fix: `MobileCoreAdapter` took an injected
 * controller interface rather than importing a store. This is that idea
 * finished — the BODY is written once against these bindings, and each host
 * provides only what genuinely differs.
 *
 * ## What belongs here, and what does not
 *
 * A binding earns its place by being something a host can only answer for
 * itself: where the exchange object lives, where the store lives, how to
 * reach an orderbook subscription, where to print. Anything computable FROM
 * those — filtering orders by symbol, summing a combined balance, rounding a
 * size — belongs in the body, not here, or the drift comes straight back.
 */

/**
 * The slice of a live safe-cex exchange the body touches. Structural rather
 * than `BaseExchange`, because the CLI reaches its exchanges through
 * `ExchangeManager` and mobile through a bridge; none of them hands over the
 * same class, and the body only ever calls these members.
 */
export interface HostExchange {
  readonly name?: ExchangeName;
  readonly store?: Store;
  readonly fetchOHLCV?: (opts: {
    readonly symbol: string;
    readonly interval: Timeframe;
    readonly from?: number;
    readonly to?: number;
    readonly limit?: number;
  }) => Promise<readonly Candle[]>;
  readonly placeOrder?: (opts: PlaceOrderOpts) => Promise<string[]>;
  readonly placeOrders?: (opts: PlaceOrderOpts[]) => Promise<string[]>;
  readonly cancelOrders?: (orders: Order[]) => Promise<void>;
  readonly updateOrder?: (opts: UpdateOrderOpts) => Promise<unknown>;
  readonly setLeverage?: (symbol: string, leverage: number, side?: PositionSide) => Promise<void>;
  readonly changePositionMode?: (hedged: boolean, symbol?: string) => Promise<void>;
  /** safe-cex's real shape: `(isolated, symbol?)`. NOT `(symbol, marginType)` —
   *  getting that backwards passes a symbol where a boolean is read, which
   *  coerces truthy and sets isolated margin on every call. */
  readonly changeMarginType?: (isolated: boolean, symbol?: string) => Promise<void>;
}

export interface HostAccount {
  readonly name: string;
  readonly exchange?: ExchangeName;
}

/** What the write observer is told. One shape for every verb, so a host that
 *  logs writes does not need a branch per method. */
export interface HostWriteEvent {
  readonly method: string;
  readonly account: string;
  readonly summary: string;
  readonly ok: boolean;
  readonly orderIds?: readonly string[];
  readonly error?: string;
  /** SDK-reported IDs on a failed batch; an `unknown` outcome remains uncertain. */
  readonly partialOrderIds?: readonly string[];
  readonly outcome?: CommandOutcome;
  readonly partialOrderCount?: number;
  readonly totalOrderCount?: number;
}

export interface HostBindings {
  // ── Where the host keeps things ────────────────────────────────────────
  getExchange(account: string): HostExchange | undefined;
  getStore(account: string): Store | undefined;

  // ── Ambient session state ──────────────────────────────────────────────
  readActiveAccount(): string;
  readActiveSymbol(): string;
  /** Accounts with a live connection, for `isStreaming` and the default set
   *  `getCombinedBalance` sums when the caller names none. */
  readStreamingAccounts(): readonly string[];
  readAllAccounts(): readonly HostAccount[];

  // ── Reads the store cannot answer ──────────────────────────────────────
  /**
   * Orderbooks are NOT a store read on any host. Web and CLI both maintain a
   * subscription-backed cache; mobile hands back a snapshot from a listener it
   * already owns. The body must not pick one of those for everybody.
   */
  readOrderbook(symbol: string, opts: { account: string; depth?: number }): MaybePromise<OrderBook | undefined>;
  /** Absent on hosts that do not carry position history — the body reports it
   *  as unsupported rather than inventing an empty answer. */
  readHistoricalPosition?(opts: { account: string; symbol: string }): MaybePromise<HistoricalPosition | undefined>;
  /** Only for hosts that source candles somewhere other than the exchange's
   *  own `fetchOHLCV`. */
  readCandles?(
    symbol: string,
    timeframe: Timeframe,
    opts: { account: string; from?: number; to?: number; limit?: number },
  ): MaybePromise<readonly Candle[]>;
  /** Balance where the host does not keep it on the store. */
  readBalance?(account: string): Balance | undefined;
  /** The PLURAL reads, for a host whose markets/tickers are not on the store.
   *  Without these a host that supplies only `findMarket`/`findTicker` answers
   *  the singular reads and returns nothing for the plural ones. */
  readMarkets?(account: string): readonly Market[];
  readTickers?(symbols: readonly string[], account: string): readonly Ticker[];
  /** The venue for an account the host may know about without it streaming.
   *  Web resolves this through a name map with two further fallbacks; without
   *  the binding an unknown account silently reads as the default. */
  readExchangeNameForAccount?(account: string): ExchangeName | undefined;

  // ── Order policy inputs the body cannot derive ─────────────────────────
  /**
   * Resting orders for the reduce-only clamp, INCLUDING any the host holds
   * optimistically. Web has in-progress rows that exist nowhere else; a host
   * that omits this simply clamps against what the store shows.
   */
  readRestingOrders?(account: string): readonly Order[];
  /** Market lookup where the host has a cache worth using. Defaults to a
   *  scan of `getStore(account).markets`. */
  findMarket?(symbol: string, account: string): Market | undefined;
  /** Tickers for the clamp's reference price, when not on the store. */
  findTicker?(symbol: string, account: string): Ticker | undefined;

  /**
   * When set, EVERY write refuses with this message and nothing reaches a
   * venue. Declared rather than implemented: mobile refuses writes as
   * defence-in-depth behind its script-bridge gate, and expressing that as one
   * property keeps it visible, greppable and testable instead of hand-written
   * throws that a write method added later would silently escape.
   *
   * **"Every write" means every path in `CORE_WRITE_API_PATHS`, and that
   * includes `runCommand`** — which for a while it did not. `runCommand` is
   * the REPL escape hatch: `runCommand('buy $1000')` places an order without
   * touching another verb on this interface, which is why `OEMS_WRITE_API_PATHS`
   * excludes it by name. It was the one write that skipped `guardWrites`, and
   * the refusal test's hand-written list of nine missed it — the same by-hand
   * omission this property replaced. Both are derived from the path list now.
   *
   * A host that sets this SERVES the read slice and declines the write slice.
   * That is a real position in the venn diagram, not a gap.
   */
  readonly writesRefusedReason?: string;

  // ── Side channels ──────────────────────────────────────────────────────
  print?(text: string, kind: PrintKind): MaybePromise<void>;
  runCommand?(
    command: string,
    opts: { account: string; symbol?: string; depth: number },
  ): MaybePromise<RunCommandResult>;
  /**
   * Fired for EVERY write, success or failure, from inside the body's own
   * catch. The CLI's audit log needs a partial `orderIds` on a thrown
   * `closePosition`, which a wrapper around the adapter could not see.
   */
  onWrite?(event: HostWriteEvent): void;
}

/** Positions and orders filter the same way. Not exported from the package —
 *  it is an implementation detail of the body. */
export const bySymbol = <T extends { symbol: string }>(rows: readonly T[], symbol?: string): readonly T[] =>
  symbol ? rows.filter((row) => row.symbol === symbol) : rows;
