import { TradingBaseCommand } from './TradingBaseCommand';
import { CommandCategory, type CommandContext, type CommandResult } from '../Command';
import { prepareCliOrder } from '../../order/prepareCliOrder';
import { OrderSide, OrderTimeInForce, OrderType, PlaceOrderOpts, PositionSide } from '@tealstreet/safe-cex/src/types';

/**
 * Base class for buy/sell commands
 */
export abstract class BaseBuySellCommand extends TradingBaseCommand {
  abstract readonly name: string;
  abstract readonly description: string;
  readonly category = CommandCategory.Trading;
  readonly aliases: string[] = [];
  abstract readonly examples: string[];
  readonly requiresConnection = true;

  protected abstract get side(): OrderSide;

  async execute(ctx: CommandContext): Promise<CommandResult> {
    if (this.simulationGate(ctx)) return this.success();
    const exchange = ctx.exchange;
    if (!exchange) {
      return this.error('Not connected to exchange');
    }
    if (!exchange.placeOrder) {
      return this.error('Exchange does not support placing orders');
    }

    // Fail closed on a typo'd explicit symbol. Without this, an
    // unresolved symbol token is silently dropped and the order routes
    // to the focused symbol — e.g. `buy $100 XBTUSD` on an exchange
    // with no XBTUSD market would fill the focused symbol instead
    // (TSBUG-102). Omitting the symbol entirely still falls back to
    // ctx.symbol by design.
    const unknownSymbol = this.findUnknownSymbolToken(ctx);
    if (unknownSymbol) {
      return this.error(
        `Unknown symbol "${unknownSymbol}". Use a valid market symbol, or omit it to trade the focused symbol (${ctx.symbol}).`,
      );
    }

    const args = this.getRawArgs(ctx);

    // Phase 3: `best` → limit at best bid/ask (mutually exclusive with `at`).
    if (this.hasArgValue(args, 'best')) {
      return this.handleBestOrder(ctx);
    }

    // Limit order (has "at" keyword)
    if (this.hasArgValue(args, 'at')) {
      return this.handleLimitOrder(ctx);
    }
    return this.handleMarketOrder(ctx);
  }

  /**
   * Resolve `stop <price>` if present in args. Returns the absolute
   * stop price (after `processPrice` for relative forms), or undefined
   * if not present. Throws on malformed args.
   */
  protected resolveStopLoss(ctx: CommandContext, symbol: string, args: string[]): number | undefined {
    const stopIdx = this.findArgIndex(args, 'stop');
    if (stopIdx === -1) return undefined;
    const arg = args[stopIdx + 1];
    if (!arg) {
      throw new Error("'stop' must be followed by a price");
    }
    const ticker = ctx.exchange!.store?.tickers?.find((t) => t.symbol === symbol);
    if (!ticker?.last) {
      throw new Error(`Current price not available for ${symbol}`);
    }
    return parseFloat(this.processPrice(arg, ticker.last));
  }

  /**
   * Scan the args for a token that was clearly meant to be a symbol but
   * doesn't resolve to any market on this exchange. The buy/sell grammar
   * is `[SYMBOL] SIZE [at PRICE] [best] [stop PRICE] [reduce] [po]
   * [SYMBOL]`, so once keywords, their price operands, and any
   * size/percent/number token are excluded, anything left that isn't a
   * resolvable market is a typo'd symbol. Returns the first such token
   * (so `execute` can fail closed) or undefined when every token is
   * accounted for. Comma lists count as resolvable if at least one part
   * is a real market (matching `validateSymbols`).
   */
  protected findUnknownSymbolToken(ctx: CommandContext): string | undefined {
    const exchange = ctx.exchange;
    if (!exchange) return undefined;
    const args = this.getRawArgs(ctx);
    const KEYWORDS = new Set(['reduce', 'po', 'best', 'at', 'stop']);
    for (let i = 1; i < args.length; i++) {
      const tok = args[i];
      if (!tok) continue;
      if (KEYWORDS.has(tok.toLowerCase())) continue;
      // Price operand directly after `at` / `stop`.
      const prev = args[i - 1]?.toLowerCase();
      if (prev === 'at' || prev === 'stop') continue;
      // Size / percent / bare-number tokens are never symbols.
      if (this.tokenLooksLikeNonSymbol(tok)) continue;
      // Symbol (or comma list of symbols). Fail closed if ANY part
      // doesn't resolve — `buy BTCUSDT,XBTUSD $100` must not silently
      // route only the valid leg and drop the typo'd one.
      const parts = tok
        .split(',')
        .map((s) => s.trim())
        .filter(Boolean);
      for (const part of parts) {
        if (this.validateSymbols(exchange, [part]).length === 0) return part;
      }
    }
    return undefined;
  }

