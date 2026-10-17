/**
 * The OEMS context — everything a caller needs to reason about the user's
 * current book, and nothing else.
 *
 * ## Why these shapes are not safe-cex's
 *
 * A safe-cex `Position` / `Order` / `Balance` carries venue payloads, nested
 * `info` objects and fields that exist for one exchange. This package stays
 * dependency-free and re-declares narrow POJOs instead, for three reasons:
 *
 * 1. **Credentials.** `exchangeAccountController.getAllAccounts()` returns
 *    rows holding `apiKey`, `apiSecret`, `passphrase`, the `secondary_*` pair
 *    and wallet private keys. A projection built by naming the fields it wants
 *    cannot leak one; a projection built by spreading and deleting can, and
 *    will, the first time a field is added upstream.
 * 2. **Size.** This context is serialised into a prompt on every request. A
 *    venue payload is worth nothing to a planner and is often larger than the
 *    rest of the book put together.
 * 3. **Stability.** These names are what a stored plan is written against.
 *    They must be free to stay still while safe-cex's types move.
 *
 * ## A ref is an ADDRESS, never a payload
 *
 * `cancelOrder`, `cancelOrders`, `updateOrder` and `closePosition` all take a
 * whole safe-cex object, and none of these refs is one. That is the design,
 * not a gap: the host re-resolves the address against its own store and
 * forwards ITS object (`resolveModuleOrder` / `resolveModulePosition` in
 * `apps/web/src/components/customModules/frame/`). Bybit's `updateOrder`
 * cancels and RE-PLACES from `{...order, ...update}`, so a forged `symbol` or
 * `amount` does not merely describe an order wrongly — it becomes a new live
 * order on whatever was named.
 *
 * So the only fields a consumer may send back are the ones in
 * `OemsOrderAddress` and `OemsPositionAddress`. Everything else here is for
 * READING. Widening an address is a security change.
 *
 * Every number is already rounded for display by the producer. A consumer must
 * treat them as an approximation of the book and let the host re-read live
 * state before acting — see `docs/plans/AI_OEMS_v1.md`.
 */

/** Order direction. Mirrors safe-cex `OrderSide` as plain literals. */
export type OemsSide = 'buy' | 'sell';

/** Position direction. Mirrors safe-cex `PositionSide`. */
export type OemsPositionSide = 'long' | 'short';

/** Order kind. Mirrors safe-cex `OrderType`. */
export type OemsOrderType =
  'market' | 'limit' | 'conditional' | 'stop_market' | 'take_profit_market' | 'trailing_stop_market';

/** Mirrors safe-cex `PositionMode`. */
export type OemsPositionMode = 'one-way' | 'hedged';

/**
 * One connected account. NEVER built by spreading an account config — see the
 * credentials note above.
 *
 * `active` and `testnet` are carried because omitting them lets a planner
 * target an account the user has switched off, or place a real-looking order
 * on a paper venue and report it as real.
 */
export interface OemsAccountRef {
  readonly name: string;
  readonly exchange: string;
  /** The user's own active/inactive toggle, not a health signal. */
  readonly active: boolean;
  readonly testnet: boolean;
  /** A live, non-disposed exchange object exists. False while connecting. */
  readonly connected: boolean;
}

/** The whole of what may be sent back to address an order. */
export interface OemsOrderAddress {
  readonly account: string;
  readonly id: string;
}

/**
 * The whole of what may be sent back to address a position.
 *
 * `side` is part of the key, not decoration: a hedge-mode account holds a long
 * and a short on one symbol at once, so `(account, symbol)` addresses two
 * different positions.
 */
export interface OemsPositionAddress {
  readonly account: string;
  readonly symbol: string;
  readonly side: OemsPositionSide;
}

export interface OemsPositionRef extends OemsPositionAddress {
  /** Absolute size in contracts. Always positive; `side` carries direction. */
  readonly contracts: number;
  /**
   * The same position in the venue's OWN units — what an order's `amount`
   * means, and what `market.amountStep` / `minAmount` are quoted in.
   *
   * It is not `contracts`, and the difference is not cosmetic: native is
   * `contracts / contractSize` on a linear market and `notional / contractSize`
   * on an inverse one. On OKX's `contractSize 0.01` markets the two differ by
   * a hundredfold, in the direction that silently CLOSES ALMOST NOTHING.
   *
   * A producer must derive it through `nativePositionSize`
   * (`packages/safe-cex/src/utils/nativePositionSize.ts`), which both
   * `getPositionNativeSize` and `getPositionNativeContracts` now call.
   *
   * **This used to say "including the `extra.contractsNative` preference",
   * meaning: return that value outright when it is non-zero.** That was the
   * drift, written down as a requirement. `extra.contractsNative` can be
   * STALE — `getPositionNativeSize.test.ts` pins a case where it reads 0.1
   * against a real 0.4 — so the rule is `Math.max(fromExtra, derived)`, and a
   * producer following the old wording sized every percentage close at a
   * quarter. `resolvePlan.sizeClose` takes `full` straight from this field.
   */
  readonly nativeContracts: number;
  readonly entryPrice: number;
  /** From the symbol's ticker, not the position — safe-cex `Position` has no
   *  mark price of its own. */
  readonly markPrice: number;
  /** The venue's own number. Not `contracts × markPrice`, which is wrong on an
   *  inverse contract. */
  readonly notional: number;
  readonly unrealizedPnl: number;
  readonly liquidationPrice: number;
  readonly leverage: number;
  /**
   * Load-bearing for anything that sizes a close. In `hedged` mode the two
   * legs are separate positions and a reduce must name a side; in `one-way`
   * mode naming the wrong side would flip the position rather than reduce it.
   */
  readonly positionMode: OemsPositionMode;
}

