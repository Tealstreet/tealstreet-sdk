import { BaseCommand } from '../BaseCommand';
import type { CommandContext } from '../Command';
import type { ExchangeLike } from '../../types';
import { PositionSide } from '@tealstreet/safe-cex/src/types';
import { add, adjust } from '@tealstreet/safe-cex/src/utils/safe-math';
import { nativePositionSize } from '@tealstreet/script-api-core/src/order/positionSizing';
import { amountPrecisionOf } from '../../utils/marketPrecision';
import { resolveMarketSymbol } from '../../utils/resolveMarketSymbol';
import { baseToNativeContracts } from '@tealstreet/safe-cex/src/utils/baseToNativeContracts';

/**
 * Base class for trading commands with utility methods for size/price processing
 */
export abstract class TradingBaseCommand extends BaseCommand {
  /**
   * Check if a string looks like a price (e.g., "100", "100.5", "+100", "-50", "5%", "$100")
   */
  protected looksLikePrice(str: string | number): boolean {
    // Optional `$` prefix, optional +/- prefix, a number with an optional
    // decimal part, and an optional trailing `%`.
    //
    // The percent used to be a separate `\d+%` alternative, which accepted `1%`
    // and rejected `0.1%` — and `stop at 0.1%` is StopCommand's own first
    // documented example, so it threw "Price 0.1% does not look like a price".
    // Fractional percentages are the normal case for a tight stop.
    return /^\$?[+-]?\d+(\.\d+)?%?$/.test(String(str));
  }

  /**
   * Check if a string looks like a USD size. Both `$1000` (prefix) and
   * `1000$` (suffix) accepted per Phase 0 decision 1; bare numbers must
   * be rejected (callers throw a "USD or % required" error so a typo
   * like `buy 100` fails loud rather than coercing).
   */
  protected looksLikeSize(str: string | number): boolean {
    const s = String(str);
    return /^\$\d+(\.\d+)?$/.test(s) || /^\d+(\.\d+)?\$$/.test(s);
  }

  /** Strip the leading or trailing `$` and parse the number. Caller
   *  must have already verified `looksLikeSize`. */
  protected parseUsdAmount(size: string): number {
    if (size.startsWith('$')) return parseFloat(size.slice(1));
    if (size.endsWith('$')) return parseFloat(size.slice(0, -1));
    throw new Error(`Invalid USD size: ${size}`);
  }

  /**
   * Convert a percentage string to an absolute price
   */
  protected convertPercentageToPrice(percentage: string, currentPrice: number): number {
    const cleanPercentage = parseFloat(percentage.replace('%', ''));
    return currentPrice + currentPrice * (cleanPercentage / 100);
  }

  /**
   * Process a price argument - handles percentages and relative prices
   */
  protected processPrice(price: string, currentPrice: number): string {
    // Strip leading $ if present
    const cleanPrice = price.startsWith('$') ? price.slice(1) : price;

    if (!this.looksLikePrice(price)) {
      throw new Error(`Price ${price} does not look like a price`);
    }
    if (cleanPrice.endsWith('%')) {
      return this.convertPercentageToPrice(cleanPrice, currentPrice).toString();
    }
    if (cleanPrice.startsWith('+') || cleanPrice.startsWith('-')) {
      return String(add(currentPrice, parseFloat(cleanPrice)));
    }
    return cleanPrice;
  }