  protected async handleMarketOrder(ctx: CommandContext): Promise<CommandResult> {
    const exchange = ctx.exchange!;
    const args = this.getRawArgs(ctx);
    const reduceOnly = this.hasArgValue(args, 'reduce');
    // `po` is parsed in market-order handling for symmetry but ignored
    // — market orders don't have a post-only mode. We still consume
    // the keyword so it doesn't trip symbol detection.
    const hasAttachedStop = this.findArgIndex(args, 'stop') !== -1;

    // Safety check: detect bare numbers that look like prices (forgot "at" keyword).
    // Skipped when `stop <p>` is present — the number after `stop` is the
    // intentional stop-loss price, not a forgotten limit.
    // When the SYMBOL leads (`buy BTC 1`), args[2] is the SIZE slot, and a bare
    // size there is legitimate — it means raw contracts. Scanning from index 2
    // unconditionally turned `buy BTC 1` into a syntax error.
    const symbolLeads = this.validateSymbols(exchange, args[1]?.split(',') || []).length > 0;
    const firstUnconsumedIndex = symbolLeads ? 3 : 2;

    if (!hasAttachedStop) {
      for (let i = firstUnconsumedIndex; i < args.length; i++) {
        const arg = args[i];
        if (arg === 'reduce' || arg === 'po') continue;
        if (/^\d+(\.\d+)?$/.test(arg)) {
          // ANY unconsumed bare number, not just one >= 100.
          //
          // The threshold assumed BTC-shaped prices. On a coin quoted at
          // $0.05 it never fired, so `sell 33$ 0.051533 reduce` dropped the
          // price and MARKET-sold — which is what a user reported as "my
          // limit filled instantly at a terrible price". There is no reading
          // of a leftover number here that means anything, so refusing is
          // always right and silently discarding it never was.
          this.printError(
            ctx,
            `Unexpected number "${arg}" - did you mean "${args[0]} ${args[1]} at ${arg}"? Use "at" keyword for limit orders.`,
          );
          return this.error('Invalid syntax');
        }
      }
    }

    // Check for symbol at start: "buy SYMBOL $100"
    const symbolsAtStart = this.validateSymbols(exchange, args[1]?.split(',') || []);
    if (symbolsAtStart.length > 0) {
      if (!args[2]) {
        return this.error('Size argument missing');
      }
      const orderIds: string[] = [];
      for (const sym of symbolsAtStart) {
        const stopLoss = this.maybeStopLoss(ctx, sym, args);
        if (stopLoss instanceof Error) return this.error(stopLoss.message);
        orderIds.push(...(await this.placeMarketOrder(ctx, sym, args[2], reduceOnly, stopLoss)));
      }
      this.printSuccess(ctx, symbolsAtStart.length > 1 ? 'Orders placed' : 'Order placed');
      return this.successWithData({ orderIds });
    }

    // Check for symbol at end: "buy $100 SYMBOL" or "buy $100 reduce SYMBOL"
    let trailingSymbols: string[] = [];
    for (let i = args.length - 1; i >= 2; i--) {
      if (args[i] === 'reduce' || args[i] === 'po' || args[i] === 'stop' || args[i] === 'best') continue;
      // Skip the price token immediately after `stop`
      if (i > 0 && args[i - 1] === 'stop') continue;
      trailingSymbols = this.validateSymbols(exchange, args[i].split(','));
      if (trailingSymbols.length > 0) break;
    }

    if (trailingSymbols.length > 0) {
      const orderIds: string[] = [];
      for (const sym of trailingSymbols) {
        const stopLoss = this.maybeStopLoss(ctx, sym, args);
        if (stopLoss instanceof Error) return this.error(stopLoss.message);
        orderIds.push(...(await this.placeMarketOrder(ctx, sym, args[1], reduceOnly, stopLoss)));
      }
      this.printSuccess(ctx, trailingSymbols.length > 1 ? 'Orders placed' : 'Order placed');
      return this.successWithData({ orderIds });
    }

    // Default: use session symbol
    if (!args[1]) {
      return this.error(`Size argument missing. Usage: ${this.name} $amount`);
    }
    const stopLoss = this.maybeStopLoss(ctx, ctx.symbol, args);
    if (stopLoss instanceof Error) return this.error(stopLoss.message);
    const orderIds = await this.placeMarketOrder(ctx, ctx.symbol, args[1], reduceOnly, stopLoss);
    this.printSuccess(ctx, 'Order placed');
    return this.successWithData({ orderIds });
  }

