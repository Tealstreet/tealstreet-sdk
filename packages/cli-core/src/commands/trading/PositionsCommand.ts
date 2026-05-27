import { BaseCommand } from '../BaseCommand';
import { CommandCategory, type CommandContext, type CommandResult } from '../Command';
import { PositionSide } from '@tealstreet/safe-cex/src/types';
import { formatPositionsTable } from './MonitorCommand';

/**
 * Positions command - lists open positions
 * Uses the same table format as the monitor command for consistency
 */
export class PositionsCommand extends BaseCommand {
  readonly name = 'positions';
  readonly description = 'List open positions';
  readonly category = CommandCategory.Trading;
  readonly aliases = ['ps'];
  readonly examples = ['positions', 'ps', 'positions --all', 'positions long', 'positions short'];
  readonly requiresConnection = true;

  async execute(ctx: CommandContext): Promise<CommandResult> {
    const positions = ctx.exchange?.store?.positions ?? [];
    const showAll = this.hasFlag(ctx, 'all', 'a');
    const onlyLongs = this.getArg(ctx, 1)?.toLowerCase() === 'long';
    const onlyShorts = this.getArg(ctx, 1)?.toLowerCase() === 'short';

    // Filter positions with non-zero notional
    let filteredPositions = positions.filter((p) => p.notional !== 0);

    // Filter by current symbol unless --all flag
    if (!showAll) {
      filteredPositions = filteredPositions.filter((p) => p.symbol === ctx.symbol);
    }

    // Filter by side if specified
    if (onlyLongs) {
      filteredPositions = filteredPositions.filter((p) => p.side === PositionSide.Long);
    } else if (onlyShorts) {
      filteredPositions = filteredPositions.filter((p) => p.side === PositionSide.Short);
    }

    if (filteredPositions.length === 0) {
      const filter = onlyLongs ? 'long ' : onlyShorts ? 'short ' : '';
      if (showAll) {
        this.printInfo(ctx, `No open ${filter}positions`);
      } else {
        this.printInfo(ctx, `No open ${filter}positions for ${ctx.symbol}`);
      }
      return this.success();
    }

    // Create a temporary exchange-like object with filtered positions for the formatter
    const filteredExchange = {
      ...ctx.exchange,
      store: {
        ...ctx.exchange?.store,
        positions: filteredPositions,
      },
    };

    // Use shared formatter from MonitorCommand
    const output = formatPositionsTable(filteredExchange as any);
    this.print(ctx, [output]);
    return this.success();
  }
}
