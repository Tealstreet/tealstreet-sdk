import { BaseCommand } from '../BaseCommand';
import { CommandCategory, type CommandContext, type CommandResult } from '../Command';

/**
 * Status command - shows current connection status
 */
export class StatusCommand extends BaseCommand {
  readonly name = 'status';
  readonly description = 'Show current connection status';
  readonly category = CommandCategory.Session;
  readonly aliases = ['st'];
  readonly examples = ['status'];
  readonly requiresConnection = false;

  async execute(ctx: CommandContext): Promise<CommandResult> {
    this.printInfo(ctx, `Account: ${ctx.accountName || 'Not connected'}`);
    this.printInfo(ctx, `Exchange: ${ctx.exchange?.name || 'Not connected'}`);
    this.printInfo(ctx, `Symbol: ${ctx.symbol}`);

    return this.success();
  }
}