  /** Resolve stop-loss arg, swallowing thrown errors as Error instances
   *  so callers can short-circuit cleanly. Returns undefined if no
   *  `stop` keyword in args. */
  private maybeStopLoss(ctx: CommandContext, symbol: string, args: string[]): number | undefined | Error {
    try {
      return this.resolveStopLoss(ctx, symbol, args);
    } catch (e) {
      return e as Error;
    }
  }

  protected async handleBestOrder(ctx: CommandContext): Promise<CommandResult> {
    const exchange = ctx.exchange!;
    const args = this.getRawArgs(ctx);
    const reduceOnly = this.hasArgValue(args, 'reduce');
    const postOnly = this.hasArgValue(args, 'po');

    // Determine the symbol — either at start ("buy SYMBOL $100 best") or
    // at end ("buy $100 best SYMBOL"). Size always comes second after a
    // leading symbol, otherwise first.
    const symbolsAtStart = this.validateSymbols(exchange, args[1]?.split(',') || []);
    const possibleSymbols: string[] = [];
    let sizeArgIndex = 1;
    if (symbolsAtStart.length > 0) {
      possibleSymbols.push(...symbolsAtStart);
      sizeArgIndex = 2;
    } else {
      // Look for trailing symbol; skip best/reduce/stop and the price after stop.
      for (let i = args.length - 1; i >= 2; i--) {
        if (args[i] === 'reduce' || args[i] === 'po' || args[i] === 'best' || args[i] === 'stop') continue;
        if (i > 0 && args[i - 1] === 'stop') continue;
        const t = this.validateSymbols(exchange, args[i].split(','));
        if (t.length > 0) {
          possibleSymbols.push(...t);
          break;
        }
      }
      if (possibleSymbols.length === 0) possibleSymbols.push(ctx.symbol);
    }

    const sizeArg = args[sizeArgIndex];
    if (!sizeArg) {
      return this.error('Size argument missing');
    }

    const orderIds: string[] = [];
    for (const sym of possibleSymbols) {
      const ticker = exchange.store?.tickers?.find((t) => t.symbol === sym);
      if (!ticker) {
        return this.error(`Ticker not available for ${sym}`);
      }
      // best for buy = best bid (passive, joins the queue);
      // best for sell = best ask. Falls back to last price if the
      // ticker is missing bid/ask.
      const bestPrice = this.side === OrderSide.Buy ? ticker.bid || ticker.last : ticker.ask || ticker.last;
      if (!bestPrice) {
        return this.error(`No bid/ask available for ${sym}`);
      }
      const stopLoss = this.maybeStopLoss(ctx, sym, args);
      if (stopLoss instanceof Error) return this.error(stopLoss.message);
      orderIds.push(...(await this.placeLimitOrder(ctx, sym, sizeArg, bestPrice, reduceOnly, stopLoss, postOnly)));
    }
    this.printSuccess(ctx, possibleSymbols.length > 1 ? 'Orders placed' : 'Order placed');
    return this.successWithData({ orderIds });
  }

