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
  unexpectedArgument?: string;
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
    const core = new RegExp(String.raw`^scale (?:buy|sell) ${sizeRe} into \d+ from ${priceRe} to ${priceRe}( .*)?$`);
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
    return { allowTaker, reduceOnly, symbol, unexpectedArgument: tokens[0] };
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
    this.assertUnambiguousOrderModifiers(
      ctx,
      -1,
      SUPPORTED_MODIFIERS,
      { into: 1, from: 1, to: 1 },
      args.indexOf('to') + 1,
    );
    const assignment = this.refuseOrderAssignments(ctx);
    if (assignment) return assignment;
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
          `${this.unknownOrderArgument(parsedCommand.symbol)} Unknown symbol "${parsedCommand.symbol}". Use a valid market symbol, or omit it to trade the focused symbol (${ctx.symbol}).`,
        );
      }
      targetSymbol = resolvedSymbol;
    }

    if (parsedCommand.unexpectedArgument) {
      return this.error(this.unknownOrderArgument(parsedCommand.unexpectedArgument));
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

    // Split the request before lot rounding. Dollar/balance shares convert at
    // each rung's price; position/inverse shares split in native contracts.
    // Floor each share so rounding cannot inflate the allocation.
    const orders = [];
    try {
      for (let i = 0; i < ordersCount; i++) {
        const price = priceLower + i * priceIncrement;
        const size = this.parseSizeAndConvertIfUsd(
          exchange,
          targetSymbol,
          args[2],
          true,
          reduceOnly ? (orderSide === OrderSide.Buy ? PositionSide.Short : PositionSide.Long) : undefined,
          reduceOnly ? 'position' : 'balance',
          price,
          1 / ordersCount,
          true,
        );
        orders.push({
          size,
          price: String(price),
          side: orderSide,
          reduceOnly: reduceOnly,
        });
      }
    } catch (e) {
      return this.error((e as Error).message);
    }

    const preparedOrders = prepareCliOrders(
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
    );

    // Check each prepared rung at its own rounded price before the batch write.
    // Retain the combined notional and charge it only after acceptance.
    let budgetNotional = 0;
    if (!reduceOnly) {
      try {
        for (const order of preparedOrders) {
          this.enforceFatfinger(ctx, order.symbol, order.amount);
          budgetNotional += this.checkBudget(ctx, exchange, order.symbol, order.amount, order.price, budgetNotional);
        }
      } catch (e) {
        return this.error((e as Error).message);
      }
    }

    const orderIds = (await exchange.placeOrders(preparedOrders)) ?? [];

    if (budgetNotional > 0) ctx.preprocessStore?.addBudgetSpent(budgetNotional);
    this.printSuccess(ctx, `Created orders ${orderIds.join(',')}`);
    return this.successWithData({ orderIds });
  }
}
