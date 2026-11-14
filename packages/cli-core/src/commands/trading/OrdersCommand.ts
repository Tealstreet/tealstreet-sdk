import { BaseCommand } from '../BaseCommand';
import { CommandCategory, type CommandContext, type CommandResult } from '../Command';
import { OrderStatus } from '@tealstreet/safe-cex/src/types';
import { formatOrdersTable } from './MonitorCommand';

/**
 * Orders command - lists open orders
 * Uses the same table format as the monitor command for consistency
 */
export class OrdersCommand extends BaseCommand {
  readonly name = 'orders';
  readonly description = 'List open orders';
  readonly category = CommandCategory.Trading;
  readonly aliases = ['o'];
  readonly examples = ['orders', 'o', 'orders all', 'orders --all'];
  readonly requiresConnection = true;

  async execute(ctx: CommandContext): Promise<CommandResult> {
    const orders = ctx.exchange?.store?.orders ?? [];
    const showAll = this.hasFlag(ctx, 'all', 'a') || this.getArg(ctx, 1)?.toLowerCase() === 'all';

    // Filter to current symbol unless --all flag
    const filteredOrders = showAll ? orders : orders.filter((o) => o.symbol === ctx.symbol);

    // Only show open/creating orders
    const openOrders = filteredOrders.filter(
      (o) => o.status === OrderStatus.Open || o.status === OrderStatus.Creating,
    );

    if (openOrders.length === 0) {
      if (showAll) {
        this.printInfo(ctx, 'No open orders');
      } else {
        this.printInfo(ctx, `No open orders for ${ctx.symbol}`);
      }
      return this.success();
    }

    // Create a temporary exchange-like object with filtered orders for the formatter
    const filteredExchange = {
      ...ctx.exchange,
      store: {
        // SDK stores expose markets through a prototype getter, which object spread drops.
        markets: ctx.exchange?.store?.markets,
        orders: openOrders,
      },
    };

    // Use shared formatter from MonitorCommand
    const output = formatOrdersTable(filteredExchange as any);
    this.print(ctx, [output]);
    return this.success();
  }
}
