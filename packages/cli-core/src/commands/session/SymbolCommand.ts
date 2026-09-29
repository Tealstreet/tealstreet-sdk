import { marketSymbolExists, resolveMarketSymbol } from '../../utils/resolveMarketSymbol';
import { BaseCommand } from '../BaseCommand';
import { CommandCategory, type CommandContext, type CommandResult } from '../Command';

/**
 * Symbol command - changes the current trading symbol
 */
export class SymbolCommand extends BaseCommand {
  readonly name = 'symbol';
  readonly description = 'Change the current trading symbol';
  readonly category = CommandCategory.Session;
  readonly aliases = ['sym', 'switch'];
  readonly examples = ['symbol BTCUSDT', 'switch ETH', 'sym BTC'];
  readonly requiresConnection = false;

  async execute(ctx: CommandContext): Promise<CommandResult> {
    const newSymbol = this.getArg(ctx, 1)?.toUpperCase();

    if (!newSymbol) {
      this.printInfo(ctx, `Current symbol: ${ctx.symbol}`);
      this.printInfo(ctx, 'Usage: symbol <symbol>');
      return this.success();
    }

    // `sym BTC` / `switch ETH` are this command's own documented examples, but
    // it used to store whatever was typed. The session symbol then didn't
    // exist on the exchange, and the failure surfaced one command later as
    // "Price data is not available yet for a symbol" — which reads as a
    // market-data outage rather than a symbol that was never real.
    const markets = ctx.exchange?.store?.markets;
    const resolved = resolveMarketSymbol(markets, newSymbol);

    // No connection yet (requiresConnection is false) means no markets to
    // resolve against — keep the old behaviour rather than rejecting input the
    // user cannot yet validate.
    if (markets?.length && !marketSymbolExists(markets, resolved)) {
      return this.error(`Unknown symbol "${newSymbol}" on ${ctx.exchange?.name ?? 'this exchange'}.`);
    }

    ctx.adapter.setDefaultSymbol(resolved);
    this.printInfo(ctx, `Symbol set to ${resolved}`);

    return {
      success: true,
      updateSession: {
        symbol: resolved,
      },
    };
  }
}
