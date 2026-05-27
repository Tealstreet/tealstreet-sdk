import { BaseCommand } from '../BaseCommand';
import { CommandCategory, type CommandContext, type CommandResult } from '../Command';

/**
 * `unalias <name>` — remove an alias.
 * `unalias all`    — clear every alias.
 */
export class UnaliasCommand extends BaseCommand {
  readonly name = 'unalias';
  readonly description = 'Remove an alias, or `unalias all`';
  readonly category = CommandCategory.Utility;
  readonly aliases: string[] = [];
  readonly examples = ['unalias b1', 'unalias all'];
  readonly requiresConnection = false;

  async execute(ctx: CommandContext): Promise<CommandResult> {
    if (!ctx.preprocessStore) {
      return this.error('unalias: preprocess store unavailable');
    }
    const target = this.getArg(ctx, 1);
    if (!target) {
      this.printError(ctx, 'Usage: unalias <name>  or  unalias all');
      return this.error('Missing target');
    }
    if (target === 'all') {
      ctx.preprocessStore.unsetAllAliases();
      this.printSuccess(ctx, 'All aliases removed');
      return this.success();
    }
    const removed = ctx.preprocessStore.unsetAlias(target);
    if (!removed) {
      this.printWarning(ctx, `${target} is not an alias`);
      return this.success();
    }
    this.printSuccess(ctx, `${target} unaliased`);
    return this.success();
  }
}
