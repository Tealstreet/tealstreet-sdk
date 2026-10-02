import type { ExchangeName } from '@tealstreet/safe-cex/src/types';

import { describeInstrument } from '@tealstreet/safe-cex/src/instrument-class/describe';
import { classifyMarket } from '@tealstreet/safe-cex/src/instrument-class/resolve';
import { BaseCommand } from '../BaseCommand';
import { CommandCategory, type CommandContext, type CommandResult } from '../Command';

/**
 * `check <symbol>` — says what a listed contract IS, in one line.
 *
 * The question jrnats asked in the Discord report is "is this crypto or
 * tradfi" — answered without placing an order, without a new REST call, and
 * without the classifier's warning being hidden inside an order's refusal.
 * `classifyMarket` already reads from `Market.info` on the startup cache; this
 * command surfaces that result.
 *
 * It is deliberately a READ, not an eligibility check. `bybit/CLAUDE.md` →
 * "Open feature: contract tradeability probes" is explicit that eligibility
 * is a function of calendars and the account's signed agreements, of which
 * only the first is public. `/v5/user/agreement` is the SIGNING endpoint and
 * must not be reached from a passive check — see `safe-cex/CLAUDE.md` →
 * "instrument-class/".
 */
export class CheckCommand extends BaseCommand {
  readonly name = 'check';
  readonly description = 'Say what a listed contract is a claim on (crypto / tokenized equity / ETF / commodity / FX)';
  readonly category = CommandCategory.Session;
  readonly aliases: string[] = [];
  readonly examples = ['check BTCUSDT', 'check SHOPUSDT', 'check XAUUSDT'];
  readonly requiresConnection = true;

  async execute(ctx: CommandContext): Promise<CommandResult> {
    if (!ctx.exchange) {
      return this.error('Not connected to any exchange');
    }

    const symbolArg = this.getArg(ctx, 1);
    const symbol = symbolArg ?? ctx.symbol;
    if (!symbol) {
      return this.error('Usage: check <symbol>');
    }

    const market = ctx.exchange.store?.markets?.find((m) => m.symbol === symbol);
    if (!market) {
      return this.error(`No market "${symbol}" on ${ctx.exchange.name}`);
    }

    const result = classifyMarket(ctx.exchange.name as ExchangeName, market);
    const line = describeInstrument(symbol, ctx.exchange.name, result);
    this.printInfo(ctx, line);
    return this.success();
  }
}
