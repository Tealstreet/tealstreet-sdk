import { TradingBaseCommand } from './TradingBaseCommand';
import { CommandCategory, type CommandContext, type CommandResult } from '../Command';
import { prepareCliAmendPrice } from '../../order/prepareCliOrder';
import { OrderStatus, OrderType, OrderSide } from '@tealstreet/safe-cex/src/types';
import { formatPriceCompact } from '@tealstreet/utils/src/formatting';

/**
 * `bump <amount>` and friends — shift open limit orders by a price delta.
 *
 * Positional syntax (Phase 3):
 *   bump <amount>                                     — all sides, focused symbol
 *   bump <inst> <amount>                              — all sides, given instrument
 *   bump <buy|sell> <amount> [inst]                   — single side
 *   bump <top|bottom> <count> <amount> [inst]         — top/bottom N by price
 *   bump <buy|sell> <top|bottom> <count> <amount> [inst]
 *
 * Legacy flag syntax (kept for backwards compat):
 *   bump <amount> --buys      — only buys
 *   bump <amount> --sells     — only sells
 *
 * Stops are not affected — only `OrderType.Limit` open orders are
 * touched, matching the spec.
 */
export class BumpCommand extends TradingBaseCommand {
  readonly name = 'bump';
  readonly description = 'Move open limit orders by a price delta';
  readonly category = CommandCategory.Trading;
  readonly aliases: string[] = [];
  readonly examples = [
    'bump +10',
    'bump ETHUSDT +10',
    'bump buy +10',
    'bump sell +5 ETHUSDT',
    'bump top 3 +5',
    'bump buy bottom 5 +2 ETHUSDT',
    'bump 100 --buys',
  ];
  readonly requiresConnection = true;

  async execute(ctx: CommandContext): Promise<CommandResult> {
    if (this.simulationGate(ctx)) return this.success();
    const exchange = ctx.exchange;
    if (!exchange?.updateOrder) {
      this.printError(ctx, 'This exchange does not support order updates');
      return this.error('updateOrder not supported');
    }

    const args = this.getRawArgs(ctx);
    const assignment = this.refuseOrderAssignments(ctx);
    if (assignment) return assignment;
    let i = 1;

    // Optional leading symbol (only if there's at least one more token to consume)
    let symbol: string | null = null;
    if (
      args[i] &&
      args.length > i + 1 &&
      !['buy', 'buys', 'sell', 'sells', 'top', 'bottom'].includes(args[i].toLowerCase()) &&
      this.validateSymbols(exchange, [args[i]]).length > 0
    ) {
      symbol = this.validateSymbols(exchange, [args[i]])[0];
      i++;
    }

    // Optional side (positional)
    let side: OrderSide | null = null;
    if (args[i]?.toLowerCase() === 'buy' || args[i]?.toLowerCase() === 'buys') {
      side = OrderSide.Buy;
      i++;
    } else if (args[i]?.toLowerCase() === 'sell' || args[i]?.toLowerCase() === 'sells') {
      side = OrderSide.Sell;
      i++;
    }

    // Optional top/bottom selector
    let selector: 'top' | 'bottom' | null = null;
    let selectorCount: number | null = null;
    if (args[i]?.toLowerCase() === 'top' || args[i]?.toLowerCase() === 'bottom') {
      selector = args[i].toLowerCase() === 'top' ? 'top' : 'bottom';
      i++;
      selectorCount = this.parseOrderNumber(args[i]);
      if (!Number.isSafeInteger(selectorCount) || selectorCount <= 0) {
        this.printError(ctx, `bump ${selector}: count must be a positive integer`);
        return this.error('Invalid count');
      }
      i++;
    }

    // Amount (required)
    const amountArg = args[i];
    if (!amountArg) {
      this.printError(ctx, 'Usage: bump [<inst>] [<buy|sell>] [<top|bottom> <n>] <amount>');
      return this.error('Missing amount argument');
    }
    const amount = this.parseOrderNumber(amountArg);
    if (!Number.isFinite(amount) || amount === 0) {
      this.printError(ctx, `Invalid amount: ${amountArg}`);
      return this.error('Invalid amount');
    }
    i++;

    // Trailing symbol (if a leading one wasn't already consumed)
    if (symbol === null && args[i] && this.validateSymbols(exchange, [args[i]]).length > 0) {
      symbol = this.validateSymbols(exchange, [args[i]])[0];
      i++;
    }

    const invalid = this.refuseUnknownArgs(
      ctx,
      Array.from({ length: i }, (_, index) => index),
    );
    if (invalid) return invalid;

    const flagBuys = this.hasFlag(ctx, 'buys', 'b');
    const flagSells = this.hasFlag(ctx, 'sells', 's');
    if ((flagBuys && flagSells) || (side === OrderSide.Buy && flagSells) || (side === OrderSide.Sell && flagBuys)) {
      return this.error('Conflicting side selectors. Nothing was changed.');
    }
    if (side === null) {
      if (flagBuys) side = OrderSide.Buy;
      else if (flagSells) side = OrderSide.Sell;
    }

    const targetSymbol = symbol ?? ctx.symbol;
    const orders = exchange.store?.orders ?? [];
    let targetOrders = orders.filter(
      (o) => o.symbol === targetSymbol && o.status === OrderStatus.Open && o.type === OrderType.Limit,
    );
    if (side === OrderSide.Buy) targetOrders = targetOrders.filter((o) => o.side === OrderSide.Buy);
    else if (side === OrderSide.Sell) targetOrders = targetOrders.filter((o) => o.side === OrderSide.Sell);

    if (selector === 'top') {
      targetOrders = [...targetOrders].sort((a, b) => b.price - a.price).slice(0, selectorCount!);
    } else if (selector === 'bottom') {
      targetOrders = [...targetOrders].sort((a, b) => a.price - b.price).slice(0, selectorCount!);
    }

    if (targetOrders.length === 0) {
      this.printInfo(ctx, `No matching limit orders to bump on ${targetSymbol}`);
      return this.success();
    }

    const market = exchange.store?.markets?.find((m) => m.symbol === targetSymbol);
    const pricePrecision = market?.precision?.price;
    const direction = amount > 0 ? 'up' : 'down';
    const formattedAmount = formatPriceCompact(Math.abs(amount), pricePrecision);
    this.printInfo(
      ctx,
      `Bumping ${targetOrders.length} ${targetSymbol} order(s) ${direction} by ${formattedAmount}...`,
    );

    let successCount = 0;
    let failCount = 0;
    for (const order of targetOrders) {
      const newPrice = order.price + amount;
      if (newPrice <= 0) {
        this.printWarning(
          ctx,
          `Skipping order ${order.id}: new price would be ${formatPriceCompact(newPrice, pricePrecision)}`,
        );
        failCount++;
        continue;
      }
      try {
        await exchange.updateOrder({
          order,
          update: { price: prepareCliAmendPrice(exchange, order.symbol, newPrice) },
        });
        successCount++;
      } catch (e) {
        this.printError(ctx, `Failed to update order ${order.id}: ${(e as Error).message}`);
        failCount++;
      }
    }

    if (failCount === 0) {
      this.printSuccess(ctx, `Bumped ${successCount} order(s) ${direction} by ${formattedAmount}`);
    } else {
      this.printWarning(ctx, `Bumped ${successCount} order(s), ${failCount} failed`);
    }
    return this.success();
  }
}
