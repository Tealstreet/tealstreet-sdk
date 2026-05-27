import { BaseCommand } from '../BaseCommand';
import { CommandCategory, type CommandContext, type CommandResult } from '../Command';

const DEFAULT_LIMIT = 50;
const MAX_LIMIT = 200;

/**
 * `history` / `history N` — print recent commands the user has
 * typed (newest first). Source is the adapter's
 * `getCommandHistory(limit?)` — each runtime plugs in its own
 * backing store (standalone CLI: in-memory ring buffer in repl.ts;
 * web terminal: per-tab `cliHistoryAtom`).
 *
 * Defaults to the last 50 entries, hard-capped at 200. Output is
 * a flat list of commands — no timestamps for v1 (the existing
 * input-history sources don't carry them). If a future runtime
 * starts capturing timestamps, the adapter signature is forward-
 * compatible: just return an enriched string from
 * `getCommandHistory`.
 */
export class HistoryCommand extends BaseCommand {
  readonly name = 'history';
  readonly description = 'Show recent commands';
  readonly category = CommandCategory.Utility;
  readonly aliases: string[] = [];
  readonly examples = ['history', 'history 20'];
  readonly requiresConnection = false;

  async execute(ctx: CommandContext): Promise<CommandResult> {
    const getter = ctx.adapter.getCommandHistory;
    if (!getter) {
      this.printInfo(ctx, 'history not available in this runtime');
      return this.success();
    }
    const limitArg = this.getArg(ctx, 1);
    let limit = DEFAULT_LIMIT;
    if (limitArg !== undefined) {
      const n = parseInt(limitArg, 10);
      if (!Number.isFinite(n) || n <= 0) {
        this.printError(ctx, `Usage: history [N]   (N is a positive integer, max ${MAX_LIMIT})`);
        return this.error('Invalid limit');
      }
      limit = Math.min(n, MAX_LIMIT);
    }
    const entries = getter(limit);
    if (entries.length === 0) {
      this.printInfo(ctx, '(no history)');
      return this.success();
    }
    this.print(ctx, entries);
    return this.success();
  }
}
