import { TradingBaseCommand } from './TradingBaseCommand';
import { CommandCategory, type CommandContext, type CommandResult } from '../Command';
import { prepareCliOrders } from '../../order/prepareCliOrder';
import { OrderSide, OrderTimeInForce, OrderType, PositionSide } from '@tealstreet/safe-cex/src/types';
import { subtract } from '@tealstreet/safe-cex/src/utils/safe-math';

/**
 * Modifiers the scale allocator genuinely implements: `reduce`
 * (reduce-only legs, position-scoped percent sizing) and `taker`
 * (drops PostOnly so orders may cross).
 */
export const SUPPORTED_MODIFIERS: readonly string[] = ['reduce', 'taker'];

/**
 * Grammar keywords for scaling algorithms the linear allocator does not
 * implement. They remain reserved even if a market shares the name.
 */
export const RESERVED_MODIFIERS: readonly string[] = ['cubic', 'icubic', 'exponential'];

interface ParsedScaleCommand {
  allowTaker: boolean;
  reduceOnly: boolean;
  symbol?: string;
}

/**
 * Scale command - creates a set of scaled orders
 */
export class ScaleCommand extends TradingBaseCommand {
  readonly name = 'scale';
  readonly description = 'Create scaled orders';
  readonly category = CommandCategory.Trading;
  readonly aliases = [];
  readonly examples = [
    'scale buy 10 into 10 from 31000 to 32000',
    'scale sell $1000 into 10 from +0.25% to +1% ETHUSDT',
    'scale sell $10000 into 25 from 50000 to 48500 reduce',
    'scale sell $10000 into 25 from 50000 to 48500 taker',
    'scale sell 1 into 5 from +1% to 32000 reduce',
  ];
  readonly requiresConnection = true;

  /**
   * Parse scale syntax without consulting the exchange's market inventory.
   * The optional tail is any implemented modifiers followed by one
   * optional symbol scope.
   */
  private parseCommand(input: string): ParsedScaleCommand | string {
    const sizeRe = String.raw`(?:\$\d+(?:\.\d+)?|\d+(?:\.\d+)?\$|\d+(?:\.\d+)?%(?:bal|pos)?)`;
    const priceRe = String.raw`(?:[-+]?[\d.]+%?|\d+%?)`;
    const core = new RegExp(
      String.raw`^scale (?:buy|sell) ${sizeRe} into \d+ from ${priceRe} to ${priceRe}( .*)?$`,
    );
    const match = core.exec(input);
    if (!match) {
      return `Invalid syntax "${input}"`;
    }

    const tail = (match[1] ?? '').trim();
    if (!tail) {
      return { allowTaker: false, reduceOnly: false };
    }

    const tokens = tail.split(/\s+/);
    let allowTaker = false;
    let reduceOnly = false;

    while (tokens.length > 0) {
      const next = tokens[0];
      if (RESERVED_MODIFIERS.includes(next)) {
        return `modifier '${next}' is not implemented; supported: ${SUPPORTED_MODIFIERS.join(', ')}`;
      }
      if (!SUPPORTED_MODIFIERS.includes(next)) break;

      tokens.shift();
      if (next === 'reduce') {
        if (reduceOnly) return `modifier 'reduce' specified more than once`;
        reduceOnly = true;
      } else if (next === 'taker') {
        if (allowTaker) return `modifier 'taker' specified more than once`;
        allowTaker = true;
      }
    }

    const symbol = tokens.shift();
    if (symbol && (SUPPORTED_MODIFIERS.includes(symbol) || RESERVED_MODIFIERS.includes(symbol))) {
      return `modifier '${symbol}' is not implemented; supported: ${SUPPORTED_MODIFIERS.join(', ')}`;
    }
    if (tokens.length > 0) {
      return `modifier '${symbol}' is not implemented; supported: ${SUPPORTED_MODIFIERS.join(', ')}`;
    }
    return { allowTaker, reduceOnly, ...(symbol ? { symbol } : {}) };
  }

