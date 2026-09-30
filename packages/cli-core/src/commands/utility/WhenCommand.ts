import { BaseCommand } from '../BaseCommand';
import { CommandCategory, type CommandContext, type CommandResult } from '../Command';
import { parseConditionText } from '../../preprocess/conditions';
import { peelParenBlock } from '../../preprocess/parserUtils';
import { armTrigger, currentTriggerDepth, TriggerDepthExceededError } from '../../triggers/triggerRegistry';

/**
 * `when (<condition>) (<command>)` — arm a command to fire once, later.
 *
 * Asked for as "would like to utilise wait so then TP1 is hit, price moves to
 * Entry" and "after TP3, would like to have a runner" (Discord, 2026-09-29).
 * Both are the same missing shape: a condition that stays armed while the user
 * keeps trading. `wait` cannot be it — it holds the line it is typed on and is
 * capped at ten minutes, and a take-profit can sit unfilled for hours.
 *
 * **The parens are not decoration.** A condition is variable-length
 * (`price > 50000`, `fill BTCUSDT`, `both (A) and (B)`) and a command is too,
 * so there is no delimiter a heuristic could find. `if (<cond>) (<then>)`
 * already solved this in the preprocess layer; this borrows its shape and its
 * `peelParenBlock`, and `splitChain` is paren-aware, so `;` inside the action
 * belongs to the action rather than to the outer chain.
 *
 * Parsed from `ctx.raw` because `parseArgs` splits on whitespace and loses the
 * grouping.
 */
export class WhenCommand extends BaseCommand {
  readonly name = 'when';
  readonly description = 'Arm a command to run once, when a condition becomes true';
  readonly category = CommandCategory.Utility;
  readonly aliases: string[] = [];
  readonly examples = [
    'when (price >= 0.051533) (be +3%)',
    'when (price >= 0.053169) (trail 20%)',
    'when (fill) (be)',
    'when (both (price > 50000) and (pnl > 0)) (close 50%)',
  ];
  readonly requiresConnection = true;

  async execute(ctx: CommandContext): Promise<CommandResult> {
    const usage = 'Usage: when (<condition>) (<command>) — e.g. `when (price >= 0.0716) (trail 20%)`';

    if (!ctx.exchange) return this.error('when: not connected to an exchange');

    // A one-shot invocation exits the moment the command returns, so an armed
    // trigger would be destroyed before it could ever fire. Refusing is the
    // honest answer; arming and silently never firing is not.
    if (!ctx.adapter.runChain) {
      return this.error(
        'when: needs a session that stays open. A one-shot command exits before a trigger could fire — ' +
          'run it from the interactive CLI.',
      );
    }

    const rest = ctx.raw.trim().slice(this.name.length).trimStart();
    const cond = peelParenBlock(rest);
    if (!cond) {
      this.printError(ctx, usage);
      return this.error(`when: expected '(<condition>)' after 'when'`);
    }
    const action = peelParenBlock(rest.slice(cond.consumed).trimStart());
    if (!action) {
      this.printError(ctx, usage);
      return this.error(`when: expected '(<command>)' after the condition`);
    }
    const tail = rest.slice(cond.consumed).trimStart().slice(action.consumed).trim();
    if (tail.length > 0) {
      return this.error(`when: unexpected trailing tokens after the command — got '${tail}'`);
    }

    const actionText = action.body.trim();
    if (!actionText) return this.error('when: the command to run is empty');

    const condition = parseConditionText(cond.body.trim());
    if (!condition) {
      return this.error(`when: unrecognized condition '${cond.body.trim()}'`);
    }

    const symbol = ctx.symbol;
    const account = ctx.accountName;
    // Resolved at FIRE time, never closed over: switching account stops the old
    // exchange, and a trigger armed beforehand would otherwise poll a dead
    // store and send its order to an account the user has left.
    const resolveExchange = () =>
      (account && ctx.adapter.getExchange ? ctx.adapter.getExchange(account) : undefined) ?? ctx.exchange;

    let trigger;
    try {
      trigger = armTrigger({
        describe: condition.describe,
        action: actionText,
        symbol,
        account,
        resolveExchange,
        condition,
        depth: currentTriggerDepth(),
        run: async (text) => {
          // Through the HOST's chain runner, not the command executor: a fired
          // command must get the same preprocessing, aliases, chain operators
          // and write-op audit trail a typed one does.
          // Scoped to where it was ARMED, not to wherever the session has
          // drifted by the time it fires. Arming on BTCUSDT and then switching
          // symbol would otherwise move an ETHUSDT position's stop.
          await ctx.adapter.runChain!(text, { symbol, account });
        },
        onSettled: (settled) => {
          if (settled.status === 'fired') {
            ctx.adapter.printInfo(`when [${settled.id.slice(-6)}] fired: ${settled.action}`);
          } else if (settled.status === 'failed') {
            ctx.adapter.printError(`when [${settled.id.slice(-6)}] failed: ${settled.error}`);
          } else if (settled.status === 'expired') {
            ctx.adapter.printWarning(`when [${settled.id.slice(-6)}] expired without firing: ${settled.describe}`);
          }
        },
      });
    } catch (err) {
      if (err instanceof TriggerDepthExceededError) return this.error(err.message);
      throw err;
    }

    this.printSuccess(ctx, `Armed [${trigger.id.slice(-6)}] ${condition.describe} → ${actionText}`);
    this.printInfo(ctx, `\`tasks\` lists it, \`kill ${trigger.id.slice(-6)}\` drops it.`);
    return this.success();
  }
}
