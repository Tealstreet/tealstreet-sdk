import { BaseCommand } from '../BaseCommand';
import { CommandCategory, type CommandContext, type CommandResult } from '../Command';
import { parseDuration } from '../../preprocess/sleep';

/**
 * `sleep <duration>` — pause for the given duration. Accepts `ms`, `s`
 * (default), `m`. Hard-capped at 10 minutes.
 *
 * The preprocess layer marks `sleep` segments with `block: true` so the
 * runner awaits this command's promise instead of fire-and-forget. That
 * is what gives `repeat 5 (buy 1; sleep 1m)` its pacing — every other
 * command in the chain is fire-and-forget, only `sleep` blocks.
 */
export class SleepCommand extends BaseCommand {
  readonly name = 'sleep';
  readonly description = 'Pause for ms / s / m (default s, max 10m)';
  readonly category = CommandCategory.Utility;
  readonly aliases: string[] = [];
  readonly examples = ['sleep 5', 'sleep 250ms', 'sleep 30s', 'sleep 2m'];
  readonly requiresConnection = false;

  async execute(ctx: CommandContext): Promise<CommandResult> {
    const arg = this.getArg(ctx, 1);
    if (!arg) {
      this.printError(ctx, 'Usage: sleep <duration> (e.g. 5s, 250ms, 2m)');
      return this.error('Missing duration');
    }
    let ms: number;
    try {
      ms = parseDuration(arg);
    } catch (e) {
      this.printError(ctx, (e as Error).message);
      return this.error((e as Error).message);
    }
    await new Promise<void>((resolve) => setTimeout(resolve, ms));
    return this.success();
  }
}
