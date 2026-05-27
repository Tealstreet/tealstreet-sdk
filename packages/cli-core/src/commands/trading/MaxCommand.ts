import { TradingBaseCommand } from './TradingBaseCommand';
import { CommandCategory, type CommandContext, type CommandResult } from '../Command';

/**
 * `max` — show the largest possible order size for the active symbol
 * given available balance and current leverage.
 *
 * Math: `max_notional = balance.free * leverage`,
 *       `max_contracts = max_notional / price` (rounded to amount step).
 *
 * Leverage source: open position's leverage if any, else 1x. Without
 * a position the per-symbol leverage isn't tracked in `safe-cex`'s
 * store, so 1x is the conservative default; the message flags it.
 */
export class MaxCommand extends TradingBaseCommand {
  readonly name = 'max';
  readonly description = 'Show the largest possible order size for the active symbol';
  readonly category = CommandCategory.Trading;
  readonly aliases: string[] = [];
  readonly examples = ['max', 'max ETHUSDT'];
  readonly requiresConnection = true;

  async execute(ctx: CommandContext): Promise<CommandResult> {
    const exchange = ctx.exchange;
    if (!exchange) {
      return this.error('Not connected to exchange');
    }

    const symbol = this.getTargetSymbol(ctx, []);
    const balance = exchange.store?.balance;
    if (!balance) {
      return this.error('Balance is not available yet, try again');
    }
    const ticker = exchange.store?.tickers?.find((t) => t.symbol === symbol);
    if (!ticker?.last) {
      return this.error(`Price not available for ${symbol}`);
    }
    const market = exchange.store?.markets?.find((m) => m.symbol === symbol);
    if (!market) {
      return this.error(`Market not found for ${symbol}`);
    }

    const position = (exchange.store?.positions ?? []).find((p) => p.symbol === symbol && p.contracts !== 0);
    const leverage = position?.leverage ?? 1;
    const leverageNote = position
      ? `${leverage}x (from open position)`
      : `${leverage}x (no open position — assumed; set with 'lev <n>')`;

    const maxNotional = balance.free * leverage;
    // Convert USD notional → contracts via the shared helper so inverse
    // markets (contracts are USD, not base/price) and quanto markets
    // (base-unit precision) display correctly instead of the linear
    // `notional / price` count (TSBUG-105).
    const maxContracts = this.notionalToSize(exchange, symbol, maxNotional);

    this.print(ctx, [
      `${symbol}`,
      `  Free balance: ${balance.free}`,
      `  Leverage:     ${leverageNote}`,
      `  Max notional: $${maxNotional.toFixed(2)}`,
      `  Max contracts: ${maxContracts}`,
    ]);
    return this.success();
  }
}