  async execute(ctx: CommandContext): Promise<CommandResult> {
    if (this.simulationGate(ctx)) return this.success();
    const exchange = ctx.exchange;
    if (!exchange) {
      return this.error('Not connected to exchange');
    }
    if (!exchange.placeOrders) {
      return this.error('Exchange does not support placing multiple orders');
    }

    const args = this.getRawArgs(ctx);
    const commandStr = args.join(' ');
    const parsedCommand = this.parseCommand(commandStr);
    if (typeof parsedCommand === 'string') {
      return this.error(parsedCommand);
    }

    let targetSymbol = ctx.symbol;
    if (parsedCommand.symbol) {
      const [resolvedSymbol] = this.validateSymbols(exchange, [parsedCommand.symbol]);
      // An explicit scope must be a canonical market symbol. validateSymbols
      // also supports fuzzy base-token resolution, which must not turn a typo
      // such as BTCUSDX into BTCUSDT.
      if (!resolvedSymbol || resolvedSymbol.toUpperCase() !== parsedCommand.symbol.toUpperCase()) {
        return this.error(
          `Unknown symbol "${parsedCommand.symbol}". Use a valid market symbol, or omit it to trade the focused symbol (${ctx.symbol}).`,
        );
      }
      targetSymbol = resolvedSymbol;
    }

    const { allowTaker, reduceOnly } = parsedCommand;

    // Health gate — block opening exposure when the private stream is
    // degraded; reduce-only scales reduce risk and stay allowed. Gated
    // after syntax validation so a malformed scale surfaces its parse
    // error rather than the degraded message (TSBUG-106).
    if (!reduceOnly) {
      const blocked = this.healthGate(ctx);
      if (blocked) return this.error(blocked);
    }

    const orderSide = args[1] === 'buy' ? OrderSide.Buy : OrderSide.Sell;
    // Match the buy/sell vs close convention from Phase 0 decision 1:
    // entries (non-reduce) take % of balance, exits (reduce) take % of
    // position. Without `allowPercentage=true` the regex-vs-parser
    // layer mismatch surfaced as confusing throws — see the test
    // `regex accepts percent-size, parser used to reject it`.
    let totalSize: number;
    try {
      totalSize = this.parseSizeAndConvertIfUsd(
        exchange,
        targetSymbol,
        args[2],
        true,
        reduceOnly ? (orderSide === OrderSide.Buy ? PositionSide.Short : PositionSide.Long) : undefined,
        reduceOnly ? 'position' : 'balance',
      );
    } catch (e) {
      return this.error((e as Error).message);
    }
    const ordersCount = parseInt(args[4], 10);

    const currentPrice = exchange.store?.tickers?.find((ticker) => ticker.symbol === targetSymbol)?.last;
    if (!currentPrice) {
      return this.error(`Current price is not available for ticker ${targetSymbol}, try again.`);
    }

    const from = this.processPrice(args[6], currentPrice);
    const to = this.processPrice(args[8], currentPrice);
    const priceUpper = Math.max(parseFloat(from), parseFloat(to));
    const priceLower = Math.min(parseFloat(to), parseFloat(from));
    const priceIncrement = subtract(priceUpper, priceLower) / subtract(ordersCount, 1);
    const orderSize = totalSize / ordersCount;

    const orders = [];
    for (let i = 0; i < ordersCount; i++) {
      const price = priceLower + i * priceIncrement;
      orders.push({
        size: orderSize,
        price: String(price),
        side: orderSide,
        reduceOnly: reduceOnly,
      });
    }

    // Fatfinger gate — applies per-leg, not per-total. A scale command
    // that adds up to MORE than the limit but where each leg is
    // smaller is allowed (the cap protects against single fat-finger
    // entries, not aggregate exposure).
    // Budget is aggregate — the whole ladder's notional, charged once
    // AFTER the orders place (checked before so we fail closed early).
    let budgetNotional = 0;
    if (!reduceOnly) {
      try {
        for (const order of orders) {
          this.enforceFatfinger(ctx, targetSymbol, order.size);
        }
        const totalAmount = orders.reduce((sum, o) => sum + Math.abs(o.size), 0);
        budgetNotional = this.checkBudget(ctx, exchange, targetSymbol, totalAmount);
      } catch (e) {
        return this.error((e as Error).message);
      }
    }

    const orderIds =
      (await exchange.placeOrders(
        prepareCliOrders(
          exchange,
          orders.map((order) => ({
            side: order.side,
            symbol: targetSymbol,
            price: parseFloat(order.price),
            type: OrderType.Limit,
            amount: order.size,
            ...(!allowTaker ? { timeInForce: OrderTimeInForce.PostOnly } : {}),
            reduceOnly: order.reduceOnly,
          })),
        ),
      )) ?? [];

    if (budgetNotional > 0) ctx.preprocessStore?.addBudgetSpent(budgetNotional);
    this.printSuccess(ctx, `Created orders ${orderIds.join(',')}`);
    return this.successWithData({ orderIds });
  }
}
