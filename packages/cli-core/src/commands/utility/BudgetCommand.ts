import { BaseCommand } from '../BaseCommand';
import { CommandCategory, type CommandContext, type CommandResult } from '../Command';

/**
 * `budget`            — show the current cap and how much has been spent
 * `budget <usd>`      — set the aggregate notional cap (e.g. `budget $5000` or `budget 5000`)
 * `budget off`        — disable the cap (also `budget 0`)
 * `budget reset`      — reset the session "spent" accumulator to 0
 *
 * The cap is an opt-in, off-by-default cumulative limit on the total USD
 * notional of EXPOSURE-INCREASING orders placed this session (TSBUG-106).
 * Reduce-only / close / cancel don't count. Enforced per-order in
 * `TradingBaseCommand.enforceBudget`. The cap persists across sessions;
 * the spent accumulator is session-only and resets on restart, on
 * `budget reset`, and whenever the cap is changed.
 */
export class BudgetCommand extends BaseCommand {
  readonly name = 'budget';
  readonly description = 'Set or show the aggregate notional budget (cumulative order cap)';
  readonly category = CommandCategory.Utility;
  readonly aliases: string[] = [];
  readonly examples = ['budget', 'budget $5000', 'budget 5000', 'budget off', 'budget reset'];
  readonly requiresConnection = false;

  async execute(ctx: CommandContext): Promise<CommandResult> {
    const store = ctx.preprocessStore;
    if (!store) {
      return this.error('budget: preprocess store unavailable');
    }
    const a1 = this.getArg(ctx, 1)?.toLowerCase();

    // No args → show status.
    if (!a1) {
      const cap = store.getBudget();
      if (!cap) {
        this.printInfo(ctx, 'Budget: off (no aggregate cap set)');
        return this.success();
      }
      const spent = store.getBudgetSpent();
      this.printInfo(ctx, `Budget: $${spent.toFixed(2)} spent of $${cap.toFixed(2)} (this session)`);
      return this.success();
    }

    if (a1 === 'reset') {
      store.resetBudgetSpent();
      this.printSuccess(ctx, 'Budget spent reset to $0');
      return this.success();
    }

    if (a1 === 'off') {
      store.setBudget(0);
      this.printSuccess(ctx, 'Budget disabled');
      return this.success();
    }

    // `budget <usd>` — accept `$5000`, `5000$`, or bare `5000`. Require a
    // strict numeric token so `5000abc` doesn't silently parse to 5000.
    const numericPart = a1.replace(/^\$/, '').replace(/\$$/, '');
    if (!/^\d+(\.\d+)?$/.test(numericPart)) {
      this.printError(
        ctx,
        `Invalid budget '${this.getArg(ctx, 1)}' — use a USD amount (e.g. $5000), 'off', or 'reset'`,
      );
      return this.error('Invalid budget');
    }
    const usd = parseFloat(numericPart);
    if (usd === 0) {
      store.setBudget(0);
      this.printSuccess(ctx, 'Budget disabled');
      return this.success();
    }
    store.setBudget(usd);
    this.printSuccess(ctx, `Budget set to $${usd.toFixed(2)} (cumulative, this session)`);
    return this.success();
  }
}
