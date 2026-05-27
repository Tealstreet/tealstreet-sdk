import { BaseCommand } from '../BaseCommand';
import { CommandCategory, type CommandContext, type CommandResult } from '../Command';

/**
 * `untrack @<name>` — remove a single tracker.
 * `untrack all`     — clear every tracker.
 *
 * Doesn't touch static (`set`) or live (`sets`) bindings — use
 * `unset @<name>` for the kind-agnostic remove.
 */
export class UntrackCommand extends BaseCommand {
  readonly name = 'untrack';
  readonly description = 'Remove a tracker, or `untrack all`';
  readonly category = CommandCategory.Utility;
  readonly aliases: string[] = [];
  readonly examples = ['untrack @hi', 'untrack all'];
  readonly requiresConnection = false;

  async execute(ctx: CommandContext): Promise<CommandResult> {
    if (!ctx.preprocessStore) {
      return this.error('untrack: preprocess store unavailable');
    }
    const target = this.getArg(ctx, 1);
    if (!target) {
      this.printError(ctx, 'Usage: untrack @<name>  or  untrack all');
      return this.error('Missing target');
    }

    if (target === 'all') {
      ctx.preprocessStore.unsetAllTrackedVars();
      this.printSuccess(ctx, 'All trackers removed');
      return this.success();
    }

    if (!target.startsWith('@') || target.length < 2) {
      this.printError(ctx, 'Usage: untrack @<name>  or  untrack all');
      return this.error('Invalid variable name');
    }

    const name = target.slice(1);
    const removed = ctx.preprocessStore.unsetTrackedVar(name);
    if (!removed) {
      this.printWarning(ctx, `@${name} is not a tracker`);
      return this.success();
    }
    this.printSuccess(ctx, `@${name} untracked`);
    return this.success();
  }
}