  /**
   * Parse a size argument and convert to contract size.
   *
   * Accepted forms (Phase 0 decision 1):
   *   - `$1000` / `1000$`        — USD notional (always)
   *   - `50%`                    — percent; defaults per `defaultPercentScope`
   *   - `50%bal`                 — explicit percent of free balance
   *   - `50%pos`                 — explicit percent of open position
   *
   * Bare numbers throw — `buy 100` (without a marker) is a dangerous
   * typo and must not silently coerce to either coins or USD.
   *
   * For percent-of-balance the contract count is
   * `(balance.free * leverage * pct/100) / price`, where leverage
   * comes from an open position if any and otherwise defaults to 1x.
   *
   * @param defaultPercentScope which scope a bare `%` resolves to
   *        when no `%bal`/`%pos` suffix is present. Entry commands
   *        (`buy`, non-reduce `chase`) pass `'balance'`; exit commands
   *        (`close`, reduce-only `stop`/`chase`) pass `'position'`.
   * @param price resolved order price; omitted for market-price sizing.
   */
  protected parseSizeAndConvertIfUsd(
    exchange: ExchangeLike,
    symbol: string,
    size: string,
    allowPercentage = false,
    side?: PositionSide,
    defaultPercentScope: 'position' | 'balance' = 'position',
    price?: number,
  ): number {
    const trimmed = size.trim();

    // Percent forms (only when allowed)
    if (allowPercentage && /^[+-]?\d+(\.\d+)?(%|%bal|%pos)$/.test(trimmed)) {
      let scope: 'position' | 'balance' = defaultPercentScope;
      let pctStr = trimmed;
      if (trimmed.endsWith('%bal')) {
        scope = 'balance';
        pctStr = trimmed.slice(0, -4);
      } else if (trimmed.endsWith('%pos')) {
        scope = 'position';
        pctStr = trimmed.slice(0, -4);
      } else {
        pctStr = trimmed.slice(0, -1);
      }
      const pct = parseFloat(pctStr);
      if (!Number.isFinite(pct) || pct <= 0) {
        throw new Error(`Invalid percentage: ${size}`);
      }

      const market = exchange.store?.markets?.find((m) => m.symbol === symbol);
      if (!market) {
        throw new Error(`Market data not found for symbol ${symbol}`);
      }

      if (scope === 'position') {
        const position = exchange.store?.positions?.find(
          (p) => p.symbol === symbol && (side === undefined || p.side === side),
        );
        if (!position) {
          throw new Error(`No ${side ?? ''} position found for symbol ${symbol}`.replace(/\s+/g, ' ').trim());
        }
        // NATIVE contracts. `position.contracts` is base and differs by
        // `contractSize`, so `close 50%` on an OKX-shaped market asked for a
        // multiple of the position rather than half of it.
        return adjust((nativePositionSize(position, market) * pct) / 100, amountPrecisionOf(market));
      }

      // balance scope
      const balance = exchange.store?.balance;
      if (!balance) {
        throw new Error('Balance not available yet, try again');
      }
      const positionForLev = (exchange.store?.positions ?? []).find((p) => p.symbol === symbol && p.contracts !== 0);
      const leverage = positionForLev?.leverage ?? 1;
      const notional = (balance.free * leverage * pct) / 100;
      // Inverse contracts are USD-denominated: each contract is
      // `contractSize` USD (1 for Bybit/Bitget; 100 or 10 for
      // Binance Coin-M / Deribit). The contract count is notional /
      // contractSize. No divide-by-price; we also skip the ticker
      // lookup, since it's irrelevant on this path. See the longer
      // explanation in `notionalToSize`.
      if (market.isInverse) {
        const contractSize = market.contractSize && market.contractSize > 0 ? market.contractSize : 1;
        return adjust(notional / contractSize, amountPrecisionOf(market));
      }
      const conversionPrice = price ?? exchange.store?.tickers?.find((t) => t.symbol === symbol)?.last;
      if (!conversionPrice) {
        throw new Error(`Price not available for ${symbol}`);
      }
      // See notionalToSize — linear venues can still carry a ctVal (OKX).
      return adjust(baseToNativeContracts(market, notional / conversionPrice), amountPrecisionOf(market));
    }

    if (this.looksLikeSize(trimmed)) {
      return this.notionalToSize(exchange, symbol, this.parseUsdAmount(trimmed), price);
    }

    throw new Error(`Invalid size "${size}". Use $1000, 1000$, or NN% (or NN%bal / NN%pos).`);
  }

