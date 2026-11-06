import { validateLiveVarSource } from '../../preprocess/reactiveVars';
import { firstDashToken } from '../parseArgs';
import { BaseCommand } from '../BaseCommand';
import { CommandCategory, type CommandContext, type CommandResult } from '../Command';

/**
 * `sets @<name> <source>` — create or update a *reactive* (live)
 * variable. Unlike `set` which snapshots the value at definition time,
 * `sets` stores the SOURCE expression and re-evaluates it on every
 * `@<name>` use.
 *
 * Source can be any single-token value the substitute layer can produce
 * — `$last`, `$ask`, `$mark`, `$entry`, `$pnl`, …, a math expression
 * (`$ask-$bid`, `@mul*2`), a price-anchor with offset (`$entry +1%`),
 * or a bare numeric literal.
 *
 * Mutually exclusive with `set` on the same name — calling `sets @x …`
 * after `set @x …` clears the static binding (and vice versa). The
 * storage layer enforces this; the substitute layer trusts there's
 * at most one binding per name.
 *
 * Session-scoped: live bindings do NOT persist across CLI sessions
 * (no `live-vars.json`). Re-type the `sets` next session.
 *
 * `sets` is preprocess-passthrough — the source must NOT be substituted
 * at definition time, otherwise we'd freeze the value and defeat
 * reactivity. The FULL_PASSTHROUGH_COMMANDS set in substitute.ts
 * carries this.
 */
export class SetsCommand extends BaseCommand {
  readonly name = 'sets';
  readonly description = 'Create or update a reactive (live) variable';
  readonly category = CommandCategory.Utility;
  readonly aliases: string[] = [];
  readonly examples = [
    'sets @spread $ask-$bid',
    'sets @above $entry +1%',
    'sets @doubled @x*2',
  ];
  readonly requiresConnection = false;

  async execute(ctx: CommandContext): Promise<CommandResult> {
    if (!ctx.preprocessStore) {
      return this.error('sets: preprocess store unavailable');
    }
    // parseArgs would drop it, so `$entry -1.5%` would store `$entry` with no offset.
    const dashToken = firstDashToken(ctx.raw);
    if (dashToken) {
      const message = `sets: refusing "${dashToken}": flags and negative values are not supported, nothing was stored`;
      this.printError(ctx, message);
      return this.error(message);
    }
    const target = this.getArg(ctx, 1);
    if (!target || !target.startsWith('@') || target.length < 2) {
      this.printError(ctx, 'Usage: sets @<name> <source>');
      return this.error('Invalid variable name');
    }
    const name = target.slice(1);
    const source = ctx.args._.slice(2).join(' ').trim();
    if (!source) {
      this.printError(ctx, 'Usage: sets @<name> <source>');
      return this.error('Missing source');
    }
    const validationError = validateLiveVarSource(source);
    if (validationError !== null) {
      this.printError(ctx, `sets: ${validationError}`);
      return this.error(validationError);
    }
    ctx.preprocessStore.setLiveVar(name, source);
    this.printSuccess(ctx, `@${name} (live) = ${source}`);
    return this.success();
  }
}
