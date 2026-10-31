import { firstDashToken } from '../parseArgs';
import { validateLiveVarSource } from '../../preprocess/reactiveVars';
import { BaseCommand } from '../BaseCommand';
import { CommandCategory, type CommandContext, type CommandResult } from '../Command';

/**
 * `track max @<name> <source>` / `track min @<name> <source>` —
 * register a high-water (max) / low-water (min) aggregator over
 * the given source expression. The stored value is the most
 * extreme value of the source seen since the tracker was
 * registered; each `@<name>` substitution re-evaluates the source
 * and updates the extreme if the new value is more extreme.
 *
 * `track list` — print every registered tracker.
 *
 * Source vocabulary: same as `sets` (live vars). Any `$constant`,
 * `@var`, numeric literal, math expression, or price-anchor +
 * offset. Garbage rejected at definition time.
 *
 * Mutex with `set` and `sets` — registering a tracker clears any
 * same-name static or live binding. The storage layer enforces it.
 *
 * Session-scoped: trackers do NOT persist across CLI sessions.
 * A high-water mark from yesterday isn't meaningful in a new
 * session — re-register with the same `track max` command.
 *
 * `track` is preprocess-passthrough — the source MUST NOT be
 * substituted at definition time, otherwise we'd freeze the
 * source's CURRENT value as the entire source expression. The
 * FULL_PASSTHROUGH_COMMANDS set in substitute.ts carries this.
 */
export class TrackCommand extends BaseCommand {
  readonly name = 'track';
  readonly description = 'Track the high/low-water mark of a source expression';
  readonly category = CommandCategory.Utility;
  readonly aliases: string[] = [];
  readonly examples = [
    'track max @hi $pnl',
    'track min @lo $pnl',
    'track max @maxSpread $ask-$bid',
    'track list',
  ];
  readonly requiresConnection = false;

  async execute(ctx: CommandContext): Promise<CommandResult> {
    if (!ctx.preprocessStore) {
      return this.error('track: preprocess store unavailable');
    }
    // parseArgs would drop it, so `$last -2%` would track `$last` with no offset.
    const dashToken = firstDashToken(ctx.raw);
    if (dashToken) {
      const message = `track: refusing "${dashToken}": flags and negative values are not supported, nothing was stored`;
      this.printError(ctx, message);
      return this.error(message);
    }
    const sub = (this.getArg(ctx, 1) ?? '').toLowerCase();

    if (sub === 'list') {
      return this.handleList(ctx);
    }

    if (sub !== 'max' && sub !== 'min') {
      this.printError(ctx, 'Usage: track max|min @<name> <source>  or  track list');
      return this.error('Invalid subcommand');
    }

    const target = this.getArg(ctx, 2);
    if (!target || !target.startsWith('@') || target.length < 2) {
      this.printError(ctx, `Usage: track ${sub} @<name> <source>`);
      return this.error('Invalid variable name');
    }
    const name = target.slice(1);
    const source = ctx.args._.slice(3).join(' ').trim();
    if (!source) {
      this.printError(ctx, `Usage: track ${sub} @<name> <source>`);
      return this.error('Missing source');
    }
    const validationError = validateLiveVarSource(source);
    if (validationError !== null) {
      this.printError(ctx, `track: ${validationError}`);
      return this.error(validationError);
    }
    ctx.preprocessStore.setTrackedVar(name, sub, source);
    this.printSuccess(ctx, `@${name} (track ${sub}) = ${source}`);
    return this.success();
  }

  private handleList(ctx: CommandContext): CommandResult {
    const trackers = ctx.preprocessStore!.getTrackedVars();
    const entries = Object.entries(trackers);
    if (entries.length === 0) {
      this.printInfo(ctx, '(no trackers set)');
      return this.success();
    }
    // Sort alphabetically. Each line shows kind, source, and
    // current value (or `(unseeded)` if the tracker hasn't been
    // resolved yet — i.e. nobody has referenced @<name> since it
    // was registered).
    const lines = entries
      .sort(([a], [b]) => a.localeCompare(b))
      .map(([k, { kind, source, value }]) => {
        const valueStr = value === undefined ? '(unseeded)' : String(value);
        return `@${k} (track ${kind}) = ${source}  →  ${valueStr}`;
      });
    this.print(ctx, lines);
    return this.success();
  }
}