  /**
   * Convert USD notional to contract size.
   *
   * Most exchanges are **linear**: 1 contract = 1 base coin, so a $100
   * notional on BTC at $50k is 0.002 BTC and `amount = notional/price`.
   *
   * **Inverse contracts** flip this: each contract represents a fixed
   * USD amount (the `contractSize`), so the amount you send is
   * `notional / contractSize`. Dividing by price (the linear formula)
   * would round to zero against the typical precision (1) and the
   * order goes out empty — that's the `orderQty is required`
   * rejection a user reported on BitMEX (retired 2026-10).
   *
   * Inverse contract-size landscape across exchanges:
   *   - Bybit inverse BTCUSD       → $1   per contract (contractSize unset; default 1)
   *   - Bitget inverse BTCUSD      → $1   per contract (contractSize unset; default 1)
   *   - Binance Coin-M BTCUSD_PERP → $100 per contract (if we ever support it)
   *   - Binance Coin-M ETHUSD_PERP → $10  per contract (same)
   *   - Deribit BTC-PERPETUAL      → $10  per contract (same)
   *
   * We read `market.contractSize` when set and default to 1 for the
   * exchanges we support today. New inverse-quoting exchanges should
   * populate `contractSize` in their `fetchMarkets` mapping so this
   * formula stays correct without per-exchange branches here.
   */
  protected notionalToSize(exchange: ExchangeLike, symbol: string, notional: number, price?: number): number {
    const market = exchange.store?.markets?.find((m) => m.symbol === symbol);
    const amountPrecision = amountPrecisionOf(market);

    if (market?.isInverse) {
      const contractSize = market.contractSize && market.contractSize > 0 ? market.contractSize : 1;
      return adjust(notional / contractSize, amountPrecision);
    }

    const conversionPrice = price ?? exchange.store?.tickers?.find((ticker) => ticker.symbol === symbol)?.last;
    if (!conversionPrice) {
      throw new Error('Price data is not available yet for a symbol');
    }
    // Linear is NOT always 1 contract = 1 base coin — OKX publishes a real ctVal
    // on its linear swaps (1000 DOGE, 0.01 BTC), so the base amount has to be
    // divided by it to get the native contract count.
    const baseAmount = notional / conversionPrice;
    return adjust(baseToNativeContracts(market ?? { contractSize: 1 }, baseAmount), amountPrecision);
  }

  /**
   * Check if a symbol exists in the exchange
   */
  protected validateSymbol(exchange: ExchangeLike, symbol: string): boolean {
    return exchange.store?.tickers?.some((ticker) => ticker.symbol === symbol) ?? false;
  }

  /**
   * Transform a symbol to the exchange's format
   */
  protected transformSymbol(exchange: ExchangeLike, symbol: string): string {
    return resolveMarketSymbol(exchange.store?.markets, symbol);
  }

  /**
   * Validate and transform multiple symbols
   */
  protected validateSymbols(
    exchange: ExchangeLike,
    symbols?: string[],
    allowAll?: boolean,
    side?: PositionSide,
  ): string[] {
    if (!Array.isArray(symbols)) {
      return [];
    }
    if (allowAll && symbols.includes('%all%')) {
      return (exchange.store?.positions ?? [])
        .filter((position) => position.side === side)
        .map((position) => position.symbol);
    }
    return symbols
      .filter((symbol) => !this.tokenLooksLikeNonSymbol(symbol))
      .map((symbol) => this.transformSymbol(exchange, symbol))
      .filter((symbol) => this.validateSymbol(exchange, symbol));
  }

  /**
   * Reject anything that already parses as a size, price, or percent
   * before it reaches `transformSymbol`. Without this, tokens like
   * `'1000$'` slip through `transformSymbol`'s fuzzy base-prefix
   * matching when an exchange has a market with a base that prefixes
   * `'1000'` (some chains list a literal `'1000'` token, and the
   * `1000$` size suffix happens to start with that). The result was
   * `buy 1000$` getting routed as a SYMBOL instead of a SIZE → no
   * `args[2]` for size → either "Size argument missing" or, on
   * downstream paths, an empty `qty` field hitting the exchange
   * ("orderQty is required").
   */
  protected tokenLooksLikeNonSymbol(token: string): boolean {
    if (typeof token !== 'string') return false;
    const t = token.trim();
    if (!t) return false;
    // Size-shaped: $N, N$, N%, N%bal, N%pos.
    if (this.looksLikeSize(t)) return true;
    if (/^\d+(?:\.\d+)?(?:%|%bal|%pos)$/.test(t)) return true;
    // Pure-numeric / signed numeric — e.g. price-shaped tokens that
    // someone might accidentally pass into the trailing-symbol scan.
    if (/^[+-]?\d+(?:\.\d+)?$/.test(t)) return true;
    // Percent without a markers — `5%`, `-1.1%`.
    if (/^[+-]?\d+(?:\.\d+)?%$/.test(t)) return true;
    return false;
  }

  /**
   * Check if an argument is present in the args array
   */
  protected hasArgValue(args: string[], key: string): boolean {
    return args.includes(key);
  }

