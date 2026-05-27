import { BaseCommand } from '../BaseCommand';
import { CommandCategory, type CommandContext, type CommandResult } from '../Command';

/**
 * `fatfinger`                 — list all fatfinger limits
 * `fatfinger <contracts>`     — set max contracts on the focused symbol
 * `fatfinger <contracts> <s>` — set max contracts on a specific symbol
 *
 * `unfatfinger <symbol>`      — remove a single limit
 * `unfatfinger all`           — clear every limit
 *
 * The limit is checked in `TradingBaseCommand.enforceFatfinger`, called
 * by every sized order command (buy / sell / scale / swarm / twap /
 * chase) before placing each individual order. Per-individual matching
 * means `buy 1, repeat 11` is allowed even if the running total would
 * exceed the limit — same as ichibot.
 */
export class FatfingerCommand extends BaseCommand {
  readonly name = 'fatfinger';
  readonly description = 'Set or list per-symbol max-order-size guards';
  readonly category = CommandCategory.Utility;
  readonly aliases: string[] = [];
  readonly examples = [
    'fatfinger',
    'fatfinger 10',
    'fatfinger 0.5 BTCUSDT',
    'fatfinger 1000 ETHUSDT',
  ];
  readonly requiresConnection = false;

  async execute(ctx: CommandContext): Promise<CommandResult> {
    if (!ctx.preprocessStore) {
      return this.error('fatfinger: preprocess store unavailable');
    }
    const a1 = this.getArg(ctx, 1);

    // No args → list every set limit.
    if (!a1) {
      const all = ctx.preprocessStore.getFatfingers();
      const entries = Object.entries(all).sort(([a], [b]) => a.localeCompare(b));
      if (entries.length === 0) {
        this.printInfo(ctx, '(no fatfinger limits set)');
        return this.success();
      }
      this.print(ctx, entries.map(([sym, n]) => `  ${sym}: ${n}`));
      return this.success();
    }

    const contracts = parseFloat(a1);
    if (!Number.isFinite(contracts) || contracts <= 0) {
      this.printError(ctx, `Invalid contract size '${a1}' — must be a positive number`);
      return this.error('Invalid size');
    }
    const symbolArg = this.getArg(ctx, 2);
    const symbol = (symbolArg ?? ctx.symbol).toUpperCase();
    if (!symbol) {
      this.printError(ctx, 'No symbol — pass one explicitly or focus a symbol first');
      return this.error('No symbol');
    }
    ctx.preprocessStore.setFatfinger(symbol, contracts);
    this.printSuccess(ctx, `Fatfinger ${symbol}: ${contracts}`);
    return this.success();
  }
}

export class UnfatfingerCommand extends BaseCommand {
  readonly name = 'unfatfinger';
  readonly description = 'Remove a fatfinger limit (or `unfatfinger all`)';
  readonly category = CommandCategory.Utility;
  readonly aliases: string[] = [];
  readonly examples = ['unfatfinger BTCUSDT', 'unfatfinger all'];
  readonly requiresConnection = false;

  async execute(ctx: CommandContext): Promise<CommandResult> {
    if (!ctx.preprocessStore) {
      return this.error('unfatfinger: preprocess store unavailable');
    }
    const target = this.getArg(ctx, 1);
    if (!target) {
      this.printError(ctx, 'Usage: unfatfinger <symbol>  or  unfatfinger all');
      return this.error('Missing target');
    }

    if (target.toLowerCase() === 'all') {
      const before = Object.keys(ctx.preprocessStore.getFatfingers()).length;
      ctx.preprocessStore.clearFatfingers();
      this.printSuccess(ctx, `Cleared ${before} fatfinger limit${before === 1 ? '' : 's'}`);
      return this.success();
    }

    const symbol = target.toUpperCase();
    const removed = ctx.preprocessStore.unsetFatfinger(symbol);
    if (!removed) {
      this.printWarning(ctx, `${symbol} has no fatfinger limit`);
      return this.success();
    }
    this.printSuccess(ctx, `Removed fatfinger on ${symbol}`);
    return this.success();
  }
}
