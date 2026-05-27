import { BaseCommand } from '../BaseCommand';
import { CommandCategory, type CommandContext, type CommandResult } from '../Command';

/**
 * `set @<name> <value>` — create or update a user variable.
 *
 * The preprocess layer leaves the `@<name>` token literal but substitutes
 * the value tokens, so `set @x $entry` snapshots the current entry price
 * rather than storing the literal `$entry`.
 */
export class SetCommand extends BaseCommand {
  readonly name = 'set';
  readonly description = 'Create or update a user variable';
  readonly category = CommandCategory.Utility;
  readonly aliases: string[] = [];
  readonly examples = ['set @size 100$', 'set @entry $entry', 'set @stop $entry -100'];
  readonly requiresConnection = false;

  async execute(ctx: CommandContext): Promise<CommandResult> {
    if (!ctx.preprocessStore) {
      return this.error('set: preprocess store unavailable');
    }
    const target = this.getArg(ctx, 1);
    if (!target || !target.startsWith('@') || target.length < 2) {
      this.printError(ctx, 'Usage: set @<name> <value>');
      return this.error('Invalid variable name');
    }
    const name = target.slice(1);
    const value = ctx.args._.slice(2).join(' ').trim();
    if (!value) {
      this.printError(ctx, 'Usage: set @<name> <value>');
      return this.error('Missing value');
    }
    ctx.preprocessStore.setVar(name, value);
    this.printSuccess(ctx, `@${name} = ${value}`);
    return this.success();
  }
}