  protected async handleLimitOrder(ctx: CommandContext): Promise<CommandResult> {
    const exchange = ctx.exchange!;
    const args = this.getRawArgs(ctx);
    const reduceOnly = this.hasArgValue(args, 'reduce');
    const postOnly = this.hasArgValue(args, 'po');

    // Check for symbol at start: "buy SYMBOL $100 at 50000"
    const symbolsAtStart = this.validateSymbols(exchange, args[1]?.split(',') || []);
    let sizeArgIndex = 1;

    const possibleSymbols: string[] = [];
    if (symbolsAtStart.length > 0) {
      possibleSymbols.push(...symbolsAtStart);
      sizeArgIndex = 2;
    } else {
      // Check for symbol at end: "buy $100 at 50000 SYMBOL"
      //
      // `stop` and the price after it are skipped for the same reason the
      // market and `best` paths skip them, and this path did not: without it
      // `buy 100$ at 0.051124 stop 0.049570` walks back to the literal token
      // `stop`, and `resolveMarketSymbol` prefix-matches `STOP` against any
      // base a venue happens to list — so the whole bracket routes to a market
      // the user never named.
      const atIndex = this.findArgIndex(args, 'at');
      for (let i = args.length - 1; i > atIndex + 1; i--) {
        if (args[i] === 'reduce' || args[i] === 'po' || args[i] === 'best' || args[i] === 'stop') continue;
        if (i > 0 && args[i - 1] === 'stop') continue;
        const trailingSymbols = this.validateSymbols(exchange, args[i].split(','));
        if (trailingSymbols.length > 0) {
          possibleSymbols.push(...trailingSymbols);
          break;
        }
      }
    }

    // Default to session symbol if no explicit symbol found
    if (possibleSymbols.length === 0) {
      possibleSymbols.push(ctx.symbol);
    }

    const orderIds: string[] = [];
    for (const sym of possibleSymbols) {
      const currentPrice = exchange.store?.tickers?.find((ticker) => ticker.symbol === sym)?.last;
      if (!currentPrice) {
        return this.error(`Current price is not available for ticker ${sym}, try again.`);
      }
      const atIndex = this.findArgIndex(args, 'at');
      if (atIndex === -1 || atIndex + 1 >= args.length) {
        return this.error('Price argument missing after "at" keyword');
      }
      const price = args[atIndex + 1];
      const stopLoss = this.maybeStopLoss(ctx, sym, args);
      if (stopLoss instanceof Error) return this.error(stopLoss.message);
      orderIds.push(
        ...(await this.placeLimitOrder(
          ctx,
          sym,
          args[sizeArgIndex],
          parseFloat(this.processPrice(price, currentPrice)),
          reduceOnly,
          stopLoss,
          postOnly,
        )),
      );
    }

    this.printSuccess(ctx, possibleSymbols.length > 1 ? 'Orders placed' : 'Order placed');
    return this.successWithData({ orderIds });
  }

