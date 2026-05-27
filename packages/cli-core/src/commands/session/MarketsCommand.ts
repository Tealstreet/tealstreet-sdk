import { BaseCommand } from '../BaseCommand';
import { CommandCategory, type CommandContext, type CommandResult } from '../Command';

/**
 * Markets command - lists available markets on the exchange
 */
export class MarketsCommand extends BaseCommand {
  readonly name = 'markets';
  readonly description = 'List available markets on the connected exchange';
  readonly category = CommandCategory.Session;
  readonly aliases = ['mkt', 'm'];
  readonly examples = ['markets'];
  readonly requiresConnection = true;

  async execute(ctx: CommandContext): Promise<CommandResult> {
    if (!ctx.exchange) {
      return this.error('Not connected to any exchange');
    }

    const markets = ctx.exchange.store?.markets || [];
    const symbols = markets.map((m) => m.symbol).sort();

    this.printInfo(ctx, `Available markets (${markets.length}):`);

    // Print in columns
    const cols = 4;
    for (let i = 0; i < symbols.length; i += cols) {
      this.print(ctx, ['  ' + symbols.slice(i, i + cols).join('\t')]);
    }

    return this.success();
  }
}