export interface OemsOrderRef extends OemsOrderAddress {
  readonly symbol: string;
  readonly side: OemsSide;
  readonly type: OemsOrderType;
  readonly amount: number;
  readonly filled: number;
  readonly remaining: number;
  /** `remaining` in the venue's own units — the unit an amend's `amount` is
   *  in. Same distinction as `OemsPositionRef.nativeContracts`. */
  readonly nativeRemaining: number;
  readonly price: number;
  readonly reduceOnly: boolean;
}

export interface OemsBalanceRef {
  readonly account: string;
  readonly currency: string;
  readonly total: number;
  readonly free: number;
  readonly used: number;
  /** The account's own unrealized PnL. Not the sum of `positions[].unrealizedPnl` —
   *  that excludes accounts whose positions were filtered out. */
  readonly upnl: number;
}

/**
 * A market's tradeable bounds, for the symbols the context actually mentions.
 *
 * Deliberately NOT the whole market list: a user connected to a handful of
 * venues has tens of thousands of markets, and a planner only ever needs the
 * ones it is being asked about. The producer includes the symbols carried by
 * `positions`, `orders` and `activeSymbol`, and nothing else.
 */
export interface OemsMarketRef {
  readonly symbol: string;
  readonly account: string;
  /** Last traded price. Present for symbols with no position, where there is
   *  no `markPrice` to read one from. */
  readonly lastPrice: number;
  /** Lot step, in the venue's NATIVE units — `market.precision.amount`, which
   *  every adapter maps from the venue's own `lotSz` / `qtyStep`. */
  readonly amountStep: number;
  readonly priceStep: number;
  /**
   * `market.limits.amount` — whose unit is ADAPTER-DEPENDENT and therefore only
   * safely comparable when `contractSize === 1`.
   *
   * OKX and BloFin build `min` as `minSz × ctVal` (base, or quote on an inverse
   * market) while leaving `max` in native contracts; KuCoin and
   * Crypto.com report both natively despite setting a `contractSize`. No single
   * conversion is right for all of them, and a wrong one refuses every order on
   * a whole venue — so a consumer must skip the comparison rather than guess.
   */
  readonly minAmount: number;
  readonly maxAmount: number;
  readonly maxLeverage: number;
  /** Quote per contract. A size in contracts is not a size in base without it. */
  readonly contractSize: number;
  /** Coin-margined: notional is quote-denominated and PnL is non-linear in
   *  price, so percentage sizing off `contracts × price` is wrong. */
  readonly isInverse: boolean;
  /** No leverage, no reduce-only, no short. `setLeverage` on a spot market is
   *  not a small error — it is a call the venue has no concept of. */
  readonly isSpot: boolean;
}

/**
 * What `boundOemsContext` had to leave out, per list.
 *
 * Present only when something was dropped. Read results have carried this
 * since the truncation marker landed; the snapshot did not, and the failure is
 * the same one: the model counts fifty of eighty positions, says "reducing
 * your fifty positions", and the confirm card — resolved against the raw book —
 * shows eighty.
 */
export interface OemsContextDropped {
  readonly accounts?: number;
  readonly positions?: number;
  readonly orders?: number;
  readonly balances?: number;
  readonly markets?: number;
}

export interface OemsContext {
  /** Epoch ms the snapshot was taken. Staleness is the consumer's problem. */
  readonly generatedAt: number;
  /** Set by `boundOemsContext`, absent when nothing was dropped. It MERGES
   *  across passes — a context bounded in the browser and again in the route
   *  can no longer see what the first pass removed. */
  readonly dropped?: OemsContextDropped;
  readonly activeAccount: string;
  readonly activeSymbol: string;
  readonly activeAccounts: readonly string[];
  readonly accounts: readonly OemsAccountRef[];
  readonly positions: readonly OemsPositionRef[];
  readonly orders: readonly OemsOrderRef[];
  readonly balances: readonly OemsBalanceRef[];
  readonly markets: readonly OemsMarketRef[];
  /**
   * The user's own fat-finger ceiling in quote currency; `0` means off.
   *
   * Carried because the host ENFORCES it by throwing a `ValidationError` from
   * inside the order path. Without it here, an over-size order looks fine on
   * the confirm card and then fails mid-batch with nothing said beforehand.
   * It applies to opening orders only — reduce-only and stop/take-profit are
   * exempt there, and must be exempt here too.
   */
  readonly fatFingerNotional: number;
}
