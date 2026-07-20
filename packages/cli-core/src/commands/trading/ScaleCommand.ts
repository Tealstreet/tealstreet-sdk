import { TradingBaseCommand } from './TradingBaseCommand';
import { CommandCategory, type CommandContext, type CommandResult } from '../Command';
import { OrderSide, OrderTimeInForce, OrderType, PositionSide } from '@tealstreet/safe-cex/src/types';
import { adjust, subtract } from '@tealstreet/safe-cex/src/utils/safe-math';

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
    'scale buy $1000 into 10 from -0.25% to -1% ETHUSDT',
    'scale sell $10000 into 25 from 50000 to 48500 reduce',
    'scale sell $10000 into 25 from 50000 to 48500 reduce taker',
    'scale sell 1 into 5 from +1% to 32000 reduce',
  ];
  readonly requiresConnection = true;

  private validateCommand(input: string): boolean {
    // Size: $N (prefix), N$ (suffix), N%, N%bal, N%pos. Mirrors
    // Phase 0 decision 1's accepted forms; bare numbers are rejected.
    // Prices: absolute, relative ([+-]N), or percent (N%). Optional
    // trailing keywords: reduce / cubic / icubic / taker plus an
    // optional symbol scope.
    const sizeRe = String.raw`(?:\$\d+(?:\.\d+)?|\d+(?:\.\d+)?\$|\d+(?:\.\d+)?%(?:bal|pos)?)`;
    const priceRe = String.raw`(?:[-+]?[\d.]+%?|\d+%?)`;
    const regex = new RegExp(
      String.raw`^scale (buy|sell) (${sizeRe}) into (\d+) from (${priceRe}) to (${priceRe})(?: (reduce|cubic|icubic|taker)(?: ([-+]?[\d.]+))?)?( [A-Za-z]+)?$`,
    );
    return regex.test(input);
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

    if (!this.validateCommand(commandStr)) {
      return this.error(`Invalid syntax "${commandStr}"`);
    }

    // Health gate — block opening exposure when the private stream is
    // degraded; reduce-only scales reduce risk and stay allowed. Gated
    // after syntax validation so a malformed scale surfaces its parse
    // error rather than the degraded message (TSBUG-106).
    if (!this.hasArgValue(args, 'reduce')) {
      const blocked = this.healthGate(ctx);
      if (blocked) return this.error(blocked);
    }

    const targetSymbol = this.getTargetSymbol(ctx, [
      'buy',
      'sell',
      'into',
      'from',
      'to',
      'reduce',
      'cubic',
      'icubic',
      'taker',
    ]);

    const orderSide = args[1] === 'buy' ? OrderSide.Buy : OrderSide.Sell;
    const reduceOnly = this.hasArgValue(args, 'reduce');
    const allowTaker = this.hasArgValue(args, 'taker');
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

    const market = exchange.store?.markets?.find((market) => market.symbol === targetSymbol);

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
        orders.map((order) => ({
          side: order.side,
          symbol: targetSymbol,
          price: adjust(parseFloat(order.price), market?.precision.price || 0),
          type: OrderType.Limit,
          amount: adjust(order.size, market?.precision.amount || 0),
          ...(!allowTaker ? { timeInForce: OrderTimeInForce.PostOnly } : {}),
          reduceOnly: order.reduceOnly,
        })),
      )) ?? [];

    if (budgetNotional > 0) ctx.preprocessStore?.addBudgetSpent(budgetNotional);
    this.printSuccess(ctx, `Created orders ${orderIds.join(',')}`);
    return this.successWithData({ orderIds });
  }
}
