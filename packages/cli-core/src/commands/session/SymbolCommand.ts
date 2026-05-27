import { BaseCommand } from '../BaseCommand';
import { CommandCategory, type CommandContext, type CommandResult } from '../Command';

/**
 * Symbol command - changes the current trading symbol
 */
export class SymbolCommand extends BaseCommand {
  readonly name = 'symbol';
  readonly description = 'Change the current trading symbol';
  readonly category = CommandCategory.Session;
  readonly aliases = ['sym', 'switch'];
  readonly examples = ['symbol BTCUSDT', 'switch ETH', 'sym BTC'];
  readonly requiresConnection = false;

  async execute(ctx: CommandContext): Promise<CommandResult> {
    const newSymbol = this.getArg(ctx, 1)?.toUpperCase();

    if (!newSymbol) {
      this.printInfo(ctx, `Current symbol: ${ctx.symbol}`);
      this.printInfo(ctx, 'Usage: symbol <symbol>');
      return this.success();
    }

    ctx.adapter.setDefaultSymbol(newSymbol);
    this.printInfo(ctx, `Symbol set to ${newSymbol}`);

    return {
      success: true,
      updateSession: {
        symbol: newSymbol,
      },
    };
  }
}