  protected async placeMarketOrder(
    ctx: CommandContext,
    symbol: string,
    sizeArg: string,
    reduceOnly: boolean,
    stopLoss?: number,
  ): Promise<string[]> {
    const exchange = ctx.exchange!;
    // Health gate + fatfinger only apply to exposure-increasing orders;
    // reduce-only closes stay allowed. Gated here (not in execute) so a
    // malformed command surfaces its parse error, not the degraded
    // message (TSBUG-106).
    if (!reduceOnly) {
      const blocked = this.healthGate(ctx);
      if (blocked) throw new Error(blocked);
    }
    const amount = this.parseSizeForEntry(exchange, symbol, sizeArg, reduceOnly);
    // Skip fatfinger + budget on reduce-only orders — both gate new
    // exposure, not closing existing exposure.
    let budgetNotional = 0;
    if (!reduceOnly) {
      this.enforceFatfinger(ctx, symbol, amount);
      budgetNotional = this.checkBudget(ctx, exchange, symbol, amount);
    }
    const order: PlaceOrderOpts = {
      symbol,
      side: this.side,
      type: OrderType.Market,
      amount,
      reduceOnly,
      timeInForce: OrderTimeInForce.GoodTillCancel,
      ...(stopLoss !== undefined ? { stopLoss } : {}),
    };
    // Exchange placeOrder returns Promise<string[]>. Bracket orders
    // (with stopLoss) typically produce 2-3 IDs (entry + SL + maybe
    // TP). Bybit's position-level TP/SL endpoint returns []. The
    // helper surfaces whatever the exchange returned; the runner
    // decides what to do with an empty array.
    const ids = (await exchange.placeOrder!(prepareCliOrder(exchange, order))) ?? [];
    // Charge the budget only AFTER the order placed (a throw above skips this).
    if (budgetNotional > 0) ctx.preprocessStore?.addBudgetSpent(budgetNotional);
    return ids;
  }

  protected async placeLimitOrder(
    ctx: CommandContext,
    symbol: string,
    sizeArg: string,
    price: number,
    reduceOnly: boolean,
    stopLoss?: number,
    postOnly = false,
  ): Promise<string[]> {
    const exchange = ctx.exchange!;
    if (!reduceOnly) {
      const blocked = this.healthGate(ctx);
      if (blocked) throw new Error(blocked);
    }
    const amount = this.parseSizeForEntry(exchange, symbol, sizeArg, reduceOnly, price);
    let budgetNotional = 0;
    if (!reduceOnly) {
      this.enforceFatfinger(ctx, symbol, amount);
      budgetNotional = this.checkBudget(ctx, exchange, symbol, amount);
    }
    const order: PlaceOrderOpts = {
      symbol,
      side: this.side,
      type: OrderType.Limit,
      price,
      amount,
      reduceOnly,
      timeInForce: postOnly ? OrderTimeInForce.PostOnly : OrderTimeInForce.GoodTillCancel,
      ...(stopLoss !== undefined ? { stopLoss } : {}),
    };
    const ids = (await exchange.placeOrder!(prepareCliOrder(exchange, order))) ?? [];
    if (budgetNotional > 0) ctx.preprocessStore?.addBudgetSpent(budgetNotional);
    return ids;
  }

  /** Buy/sell are entry commands per Phase 0 decision 1, so a bare `%`
   *  resolves to % of free balance — EXCEPT on a reduce-only order,
   *  where you're closing and a bare `%` means "% of the open position"
   *  (matching the documented scope contract on parseSizeAndConvertIfUsd).
   *  `%bal` / `%pos` overrides still work regardless. Refuses a size that
   *  rounds to 0 at the market's amount precision so an empty order never
   *  reaches the exchange (TSBUG-105). */
  protected parseSizeForEntry(
    exchange: import('../../types').ExchangeLike,
    symbol: string,
    size: string,
    reduceOnly = false,
    price?: number,
  ): number {
    // A reduce-only order closes the OPPOSITE side — a buy reduces a
    // short, a sell reduces a long. Pass that PositionSide so a bare `%`
    // sizes off the correct leg on hedged accounts (no-op on one-way
    // accounts, which only have a single leg per symbol).
    const reduceSide = reduceOnly ? (this.side === OrderSide.Buy ? PositionSide.Short : PositionSide.Long) : undefined;
    const amount = this.parseSizeAndConvertIfUsd(
      exchange,
      symbol,
      size,
      true,
      reduceSide,
      reduceOnly ? 'position' : 'balance',
      price,
    );
    if (!(amount > 0)) {
      throw new Error(`Size "${size}" rounds to 0 ${symbol} at this market's precision — order not placed.`);
    }
    return amount;
  }
}
