import { BaseCommand } from '../BaseCommand';
import { CommandCategory, type CommandContext, type CommandResult } from '../Command';
import { getSupportedExchangeNames, getExchangeInfo } from '../../exchanges';

/**
 * Exchanges command - lists supported exchanges
 */
export class ExchangesCommand extends BaseCommand {
  readonly name = 'exchanges';
  readonly description = 'List supported exchanges';
  readonly category = CommandCategory.Account;
  readonly aliases = ['ex'];
  readonly examples = ['exchanges'];
  readonly requiresConnection = false;

  async execute(ctx: CommandContext): Promise<CommandResult> {
    this.print(ctx, ['', 'Supported Exchanges:', '']);

    for (const name of getSupportedExchangeNames()) {
      const info = getExchangeInfo(name);
      if (info) {
        this.print(ctx, [`  ${name.padEnd(15)} ${info.displayName}`]);
        this.printInfo(ctx, `                 Required: ${info.requiredFields.join(', ')}`);
      }
    }

    this.print(ctx, ['']);
    return this.success();
  }
}
