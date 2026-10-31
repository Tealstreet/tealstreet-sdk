import { BaseCommand } from '../BaseCommand';
import { CommandCategory, type CommandContext, type CommandResult } from '../Command';
import { prepareCliAmendPrice } from '../../order/prepareCliOrder';
import { OrderStatus, OrderType } from '@tealstreet/safe-cex/src/types';
import { formatPriceCompact } from '@tealstreet/utils/src/formatting';

/**
 * Move command - moves a specific order to a new price
 *
 * Usage:
 *   move <order_id> <new_price>   - Move order to absolute price
 *   move <order_id> +10           - Move order up by 10
 *   move <order_id> -5            - Move order down by 5
 */
export class MoveCommand extends BaseCommand {
  readonly name = 'move';
  readonly description = 'Move an order to a new price';
  readonly category = CommandCategory.Trading;
  readonly aliases = ['mv'];
  readonly examples = ['move abc123 50000', 'move abc123 +10'];
  readonly requiresConnection = true;

  async execute(ctx: CommandContext): Promise<CommandResult> {
    if (this.simulationGate(ctx)) return this.success();
    const orderIdArg = this.getArg(ctx, 1);
    const priceArg = this.getArg(ctx, 2);

    if (!orderIdArg || !priceArg) {
      this.printError(ctx, 'Usage: move <order_id> <new_price|+/-offset>');
      this.printInfo(ctx, 'Example: move abc123 50000');
      this.printInfo(ctx, 'Example: move abc +10 (relative move)');
      return this.error('Missing arguments');
    }

    // Check if exchange supports updateOrder
    if (!ctx.exchange?.updateOrder) {
      this.printError(ctx, 'This exchange does not support order updates');
      return this.error('updateOrder not supported');
    }

    const orders = ctx.exchange?.store?.orders ?? [];

    // Find the order - match by ID prefix (for convenience)
    const matchingOrders = orders.filter(
      (o) =>
        (o.id === orderIdArg || o.id.startsWith(orderIdArg)) &&
        o.status === OrderStatus.Open &&
        o.type === OrderType.Limit,
    );

    if (matchingOrders.length === 0) {
      this.printError(ctx, `No open limit order found matching: ${orderIdArg}`);
      return this.error('Order not found');
    }

    if (matchingOrders.length > 1) {
      // Get market for price precision when displaying multiple matches
      const markets = ctx.exchange?.store?.markets ?? [];
      this.printError(ctx, `Multiple orders match "${orderIdArg}". Please be more specific:`);
      for (const o of matchingOrders) {
        const market = markets.find((m) => m.symbol === o.symbol);
        const formattedPrice = formatPriceCompact(o.price, market?.precision?.price);
        this.printInfo(ctx, `  ${o.id} - ${o.symbol} ${o.side} @ ${formattedPrice}`);
      }
      return this.error('Ambiguous order ID');
    }

    const order = matchingOrders[0];

    // Get market for price precision
    const markets = ctx.exchange?.store?.markets ?? [];
    const market = markets.find((m) => m.symbol === order.symbol);
    const pricePrecision = market?.precision?.price;

    // Parse the price - can be absolute (50000) or relative (+10, -5)
    let newPrice: number;
    if (priceArg.startsWith('+') || priceArg.startsWith('-')) {
      const offset = parseFloat(priceArg);
      if (isNaN(offset)) {
        this.printError(ctx, `Invalid price offset: ${priceArg}`);
        return this.error('Invalid price');
      }
      newPrice = order.price + offset;
    } else {
      newPrice = parseFloat(priceArg);
      if (isNaN(newPrice)) {
        this.printError(ctx, `Invalid price: ${priceArg}`);
        return this.error('Invalid price');
      }
    }

    // Validate new price is positive
    if (newPrice <= 0) {
      this.printError(ctx, `Invalid price: ${formatPriceCompact(newPrice, pricePrecision)} (must be positive)`);
      return this.error('Invalid price');
    }

    // Same price check
    if (newPrice === order.price) {
      this.printInfo(ctx, 'Order is already at this price');
      return this.success();
    }

    // Format prices for display
    const oldPriceStr = formatPriceCompact(order.price, pricePrecision);
    const newPriceStr = formatPriceCompact(newPrice, pricePrecision);

    // Update the order
    this.printInfo(ctx, `Moving order ${order.id.substring(0, 8)}... from ${oldPriceStr} to ${newPriceStr}`);

    try {
      await ctx.exchange.updateOrder({
        order,
        update: { price: prepareCliAmendPrice(ctx.exchange, order.symbol, newPrice) },
      });
      this.printSuccess(ctx, `Order moved to ${newPriceStr}`);
      return this.success();
    } catch (error) {
      this.printError(ctx, `Failed to move order: ${(error as Error).message}`);
      return this.error((error as Error).message);
    }
  }
}
