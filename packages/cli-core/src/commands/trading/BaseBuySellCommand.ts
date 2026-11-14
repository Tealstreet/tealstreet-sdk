import { deserializeCommandError, serializeCommandError } from '@tealstreet/safe-cex/src/commandOutcome';
import { TradingBaseCommand } from './TradingBaseCommand';
import { CommandCategory, type CommandContext, type CommandResult } from '../Command';
import { prepareCliOrders } from '../../order/prepareCliOrder';
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

    this.assertUnambiguousOrderModifiers(ctx, 1, ['reduce', 'po', 'best', 'at', 'stop', 'tp'], {
      at: 1,
      stop: 1,
      tp: 1,
    });
    ctx = this.resolveLeadingSymbolSlot(ctx, 1);
    const duplicate = this.refuseDuplicateModifiers(ctx, [['reduce'], ['po'], ['best', 'at'], ['stop'], ['tp']]);
    if (duplicate) return duplicate;

    // Fail closed on a typo'd explicit symbol. Without this, an
    // unresolved symbol token is silently dropped and the order routes
    // to the focused symbol — e.g. `buy $100 XBTUSD` on an exchange
    // with no XBTUSD market would fill the focused symbol instead
    // (TSBUG-102). Omitting the symbol entirely still falls back to
    // ctx.symbol by design.
    const unknownSymbol = this.findUnknownSymbolToken(ctx);
    if (unknownSymbol) {
      return this.error(
        `${this.unknownOrderArgument(unknownSymbol)} Unknown symbol "${unknownSymbol}". Use a valid market symbol, or omit it to trade the focused symbol (${ctx.symbol}).`,
      );
    }

    const args = this.getRawArgs(ctx);
    if (args.includes('po') && !args.includes('at') && !args.includes('best')) {
      return this.error('po requires a limit order: use at PRICE or best. Nothing was sent.');
    }
    const leadingSymbol = this.validateSymbols(exchange, args[1]?.split(',') || []).length > 0;
    const consumed = leadingSymbol ? [1, 2] : [1];
    for (const keyword of ['reduce', 'po', 'best', 'stop', 'tp', ...(args.includes('best') ? [] : ['at'])]) {
      const index = args.indexOf(keyword);
      if (index > 0) {
        consumed.push(index);
        if (['at', 'stop', 'tp'].includes(keyword) && args[index + 1] !== undefined) consumed.push(index + 1);
      }
    }
    const invalid = this.refuseUnknownOrderArgs(ctx, consumed, {
      allowSymbol: !leadingSymbol,
      symbolLists: true,
      symbolAfter: args.includes('at') && !args.includes('best') ? args.indexOf('at') + 1 : 1,
    });
    if (invalid) {
      if (/^\d+(?:\.\d+)?$/.test(args.find((_, index) => ![0, ...consumed].includes(index)) ?? '')) {
        this.printError(ctx, `${invalid.error} did you mean \`at <price>\`?`);
        return this.error('Invalid syntax');
      }
      return invalid;
    }

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
   * Resolve `stop <price>` or `tp <price>` if present in args. Returns the absolute
   * stop price (after `processPrice` for relative forms), or undefined
   * if not present. Throws on malformed args.
   */
  protected resolveBracketPrice(
    ctx: CommandContext,
    symbol: string,
    args: string[],
    modifier = 'stop',
  ): number | undefined {
    const stopIdx = this.findArgIndex(args, modifier);
    if (stopIdx === -1) return undefined;
    const arg = args[stopIdx + 1];
    if (!arg) {
      throw new Error(`'${modifier}' must be followed by a price`);
    }
    const ticker = ctx.exchange!.store?.tickers?.find((t) => t.symbol === symbol);
    if (!ticker?.last) {
      throw new Error(`Current price not available for ${symbol}`);
    }
    return parseFloat(this.processPrice(arg, ticker.last));
  }

  /** Reject unknown explicit symbols before constructing fan-out orders.
   * Price/size operands and implemented modifiers belong to their parsers. */
  protected findUnknownSymbolToken(ctx: CommandContext): string | undefined {
    const exchange = ctx.exchange;
    if (!exchange) return undefined;
    const args = this.getRawArgs(ctx);
    const KEYWORDS = new Set(['reduce', 'po', 'best', 'at', 'stop', 'tp']);
    for (let i = 1; i < args.length; i++) {
      const tok = args[i];
      if (!tok) continue;
      if (KEYWORDS.has(tok.toLowerCase())) continue;
      // Price operand directly after `at` / `stop`.
      const prev = args[i - 1]?.toLowerCase();
      if (prev === 'at' || prev === 'stop' || prev === 'tp') continue;
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
    // Check for symbol at start: "buy SYMBOL $100"
    const symbolsAtStart = this.validateSymbols(exchange, args[1]?.split(',') || []);
    if (symbolsAtStart.length > 0) {
      if (!args[2]) {
        return this.error('Size argument missing');
      }
      const orders: PlaceOrderOpts[] = [];
      for (const sym of symbolsAtStart) {
        const stopLoss = this.maybeStopLoss(ctx, sym, args);
        if (stopLoss instanceof Error) return this.error(stopLoss.message);
        orders.push(this.buildOrder(ctx, sym, args[2], reduceOnly, undefined, stopLoss));
      }
      const orderIds = await this.submitOrders(ctx, orders);
      this.printSuccess(ctx, symbolsAtStart.length > 1 ? 'Orders placed' : 'Order placed');
      return this.successWithData({ orderIds });
    }

    // Check for symbol at end: "buy $100 SYMBOL" or "buy $100 reduce SYMBOL"
    let trailingSymbols: string[] = [];
    for (let i = args.length - 1; i >= 2; i--) {
      if (args[i] === 'reduce' || args[i] === 'po' || args[i] === 'stop' || args[i] === 'tp' || args[i] === 'best')
        continue;
      // Skip the price token immediately after `stop`
      if (i > 0 && (args[i - 1] === 'stop' || args[i - 1] === 'tp')) continue;
      trailingSymbols = this.validateSymbols(exchange, args[i].split(','));
      if (trailingSymbols.length > 0) break;
    }

    if (trailingSymbols.length > 0) {
      const orders: PlaceOrderOpts[] = [];
      for (const sym of trailingSymbols) {
        const stopLoss = this.maybeStopLoss(ctx, sym, args);
        if (stopLoss instanceof Error) return this.error(stopLoss.message);
        orders.push(this.buildOrder(ctx, sym, args[1], reduceOnly, undefined, stopLoss));
      }
      const orderIds = await this.submitOrders(ctx, orders);
      this.printSuccess(ctx, trailingSymbols.length > 1 ? 'Orders placed' : 'Order placed');
      return this.successWithData({ orderIds });
    }

    // Default: use session symbol
    if (!args[1]) {
      return this.error(`Size argument missing. Usage: ${this.name} $amount`);
    }
    const stopLoss = this.maybeStopLoss(ctx, ctx.symbol, args);
    if (stopLoss instanceof Error) return this.error(stopLoss.message);
    const orderIds = await this.submitOrders(ctx, [
      this.buildOrder(ctx, ctx.symbol, args[1], reduceOnly, undefined, stopLoss),
    ]);
    this.printSuccess(ctx, 'Order placed');
    return this.successWithData({ orderIds });
  }

  /** Resolve stop-loss arg, swallowing thrown errors as Error instances
   *  so callers can short-circuit cleanly. Returns undefined if no
   *  `stop` keyword in args. */
  private maybeStopLoss(ctx: CommandContext, symbol: string, args: string[]): number | undefined | Error {
    try {
      return this.resolveBracketPrice(ctx, symbol, args);
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
        if (args[i] === 'reduce' || args[i] === 'po' || args[i] === 'best' || args[i] === 'stop' || args[i] === 'tp')
          continue;
        if (i > 0 && (args[i - 1] === 'stop' || args[i - 1] === 'tp')) continue;
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

    const orders: PlaceOrderOpts[] = [];
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
      orders.push(this.buildOrder(ctx, sym, sizeArg, reduceOnly, bestPrice, stopLoss, postOnly));
    }
    const orderIds = await this.submitOrders(ctx, orders);
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
        if (args[i] === 'reduce' || args[i] === 'po' || args[i] === 'best' || args[i] === 'stop' || args[i] === 'tp')
          continue;
        if (i > 0 && (args[i - 1] === 'stop' || args[i - 1] === 'tp')) continue;
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

    const orders: PlaceOrderOpts[] = [];
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
      orders.push(
        this.buildOrder(
          ctx,
          sym,
          args[sizeArgIndex],
          reduceOnly,
          parseFloat(this.processPrice(price, currentPrice)),
          stopLoss,
          postOnly,
        ),
      );
    }

    const orderIds = await this.submitOrders(ctx, orders);
    this.printSuccess(ctx, possibleSymbols.length > 1 ? 'Orders placed' : 'Order placed');
    return this.successWithData({ orderIds });
  }

  private buildOrder(
    ctx: CommandContext,
    symbol: string,
    sizeArg: string,
    reduceOnly: boolean,
    price?: number,
    stopLoss?: number,
    postOnly = false,
  ): PlaceOrderOpts {
    const takeProfit = this.resolveBracketPrice(ctx, symbol, this.getRawArgs(ctx), 'tp');
    return {
      symbol,
      side: this.side,
      type: price === undefined ? OrderType.Market : OrderType.Limit,
      amount: this.parseSizeForEntry(ctx.exchange!, symbol, sizeArg, reduceOnly, price),
      reduceOnly,
      ...(price !== undefined ? { price } : {}),
      timeInForce: postOnly ? OrderTimeInForce.PostOnly : OrderTimeInForce.GoodTillCancel,
      ...(stopLoss !== undefined ? { stopLoss } : {}),
      ...(takeProfit !== undefined ? { takeProfit } : {}),
    };
  }

  private async submitOrders(ctx: CommandContext, orders: PlaceOrderOpts[]): Promise<string[]> {
    const exchange = ctx.exchange;
    if (!exchange) throw new Error('Not connected to exchange');
    const placeOrder = exchange.placeOrder;
    if (typeof placeOrder !== 'function') throw new Error('Exchange does not support placing orders');
    const prepared = prepareCliOrders(exchange, orders);
    const notionals = new Map<PlaceOrderOpts, number>();
    let pendingNotional = 0;
    for (const order of prepared) {
      if (order.reduceOnly) continue;
      const blocked = this.healthGate(ctx);
      if (blocked) throw new Error(blocked);
      this.enforceFatfinger(ctx, order.symbol, order.amount);
      const notional = this.checkBudget(ctx, exchange, order.symbol, order.amount, order.price, pendingNotional);
      notionals.set(order, notional);
      pendingNotional += notional;
    }

    const ids: string[] = [];
    let accepted = 0;
    for (const order of prepared) {
      try {
        if (!order.reduceOnly) {
          const blocked = this.healthGate(ctx);
          if (blocked) throw new Error(blocked);
        }
        ids.push(...((await placeOrder.call(exchange, order)) ?? []));
      } catch (error) {
        if (accepted === 0) throw error;
        const details = serializeCommandError(error);
        // Keep native Unknown/client-ID attribution and any inner partial batch.
        // Earlier accepted legs also forbid retrying this whole command.
        throw deserializeCommandError({
          ...details,
          partialOrderIds: [...ids, ...(details.partialOrderIds ?? [])],
          partialOrderCount:
            Math.max(accepted, ids.length) +
            Math.max(details.partialOrderCount ?? 0, details.partialOrderIds?.length ?? 0),
        });
      }
      accepted++;
      const notional = notionals.get(order) ?? 0;
      if (notional > 0) ctx.preprocessStore?.addBudgetSpent(notional);
    }
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
