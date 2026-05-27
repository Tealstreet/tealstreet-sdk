import { BaseCommand } from '../BaseCommand';
import { CommandCategory, type CommandContext, type CommandResult } from '../Command';
import { formatMarginTable } from './MonitorCommand';

/**
 * Margin command - shows account margin/balance info
 * Uses the same table format as the monitor command for consistency
 */
export class MarginCommand extends BaseCommand {
  readonly name = 'margin';
  readonly description = 'Show account margin/balance';
  readonly category = CommandCategory.Trading;
  readonly aliases = ['balance', 'bal'];
  readonly examples = ['margin', 'balance', 'bal'];
  readonly requiresConnection = true;

  async execute(ctx: CommandContext): Promise<CommandResult> {
    const balance = ctx.exchange?.store?.balance;

    if (!balance) {
      this.printError(ctx, 'No balance data available');
      return this.success();
    }

    // Use shared formatter from MonitorCommand
    const output = formatMarginTable(ctx.exchange as any);
    this.print(ctx, [output]);
    return this.success();
  }
}