  /**
   * Find the index of an argument in the args array
   */
  protected findArgIndex(args: string[], key: string): number {
    return args.indexOf(key);
  }

  /**
   * Get the raw args array from context (for legacy command porting)
   */
  protected getRawArgs(ctx: CommandContext): string[] {
    return ctx.args._;
  }

  /**
   * Get target symbol from args - checks last args for a valid symbol
   */
  protected getTargetSymbol(ctx: CommandContext, skipKeywords: string[] = []): string {
    const exchange = ctx.exchange;
    if (!exchange) return ctx.symbol;

    const args = this.getRawArgs(ctx);
    // Check from end backwards for valid symbol
    for (let i = args.length - 1; i >= 1; i--) {
      const arg = args[i];
      // Skip known keywords
      if (skipKeywords.includes(arg.toLowerCase())) continue;
      const validated = this.validateSymbols(exchange, [arg]);
      if (validated.length > 0) {
        return validated[0];
      }
    }
    return ctx.symbol;
  }

  /**
   * Throw if a per-individual order would exceed the user-set
   * fatfinger limit for the symbol. Sized order commands call this
   * just before placeOrder. The check is per-call (matching ichibot)
   * — `buy 1, repeat 11` is allowed even if the running total would
   * exceed the limit.
   *
   * Returns silently when:
   *   - There's no PreprocessStore on the context (e.g. raw command
   *     dispatch with no user state).
   *   - No limit is set for the symbol (or it's set to <= 0).
   *
   * Throws otherwise — callers should catch and surface as a command
   * error, NOT a hard exception.
   */
  protected enforceFatfinger(ctx: CommandContext, symbol: string, contracts: number): void {
    const limit = ctx.preprocessStore?.getFatfinger(symbol);
    if (limit === undefined || !Number.isFinite(limit) || limit <= 0) return;
    if (Math.abs(contracts) > limit) {
      throw new Error(
        `fatfinger: ${symbol} order size ${Math.abs(contracts)} exceeds limit ${limit}. ` +
          `Use \`fatfinger ${limit * 2} ${symbol}\` to raise it or \`unfatfinger ${symbol}\` to remove.`,
      );
    }
  }

  /**
   * Convert a normalized order `amount` (base units / contract count, as
   * produced by parseSizeForEntry / notionalToSize) back to USD notional
   * — the inverse of `notionalToSize`. Inverse contracts ARE USD
   * (notional = amount × contractSize); linear/quanto are base×price.
   * Returns 0 when price is unavailable so the budget gate fails open
   * rather than mis-counting.
   */
  protected sizeToNotional(exchange: ExchangeLike, symbol: string, amount: number): number {
    const market = exchange.store?.markets?.find((m) => m.symbol === symbol);
    if (market?.isInverse) {
      const contractSize = market.contractSize && market.contractSize > 0 ? market.contractSize : 1;
      return Math.abs(amount) * contractSize;
    }
    const price = exchange.store?.tickers?.find((t) => t.symbol === symbol)?.last;
    if (!price) return 0;
    return Math.abs(amount) * price;
  }

  /**
   * Aggregate-budget CHECK (TSBUG-106). When an opt-in cap is set, throw
   * if this order's USD notional would push session spend over the cap.
   * Does NOT charge — returns the notional so the caller can charge it
   * via `store.addBudgetSpent(...)` ONLY after the order actually places
   * (a rejected order must not consume budget). Off by default (no cap →
   * returns 0). Caller should only invoke this for exposure-increasing
   * orders (skip reduce-only). Throws on breach — callers catch and
   * surface as a command error.
   */
  protected checkBudget(ctx: CommandContext, exchange: ExchangeLike, symbol: string, amount: number): number {
    const store = ctx.preprocessStore;
    const cap = store?.getBudget();
    if (!store || !cap || cap <= 0) return 0;
    const notional = this.sizeToNotional(exchange, symbol, amount);
    if (notional <= 0) return 0; // can't price → don't gate (fail open)
    const spent = store.getBudgetSpent();
    if (spent + notional > cap) {
      throw new Error(
        `budget: $${spent.toFixed(2)} spent + $${notional.toFixed(2)} this order exceeds the $${cap.toFixed(2)} cap. ` +
          `Use \`budget reset\` to clear, \`budget <usd>\` to raise it, or \`budget off\` to disable.`,
      );
    }
    return notional;
  }
}
