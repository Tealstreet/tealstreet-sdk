import { BaseCommand } from '../BaseCommand';
import { CommandCategory, type CommandContext, type CommandResult } from '../Command';
import { PositionSide } from '@tealstreet/safe-cex/src/types';
import { formatPriceCompact } from '@tealstreet/utils/src/formatting';
import { formatBaseSize } from '../../utils/formatBaseSize';

/**
 * Position command - shows current position for a symbol.
 *
 * Supports an optional `show` subcommand keyword for ichibot parity
 * (`position show` / `position show BTCUSDT`). It's a no-op modifier
 * — the focused-symbol-card format is the same with or without it.
 */
export class PositionCommand extends BaseCommand {
  readonly name = 'position';
  readonly description = 'Show position for current symbol';
  readonly category = CommandCategory.Trading;
  readonly aliases = ['p', 'pos'];
  readonly examples = ['position', 'p', 'pos btc', 'position show', 'position show BTCUSDT'];
  readonly requiresConnection = true;

  async execute(ctx: CommandContext): Promise<CommandResult> {
    const positions = ctx.exchange?.store?.positions ?? [];
    const tickers = ctx.exchange?.store?.tickers ?? [];
    const markets = ctx.exchange?.store?.markets ?? [];
    const balance = ctx.exchange?.store?.balance;

    // `position show [SYM]` — ichibot-parity no-op modifier. Strip
    // the `show` token so the symbol-detection below sees the same
    // args shape as `position [SYM]`.
    const first = this.getArg(ctx, 1);
    const symbolArgIndex = first?.toLowerCase() === 'show' ? 2 : 1;

    // Get target symbol - use arg if provided, otherwise current symbol
    let targetSymbol = ctx.symbol;
    const symbolArg = this.getArg(ctx, symbolArgIndex);
    if (symbolArg) {
      // Try to match symbol (case-insensitive, partial match)
      const upperArg = symbolArg.toUpperCase();
      const matched = markets.find(
        (m) =>
          m.symbol.toUpperCase() === upperArg ||
          m.symbol.toUpperCase().startsWith(upperArg) ||
          m.symbol.toUpperCase().includes(upperArg + 'USDT') ||
          m.symbol.toUpperCase().includes(upperArg + 'USD'),
      );
      if (matched) {
        targetSymbol = matched.symbol;
      } else {
        targetSymbol = symbolArg.toUpperCase();
      }
    }

    // Find position for symbol
    const position = positions.find((p) => p.symbol === targetSymbol && p.notional !== 0);

    if (!position) {
      this.printInfo(ctx, `No open position for ${targetSymbol}`);
      return this.success();
    }

    const ticker = tickers.find((t) => t.symbol === targetSymbol);
    const market = markets.find((m) => m.symbol === targetSymbol);
    const pricePrecision = market?.precision?.price;
    const markPrice = ticker?.mark ?? ticker?.last ?? 0;

    const sideStr = position.side === PositionSide.Long ? 'LONG' : 'SHORT';
    const pnlSign = position.unrealizedPnl >= 0 ? '+' : '';
    const pnlPercent =
      position.notional !== 0
        ? ((position.unrealizedPnl / Math.abs(position.notional)) * 100).toFixed(2)
        : '0.00';

    // Account leverage for this position
    const accountLev = balance?.total ? Math.abs(position.notional) / balance.total : 0;

    const lines: string[] = [];
    lines.push('');
    lines.push('─'.repeat(40));
    lines.push(`${targetSymbol}  ${sideStr}`);
    lines.push('─'.repeat(40));
    lines.push(`Size:   ${formatBaseSize(position.contracts, market)} (${formatPriceCompact(Math.abs(position.notional))})`);
    lines.push(`Entry:  ${formatPriceCompact(position.entryPrice, pricePrecision)} → ${formatPriceCompact(markPrice, pricePrecision)}`);
    lines.push(`uPnL:   ${pnlSign}${formatPriceCompact(position.unrealizedPnl)} (${pnlSign}${pnlPercent}%)`);
    lines.push(`Liq:    ${position.liquidationPrice > 0 ? formatPriceCompact(position.liquidationPrice, pricePrecision) : 'N/A'}`);
    lines.push(`Lev:    ${accountLev.toFixed(2)}x / ${position.leverage}x`);
    lines.push('─'.repeat(40));

    this.print(ctx, lines);
    return this.success();
  }
}
