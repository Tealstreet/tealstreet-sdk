import { TradingBaseCommand } from './TradingBaseCommand';
import { CommandCategory, type CommandContext, type CommandResult } from '../Command';

/**
 * `leverage` / `lev`            — show current leverage for the active symbol.
 * `leverage <n>` / `lev <n>`    — set leverage for the active symbol.
 *
 * Showing leverage is read from the open position if any. Most exchanges
 * don't expose the per-symbol leverage *without* a position in
 * `safe-cex`'s store, so the read path falls back to a "no open position
 * to read from" message instead of guessing. Setting always works (calls
 * `setLeverage(symbol, n)`).
 */
export class LeverageCommand extends TradingBaseCommand {
  readonly name = 'leverage';
  readonly description = 'Show or set leverage for the active symbol';
  readonly category = CommandCategory.Trading;
  readonly aliases = ['lev'];
  readonly examples = ['leverage', 'lev', 'lev 5', 'leverage 10 ETHUSDT'];
  readonly requiresConnection = true;

  async execute(ctx: CommandContext): Promise<CommandResult> {
    const exchange = ctx.exchange;
    if (!exchange) {
      return this.error('Not connected to exchange');
    }

    const targetSymbol = this.getTargetSymbol(ctx, []);
    const arg = this.getArg(ctx, 1);

    // Read path — `lev` with no argument or a non-numeric word ("lev show").
    // Anything that LOOKS like a number but isn't valid (negative, zero,
    // garbage like `5x` or `2..5`) errors loudly so `lev -5` doesn't
    // silently print the read view.
    if (!arg) {
      return this.printLeverage(ctx, targetSymbol, exchange);
    }

    // Treat any token containing a digit as a "set" attempt — that
    // way `-5`, `5x`, `2..5`, `0` all error visibly instead of being
    // mistaken for a read.
    const looksLikeSetAttempt = /\d/.test(arg);
    if (!looksLikeSetAttempt) {
      return this.printLeverage(ctx, targetSymbol, exchange);
    }

    // Set path
    if (!/^\d+(?:\.\d+)?$/.test(arg)) {
      return this.error(`Invalid leverage: '${arg}' — must be a positive number`);
    }
    const leverage = parseFloat(arg);
    if (!Number.isFinite(leverage) || leverage <= 0) {
      return this.error(`Invalid leverage: '${arg}' — must be a positive number`);
    }
    if (!exchange.setLeverage) {
      return this.error('Exchange does not support setting leverage');
    }

    // Read path (no arg) above bypassed the gate; the set path is
    // gated here so `lev` (no arg) keeps working in sim for inspection.
    if (this.simulationGate(ctx)) return this.success();

    try {
      await exchange.setLeverage(targetSymbol, leverage);
      this.printSuccess(ctx, `${targetSymbol} leverage set to ${leverage}x`);
      return this.success();
    } catch (e) {
      return this.error(`Failed to set leverage: ${(e as Error).message}`);
    }
  }

  private printLeverage(
    ctx: CommandContext,
    targetSymbol: string,
    exchange: NonNullable<CommandContext['exchange']>,
  ): CommandResult {
    const position = (exchange.store?.positions ?? []).find(
      (p) => p.symbol === targetSymbol && p.contracts !== 0,
    );
    if (!position) {
      this.printInfo(
        ctx,
        `No open position on ${targetSymbol} — current per-symbol leverage isn't tracked in the store. Use 'lev <n>' to set.`,
      );
      return this.success();
    }
    this.printInfo(ctx, `${targetSymbol}: ${position.leverage}x`);
    return this.success();
  }
}
